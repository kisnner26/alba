#!/usr/bin/env python3
"""crea un disco albafs (ver docs/albafs.md) con los archivos de una carpeta.

uso: mkfs.py <carpeta> <disco.img> [sectores_minimos]
"""
import os
import struct
import sys

SECTOR = 512
ENTRY = 64
NAME_MAX = 40  # incluye el terminador
FLAG_EXEC = 1


def sectors(n):
    return -(-n // SECTOR)


def build(src, min_sectors):
    names = sorted(f for f in os.listdir(src) if os.path.isfile(os.path.join(src, f)))
    for n in names:
        if len(n.encode()) >= NAME_MAX:
            sys.exit(f"nombre demasiado largo: {n}")

    dir_sectors = max(1, sectors(len(names) * ENTRY))
    cursor = 1 + dir_sectors  # sector 0 = superbloque, luego el directorio
    entries = []
    blobs = []
    for n in names:
        path = os.path.join(src, n)
        data = open(path, "rb").read()
        flags = FLAG_EXEC if os.access(path, os.X_OK) else 0
        entries.append(struct.pack("<40sIII12x", n.encode(), cursor, len(data), flags))
        blobs.append((cursor, data))
        cursor += sectors(len(data))

    total = max(cursor, min_sectors)
    img = bytearray(total * SECTOR)
    sb = struct.pack("<8sIIIII", b"ALBAFS01", 1, len(names), 1, dir_sectors, total)
    img[0 : len(sb)] = sb
    for i, e in enumerate(entries):
        off = SECTOR + i * ENTRY
        img[off : off + ENTRY] = e
    for start, data in blobs:
        img[start * SECTOR : start * SECTOR + len(data)] = data
    return img


def main():
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    min_sectors = int(sys.argv[3]) if len(sys.argv) == 4 else 2048  # 1 MiB
    open(sys.argv[2], "wb").write(build(sys.argv[1], min_sectors))


if __name__ == "__main__":
    main()
