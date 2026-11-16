// lector minimo de ELF64 (aarch64, ejecutable, little endian). solo interesa lo que hace
// falta para cargar un programa: el punto de entrada y los segmentos PT_LOAD.
// nada del archivo se da por bueno: cada campo se valida antes de usarse.

use alloc::vec::Vec;

const PT_LOAD: u32 = 1;
const ET_EXEC: u16 = 2;
const EM_AARCH64: u16 = 183;
const EHDR_LEN: usize = 64;
const PHDR_LEN: usize = 56;
const MAX_SEGMENTS: usize = 16;

pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;

pub struct Segment {
    pub vaddr: usize,
    pub offset: usize,
    pub filesz: usize,
    pub memsz: usize,
    pub flags: u32,
}

pub struct Elf {
    pub entry: usize,
    pub segments: Vec<Segment>,
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}
fn u64_at(d: &[u8], o: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[o..o + 8]);
    u64::from_le_bytes(b)
}

pub fn parse(d: &[u8]) -> Result<Elf, &'static str> {
    if d.len() < EHDR_LEN || &d[0..4] != b"\x7fELF" {
        return Err("no es un ELF");
    }
    if d[4] != 2 || d[5] != 1 {
        return Err("el ELF no es de 64 bits little endian");
    }
    if u16_at(d, 16) != ET_EXEC || u16_at(d, 18) != EM_AARCH64 {
        return Err("el ELF no es un ejecutable aarch64");
    }
    let entry = u64_at(d, 24) as usize;
    let phoff = u64_at(d, 32) as usize;
    let phentsize = u16_at(d, 54) as usize;
    let phnum = u16_at(d, 56) as usize;
    if phentsize != PHDR_LEN || phnum == 0 || phnum > MAX_SEGMENTS {
        return Err("tabla de segmentos invalida");
    }
    let table_end = phnum
        .checked_mul(PHDR_LEN)
        .and_then(|n| n.checked_add(phoff))
        .ok_or("tabla de segmentos invalida")?;
    if table_end > d.len() {
        return Err("tabla de segmentos fuera del archivo");
    }

    let mut segments = Vec::new();
    for i in 0..phnum {
        let h = phoff + i * PHDR_LEN;
        if u32_at(d, h) != PT_LOAD {
            continue;
        }
        let flags = u32_at(d, h + 4);
        let offset = u64_at(d, h + 8) as usize;
        let vaddr = u64_at(d, h + 16) as usize;
        let filesz = u64_at(d, h + 32) as usize;
        let memsz = u64_at(d, h + 40) as usize;
        if filesz > memsz || memsz == 0 {
            return Err("segmento con tamanos invalidos");
        }
        match offset.checked_add(filesz) {
            Some(end) if end <= d.len() => {}
            _ => return Err("segmento fuera del archivo"),
        }
        segments.push(Segment { vaddr, offset, filesz, memsz, flags });
    }
    if segments.is_empty() {
        return Err("el ELF no tiene segmentos cargables");
    }
    Ok(Elf { entry, segments })
}
