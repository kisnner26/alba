#!/usr/bin/env python3
"""mide alba en una configuración repetible de qemu."""
import argparse
import os
import re
import subprocess
import time
from pathlib import Path
from threading import Timer

import mkfs

KERNEL = Path("target/aarch64-unknown-none-softfloat/release/alba.bin")
DISK = Path("target/alba-disk.img")
PROGRAMS = Path("user/src/bin")
DISK_CONTENT = Path("target/disk")
LIMITS = Path("tests/presupuestos.txt")
REFERENCE = os.environ.get("ALBA_PRESUPUESTOS_BASE", "HEAD")


def guest():
    args = [
        "qemu-system-aarch64", "-machine", "virt", "-cpu", "cortex-a72",
        "-accel", "tcg", "-icount", "shift=0,sleep=on", "-m", "128M",
        "-nographic", "-global", "virtio-mmio.force-legacy=false",
        "-drive", f"file={DISK},if=none,format=raw,readonly=on,id=hd0",
        "-device", "virtio-blk-device,drive=hd0", "-kernel", str(KERNEL),
    ]
    transcript = []
    with subprocess.Popen(
        args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True,
    ) as qemu:
        def expect(pattern):
            for line in qemu.stdout:
                transcript.append(line)
                match = re.search(pattern, line)
                if match:
                    return tuple(int(value) for value in match.groups())
            raise SystemExit("no se completó la medición de qemu:\n" + "".join(transcript))

        def command(name, pattern):
            qemu.stdin.write(name + "\n")
            qemu.stdin.flush()
            return expect(pattern)

        def sample():
            return command("__recursos", r"recursos: (\d+) (\d+) (\d+) (\d+) (\d+)")

        def launch():
            measured, = command("__lanzamiento", r"lanzamiento: (\d+)")
            return measured

        timeout = Timer(30, qemu.kill)
        timeout.start()
        try:
            expect(r"alba shell\.")
            first = sample()
            time.sleep(1)
            second = sample()
            cold = launch()
            warm = launch()
            qemu.stdin.write("\x01x")
            qemu.stdin.flush()
            if qemu.wait(timeout=5) != 0:
                raise SystemExit("qemu no cerró limpiamente")
        finally:
            timeout.cancel()
            if qemu.poll() is None:
                qemu.kill()

    if first[0] == 0 or cold == 0 or warm == 0 or first[4] > second[4]:
        raise SystemExit("medición del contador inválida")
    return {
        "arranque_us": first[0],
        "ram_reposo_bytes": first[1],
        "proceso_kernel_paginas": first[2],
        "proceso_usuario_paginas": first[3],
        "timer_reposo_irqs_por_segundo": second[4] - first[4],
        "lanzamiento_frio_us": cold,
        "lanzamiento_caliente_us": warm,
    }


def measure():
    result = guest()
    result["kernel_bytes"] = KERNEL.stat().st_size
    result["disco_minimo_bytes"] = len(mkfs.build(DISK_CONTENT, 0))
    for source in sorted(PROGRAMS.glob("*.rs")):
        # mide también los binarios que solo se incluyen en el disco de pruebas.
        binary = Path("user/target/aarch64-unknown-none-softfloat/release") / source.stem
        result[f"programa.{source.stem}.bytes"] = binary.stat().st_size
    return result


def budgets(text):
    # cada línea activa contiene una métrica y un límite entero en su unidad.
    limits = {}
    for line in text.splitlines():
        fields = line.split("#", 1)[0].split()
        if not fields:
            continue
        name, value = fields
        if name in limits or not value.isdecimal():
            raise SystemExit(f"presupuesto inválido: {line}")
        limits[name] = int(value)
    return limits


def check(values):
    limits = budgets(LIMITS.read_text())
    # en ci se puede usar como referencia el commit previo al conjunto de cambios.
    previous = subprocess.run(
        ["git", "show", f"{REFERENCE}:{LIMITS.as_posix()}"],
        capture_output=True, text=True,
    )
    if previous.returncode == 0:
        old = budgets(previous.stdout)
        raised = sorted(name for name in limits.keys() & old.keys() if limits[name] > old[name])
        if raised:
            raise SystemExit(f"presupuestos aumentados: {raised}")
    if values.keys() != limits.keys():
        raise SystemExit(f"métricas distintas al presupuesto: {sorted(values.keys() ^ limits.keys())}")
    over = [f"{name}: {value} > {limits[name]}" for name, value in values.items() if value > limits[name]]
    if over:
        raise SystemExit("presupuestos superados:\n" + "\n".join(over))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="falla si se supera un presupuesto")
    args = parser.parse_args()
    result = measure()
    for name, value in sorted(result.items()):
        print(f"{name:40} {value:10}")
    if args.check:
        check(result)


if __name__ == "__main__":
    main()
