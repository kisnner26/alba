#!/usr/bin/env bash
# genera, extrae y prueba la imagen incluida en el zip.
set -euo pipefail
cd "$(dirname "$0")/.."

make windows

python3 - <<'PY'
import hashlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from threading import Timer
from zipfile import ZipFile


def boot(folder, commands, marker):
    args = [
        "qemu-system-aarch64", "-machine", "virt", "-accel", "tcg",
        "-cpu", "cortex-a72", "-m", "128M", "-display", "none",
        "-chardev", "stdio,id=alba,mux=on,signal=off,logfile=alba.log,logappend=off",
        "-serial", "chardev:alba", "-monitor", "none",
        "-global", "virtio-mmio.force-legacy=false",
        "-drive", "file=alba-disk.img,if=none,format=raw,id=hd0",
        "-device", "virtio-blk-device,drive=hd0", "-kernel", "alba.bin",
    ]
    started = time.perf_counter()
    with subprocess.Popen(
        args, cwd=folder, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    ) as qemu:
        def expect(pattern):
            for line in qemu.stdout:
                print(line.decode(errors="replace"), end="")
                if re.search(pattern, line):
                    return
            raise SystemExit(f"qemu terminó sin mostrar {pattern!r}")

        timeout = Timer(30, qemu.kill)
        timeout.start()
        try:
            expect(rb"alba shell\.")
            print(f"arranque a consola tcg: {time.perf_counter() - started:.3f} s")
            qemu.stdin.write(commands)
            qemu.stdin.flush()
            expect(marker)
            qemu.stdin.write(b"\x01x")
            qemu.stdin.flush()
            if qemu.wait(timeout=5) != 0:
                raise SystemExit("qemu no cerró limpiamente")
        finally:
            timeout.cancel()
            if qemu.poll() is None:
                qemu.kill()

    log = (folder / "alba.log").read_bytes()
    if not re.search(marker, log, re.MULTILINE):
        raise SystemExit("falta la salida de alba en el registro")
    if commands.startswith(b"ls") and not re.search(rb"hola\.txt +28 bytes", log):
        raise SystemExit("ls no mostró el archivo de prueba")


with tempfile.TemporaryDirectory(prefix="alba paquete ") as directory:
    folder = Path(directory)
    with ZipFile("target/alba-v0.1-windows-qemu.zip") as archive:
        expected = ["LEEME.txt", "alba-fabrica.img", "alba.bin", "iniciar-alba.cmd", "restaurar-disco.cmd"]
        if sorted(archive.namelist()) != expected or archive.testzip() is not None:
            raise SystemExit("archivos incorrectos o dañados en el zip")
        for name in ("iniciar-alba.cmd", "restaurar-disco.cmd", "LEEME.txt"):
            data = archive.read(name)
            if b"\r\n" not in data or re.search(rb"(?<!\r)\n|\r(?!\n)", data):
                raise SystemExit(f"{name}: se esperaban saltos crlf")
        archive.extractall(folder)

    seed = folder / "alba-fabrica.img"
    disk = folder / "alba-disk.img"
    before = hashlib.sha256(seed.read_bytes()).digest()
    shutil.copyfile(seed, disk)
    boot(folder, b"ls\ncp hola.txt persistente.txt\ncat persistente.txt\n",
         rb"^(alba> )*hola desde el disco de alba\r?$")
    boot(folder, b"cat persistente.txt\n", rb"^(alba> )*hola desde el disco de alba\r?$")

    # reproduce el reemplazo sobre una copia del disco en el paquete extraído.
    shutil.copyfile(seed, disk)
    boot(folder, b"cat persistente.txt\n", rb"cat: no se pudo abrir: persistente.txt")
    if before != hashlib.sha256(seed.read_bytes()).digest():
        raise SystemExit("se modificó la imagen de fábrica")
    subprocess.run([sys.executable, "tests/gpu.py", str(folder / "alba.bin"), str(seed)], check=True)
    print("ok: crlf, escritorio, registro, persistencia y restauración del paquete")
PY
