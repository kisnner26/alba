#!/usr/bin/env python3
"""comprueba el scanout virtio-gpu con una captura real de qemu."""
import json
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from threading import Timer


def check(kernel, disk, mode="normal"):
    with tempfile.TemporaryDirectory(prefix="alba-gpu-", dir="/tmp") as directory:
        control = str(Path(directory, "qmp"))
        image = Path(directory, "scanout.ppm")
        writable = Path(directory, "escritorio.img")
        shutil.copyfile(disk, writable)
        args = [
            "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72", "-m", "128M",
            "-display", "none", "-serial", "stdio", "-qmp", f"unix:{control},server=on,wait=off",
            "-global", "virtio-mmio.force-legacy=false",
            "-drive", f"file={writable},if=none,format=raw,id=hd0",
            "-device", "virtio-blk-device,drive=hd0", "-device", "virtio-gpu-device",
            "-device", "virtio-keyboard-device", "-device", "virtio-tablet-device",
            "-kernel", kernel,
        ]
        with subprocess.Popen(
            args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True,
        ) as qemu:
            timeout = Timer(30, qemu.kill)
            timeout.start()
            try:
                def expect(pattern):
                    for line in qemu.stdout:
                        print(line, end="")
                        if match := re.search(pattern, line):
                            return match
                    raise SystemExit(f"no apareció {pattern} en el arranque gráfico")

                def timer_irqs():
                    qemu.stdin.write("uptime\n")
                    qemu.stdin.flush()
                    return int(expect(r"interrupciones de timer: (\d+)").group(1))

                if mode == "panic":
                    expect(r"panic:")
                    expect(r"fallo de prueba de alba")
                    expect(r"sp=0x[0-9a-f]+ lr=0x[0-9a-f]+")
                    expect("gpu: fallo visible")
                else:
                    expect("desktop: listo")
                    before = timer_irqs()
                    time.sleep(1)
                    if timer_irqs() != before:
                        raise SystemExit("el escritorio estático despertó al temporizador")

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
                        if mode != "panic":
                            events = [
                                {"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": "a"}}}
                                for down in (True, False)
                            ]
                            execute("input-send-event", {"events": events})
                            expect(r"entrada: tecla")
                            execute("input-send-event", {"events": [
                                {"type": "abs", "data": {"axis": "x", "value": 20000}},
                                {"type": "abs", "data": {"axis": "y", "value": 10000}},
                            ]})
                            expect(r"entrada: puntero")

                            def position(x, y):
                                execute("input-send-event", {"events": [
                                    {"type": "abs", "data": {"axis": "x", "value": x}},
                                    {"type": "abs", "data": {"axis": "y", "value": y}},
                                ]})

                            def button(down):
                                execute("input-send-event", {"events": [
                                    {"type": "btn", "data": {"down": down, "button": "left"}},
                                ]})

                            position(15500, 30200)
                            button(True)
                            expect(r"entrada: dock")
                            button(False)

                            position(16000, 12200)
                            button(True)
                            position(18300, 14000)
                            expect(r"entrada: ventana")
                            button(False)
                            for code in ("f3", "g", "a", "l", "f4"):
                                execute("input-send-event", {"events": [
                                    {"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": code}}}
                                    for down in (True, False)
                                ]})
                            expect(r"notas: guardado")
                            qemu.stdin.write("cat notas.txt\necho fin\n")
                            qemu.stdin.flush()
                            expect(r"galalba>")
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
        if mode == "panic":
            if pixels[width * 10 * 3:width * 10 * 3 + 3] != bytes.fromhex("ea786d"):
                raise SystemExit("la captura no reflejó la pantalla de fallo")
            if pixels[width * 80 * 3:width * 150 * 3].count(bytes.fromhex("f1f2f6")) < 5:
                raise SystemExit("la pantalla de fallo no mostró texto")
            return
        dot = (15 * width + 22) * 3
        if pixels[:3] != bytes.fromhex("171c28") or pixels[dot:dot + 3] != bytes.fromhex("5d95ff"):
            raise SystemExit("la captura no refleja la barra y su actualización parcial")



if __name__ == "__main__":
    check(*sys.argv[1:])
    print("ok: pantalla de fallo verificada" if sys.argv[-1] == "panic" else "ok: scanout virtio-gpu verificado")
