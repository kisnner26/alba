#!/usr/bin/env python3
"""lanza procesos desde la shell y comprueba páginas libres tras terminar."""
import argparse
import re
import subprocess
from threading import Timer


def check(kernel, disk, seconds):
    cmd = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72", "-m", "128M",
        "-nographic", "-global", "virtio-mmio.force-legacy=false",
        "-drive", f"file={disk},if=none,format=raw,readonly=on,id=hd0",
        "-device", "virtio-blk-device,drive=hd0", "-kernel", kernel,
    ]
    with subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, text=True) as qemu:
        timer = Timer(seconds + 90, qemu.kill)
        timer.start()
        try:
            for line in qemu.stdout:
                print(line, end="", flush=True)
                if "panic:" in line or "stress: fallo" in line or "stress: paginas libres" in line:
                    raise SystemExit("falló la prueba de resistencia")
                if "alba shell." in line:
                    qemu.stdin.write(f"stress {seconds}\n")
                    qemu.stdin.flush()
                match = re.search(r"stress: (\d+) procesos sin fugas de paginas", line)
                if match and int(match[1]) >= 1000:
                    qemu.stdin.write("\x01x")
                    qemu.stdin.flush()
                    if qemu.wait(timeout=5) == 0:
                        return
                    break
            raise SystemExit("qemu terminó sin verificar la prueba de resistencia")
        finally:
            timer.cancel()
            if qemu.poll() is None:
                qemu.kill()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel")
    parser.add_argument("disk")
    parser.add_argument("--seconds", type=int, default=0)
    opts = parser.parse_args()
    check(opts.kernel, opts.disk, opts.seconds)
    print("ok: reutilización de memoria y asid verificada")
