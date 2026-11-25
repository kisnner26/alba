#!/usr/bin/env python3
"""comprueba el scanout virtio-gpu con una captura real de qemu."""
import json
import re
import socket
import subprocess
import sys
import tempfile
from pathlib import Path
from threading import Timer


def check(kernel, disk):
    with tempfile.TemporaryDirectory(prefix="alba-gpu-", dir="/tmp") as directory:
        control = str(Path(directory, "qmp"))
        image = Path(directory, "scanout.ppm")
        args = [
            "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72", "-m", "128M",
            "-display", "none", "-serial", "stdio", "-qmp", f"unix:{control},server=on,wait=off",
            "-global", "virtio-mmio.force-legacy=false",
            "-drive", f"file={disk},if=none,format=raw,readonly=on,id=hd0",
            "-device", "virtio-blk-device,drive=hd0", "-device", "virtio-gpu-device",
            "-kernel", kernel,
        ]
        with subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True,
        ) as qemu:
            timeout = Timer(30, qemu.kill)
            timeout.start()
            try:
                for line in qemu.stdout:
                    print(line, end="")
                    if "gpu: scanout " in line:
                        break
                else:
                    raise SystemExit("la gpu no estableció el scanout")

                with socket.socket(socket.AF_UNIX) as connection:
                    connection.settimeout(5)
                    connection.connect(control)
                    with connection.makefile("rwb") as qmp:
                        assert "QMP" in json.loads(qmp.readline())

                        def execute(command, arguments=None):
                            request = {"execute": command}
                            if arguments is not None:
                                request["arguments"] = arguments
                            qmp.write(json.dumps(request).encode() + b"\n")
                            qmp.flush()
                            while message := qmp.readline():
                                result = json.loads(message)
                                if "return" in result:
                                    return
                                if "error" in result:
                                    raise SystemExit(result["error"])
                            raise SystemExit("qmp se cerró durante la captura")

                        execute("qmp_capabilities")
                        execute("screendump", {"filename": str(image)})
                        execute("quit")
                if qemu.wait(timeout=5) != 0:
                    raise SystemExit("qemu no cerró limpiamente")
            finally:
                timeout.cancel()
                if qemu.poll() is None:
                    qemu.kill()
        ppm = image.read_bytes()
        header = re.match(rb"P6\s+(\d+)\s+(\d+)\s+255\s", ppm)
        if not header:
            raise SystemExit("la captura no es ppm de 24 bits")
        width, height = map(int, header.groups())
        pixels = ppm[header.end():]
        if height <= 32 or len(pixels) != width * height * 3:
            raise SystemExit("dimensiones inesperadas del scanout")
        if pixels[:3] != bytes.fromhex("1e2c42") or pixels[-3:] != bytes.fromhex("121927"):
            raise SystemExit("la captura no contiene las dos regiones del framebuffer")


if __name__ == "__main__":
    check(*sys.argv[1:])
    print("ok: scanout virtio-gpu verificado")
