#!/usr/bin/env python3
"""crea un disco albafs (ver docs/albafs.md) con los archivos de una carpeta.

uso: mkfs.py <carpeta> <disco.img> [sectores_minimos]
"""
import os
import struct
import sys
import zlib

SECTOR = 512
ENTRY = 64
NAME_MAX = 40  # incluye el terminador
FLAG_EXEC = 1


def sectors(n):
    return -(-n // SECTOR)


def build(src, min_sectors):
    names = sorted(f for f in os.listdir(src) if os.path.isfile(os.path.join(src, f)))
    if len(names) > 64:
        sys.exit("albafs v2 admite hasta 64 archivos")
    for n in names:
        if len(n.encode()) >= NAME_MAX or "\\" in n:
            sys.exit(f"nombre demasiado largo: {n}")

    cursor = 18  # dos copias de la cabecera y el directorio
    entries = []
    blobs = []
    for number, n in enumerate(names, start=1):
        path = os.path.join(src, n)
        data = open(path, "rb").read()
        flags = FLAG_EXEC if os.access(path, os.X_OK) else 0
        entries.append(struct.pack("<40sIIIQ4x", n.encode(), cursor if data else 0, len(data), flags, number))
        blobs.append((cursor, data))
        cursor += sectors(len(data))

    total = max(cursor + 1, min_sectors)
    img = bytearray(total * SECTOR)
    directory = b"".join(entries).ljust(8 * SECTOR, b"\0")
    header = struct.pack("<8sIIQI", b"ALBAFS02", 2, total, max(1, len(entries)), zlib.crc32(directory))
    header += struct.pack("<I", zlib.crc32(header))
    for bank in (0, 9):
        off = bank * SECTOR
        img[off : off + len(header)] = header
        img[off + SECTOR : off + 9 * SECTOR] = directory
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
