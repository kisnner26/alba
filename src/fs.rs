// albafs, solo lectura (formato en docs/albafs.md). monta el disco leyendo el superbloque
// y el directorio completo a memoria; los archivos se leen sector a sector bajo demanda.

use crate::blk::{self, SECTOR};
use alloc::{string::String, vec::Vec};

const MAGIC: &[u8; 8] = b"ALBAFS01";
const ENTRY: usize = 64;
const MAX_DIR_SECTORS: usize = 64;
pub const FLAG_EXEC: u32 = 1;

pub struct Entry {
    pub name: String,
    pub start: u32,
    pub size: u32,
    pub flags: u32,
}

struct Fs {
    files: Vec<Entry>,
}

static mut FS: Option<Fs> = None;

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// monta el disco. el directorio se valida: nunca se confia en lo que dice el disco.
pub fn mount() -> Result<(), &'static str> {
    let mut sb = [0u8; SECTOR];
    if !blk::read(0, &mut sb) {
        return Err("no se pudo leer el superbloque");
    }
    if &sb[0..8] != MAGIC || le32(&sb, 8) != 1 {
        return Err("no es un disco albafs v1");
    }
    let nfiles = le32(&sb, 12) as usize;
    let dir_start = le32(&sb, 16) as u64;
    let dir_sectors = le32(&sb, 20) as usize;
    let total = le32(&sb, 24) as u64;
    if dir_sectors == 0 || dir_sectors > MAX_DIR_SECTORS || nfiles > dir_sectors * (SECTOR / ENTRY) {
        return Err("directorio invalido");
    }
    if total > blk::sectors() {
        return Err("el superbloque declara mas sectores que el disco");
    }

    let mut files = Vec::new();
    let mut sector = [0u8; SECTOR];
    for i in 0..nfiles {
        if i % (SECTOR / ENTRY) == 0 && !blk::read(dir_start + (i / (SECTOR / ENTRY)) as u64, &mut sector) {
            return Err("no se pudo leer el directorio");
        }
        let e = &sector[(i % (SECTOR / ENTRY)) * ENTRY..][..ENTRY];
        let len = e[..40].iter().position(|&c| c == 0).unwrap_or(40);
        let name = core::str::from_utf8(&e[..len]).map_err(|_| "nombre invalido")?;
        let (start, size, flags) = (le32(e, 40), le32(e, 44), le32(e, 48));
        let sectors = (size as u64).div_ceil(SECTOR as u64);
        if start as u64 + sectors > total {
            return Err("un archivo se sale del disco");
        }
        files.push(Entry { name: String::from(name), start, size, flags });
    }
    unsafe { *core::ptr::addr_of_mut!(FS) = Some(Fs { files }) };
    Ok(())
}

pub fn files() -> &'static [Entry] {
    unsafe { (*core::ptr::addr_of!(FS)).as_ref().map_or(&[], |f| &f.files[..]) }
}

pub fn find(name: &str) -> Option<&'static Entry> {
    files().iter().find(|e| e.name == name)
}

/// contenido completo de un archivo
pub fn read(name: &str) -> Option<Vec<u8>> {
    let e = find(name)?;
    let mut data = Vec::with_capacity(e.size as usize);
    let mut sector = [0u8; SECTOR];
    let mut left = e.size as usize;
    let mut s = e.start as u64;
    while left > 0 {
        if !blk::read(s, &mut sector) {
            return None;
        }
        let n = left.min(SECTOR);
        data.extend_from_slice(&sector[..n]);
        left -= n;
        s += 1;
    }
    Some(data)
}
