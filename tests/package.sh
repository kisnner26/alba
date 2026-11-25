#!/usr/bin/env bash
# prueba el contenido del zip y arranca solamente los archivos extraidos.
set -euo pipefail
cd "$(dirname "$0")/.."

make windows

python3 - <<'PY'
import hashlib
import re
import subprocess
import tempfile
from pathlib import Path
from threading import Timer
from zipfile import ZipFile

with tempfile.TemporaryDirectory(prefix="alba paquete ") as directory:
    folder = Path(directory)
    with ZipFile("target/alba-windows-qemu.zip") as archive:
        expected = ["LEEME.txt", "alba-disk.img", "alba.bin", "iniciar-alba.cmd"]
        if sorted(archive.namelist()) != expected or archive.testzip() is not None:
            raise SystemExit("archivos incorrectos o dañados en el zip")
        for name in ("iniciar-alba.cmd", "LEEME.txt"):
            data = archive.read(name)
            if b"\r\n" not in data or re.search(rb"(?<!\r)\n|\r(?!\n)", data):
                raise SystemExit(f"{name}: se esperaban saltos crlf")
        archive.extractall(folder)

    disk = folder / "alba-disk.img"
    before = hashlib.sha256(disk.read_bytes()).digest()
    # mismos dispositivos y consola que el lanzador con --log.
    args = [
        "qemu-system-aarch64", "-machine", "virt", "-accel", "tcg",
        "-cpu", "cortex-a72", "-m", "128M", "-display", "none",
        "-chardev", "stdio,id=alba,mux=on,signal=off,logfile=alba.log,logappend=off",
        "-serial", "chardev:alba", "-monitor", "none",
        "-global", "virtio-mmio.force-legacy=false",
        "-drive", "file=alba-disk.img,if=none,format=raw,readonly=on,id=hd0",
        "-device", "virtio-blk-device,drive=hd0", "-kernel", "alba.bin",
    ]
    with subprocess.Popen(
        args, cwd=folder, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    ) as qemu:
        def read_until(pattern):
            for line in qemu.stdout:
                if re.match(pattern, line):
                    return
                print(line.decode(errors="replace"), end="")
            raise SystemExit(f"qemu termino sin mostrar {pattern!r}")

        # el limite cubre las lecturas bloqueantes de la salida de qemu.
        timeout = Timer(30, qemu.kill)
        timeout.start()
        try:
            read_until(rb"alba shell\.")
            qemu.stdin.write(b"ls\n")
            qemu.stdin.flush()
            read_until(rb"^(alba> )?hola\.txt +28 bytes")
            qemu.stdin.write(b"\x01x")
            qemu.stdin.flush()
            if qemu.wait(timeout=5) != 0:
                raise SystemExit("qemu no cerro limpiamente")
        finally:
            timeout.cancel()
            if qemu.poll() is None:
                qemu.kill()

    log = (folder / "alba.log").read_bytes()
    if b"alba shell." not in log or not re.search(rb"hola\.txt +28 bytes", log):
        raise SystemExit("falta la salida de alba en el registro")
    if before != hashlib.sha256(disk.read_bytes()).digest():
        raise SystemExit("el disco empaquetado fue modificado")
    print("ok: crlf, arranque, ls, registro y disco intacto")
PY
