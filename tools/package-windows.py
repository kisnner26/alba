#!/usr/bin/env python3
"""empaqueta una compilacion existente para probar alba con qemu en windows."""
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

root = Path(__file__).resolve().parent.parent
files = {
    "alba.bin": root / "target/aarch64-unknown-none-softfloat/release/alba.bin",
    "alba-disk.img": root / "target/alba-disk.img",
    "iniciar-alba.cmd": root / "tools/windows/iniciar-alba.cmd",
    "LEEME.txt": root / "tools/windows/LEEME.txt",
}
out = root / "target/alba-windows-qemu.zip"
for source in files.values():
    if not source.is_file():
        raise SystemExit(f"falta {source}; ejecuta make build disk")
with ZipFile(out, "w", ZIP_DEFLATED) as archive:
    for name, source in files.items():
        archive.write(source, name)
print(out)
