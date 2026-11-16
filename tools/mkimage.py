#!/usr/bin/env python3
"""genera la imagen plana de alba a partir del ELF.

lee los segmentos PT_LOAD y los coloca segun su direccion fisica, rellenando
con ceros los huecos. el resultado empieza con la cabecera de imagen arm64
(ver _start en src/main.rs), asi que QEMU y los firmwares reales la reconocen.

uso: mkimage.py <alba.elf> <alba.bin>
"""
import struct
import sys

PT_LOAD = 1


def load_segments(elf):
    if elf[:4] != b"\x7fELF" or elf[4] != 2 or elf[5] != 1:
        sys.exit("no es un ELF64 little endian")
    (phoff,) = struct.unpack_from("<Q", elf, 0x20)
    phentsize, phnum = struct.unpack_from("<HH", elf, 0x36)
    segs = []
    for i in range(phnum):
        ptype, _flags, offset, _vaddr, paddr, filesz, _memsz, _align = struct.unpack_from(
            "<IIQQQQQQ", elf, phoff + i * phentsize
        )
        if ptype == PT_LOAD and filesz > 0:
            segs.append((paddr, offset, filesz))
    if not segs:
        sys.exit("el ELF no tiene segmentos cargables")
    return segs


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    elf = open(sys.argv[1], "rb").read()
    segs = load_segments(elf)
    base = min(p for p, _, _ in segs)
    end = max(p + n for p, _, n in segs)
    image = bytearray(end - base)
    for paddr, offset, size in segs:
        image[paddr - base : paddr - base + size] = elf[offset : offset + size]
    open(sys.argv[2], "wb").write(image)


if __name__ == "__main__":
    main()
