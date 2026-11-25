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

/// contenido completo de un archivo, leido con una sola peticion de varios sectores
pub fn read(name: &str) -> Option<Vec<u8>> {
    let e = find(name)?;
    let size = e.size as usize;
    let mut data = alloc::vec![0u8; size.div_ceil(SECTOR) * SECTOR];
    if !data.is_empty() && !blk::read_sectors(e.start as u64, &mut data) {
        return None;
    }
    data.truncate(size);
    Some(data)
}

/// lee solo los sectores que cubren el rango, agrupando los sectores completos.
pub fn read_at(entry: &Entry, offset: u64, out: &mut [u8]) -> Option<usize> {
    let count = out.len().min((entry.size as u64).saturating_sub(offset) as usize);
    if count == 0 {
        return Some(0);
    }
    let mut sector = entry.start as u64 + offset / SECTOR as u64;
    let skip = offset as usize % SECTOR;
    let mut tail = &mut out[..count];
    let mut scratch = [0; SECTOR];
    if skip != 0 {
        let n = tail.len().min(SECTOR - skip);
        if !blk::read(sector, &mut scratch) {
            return None;
        }
        tail[..n].copy_from_slice(&scratch[skip..skip + n]);
        tail = &mut tail[n..];
        sector += 1;
    }
    let full = tail.len() / SECTOR * SECTOR;
    if full != 0 && !blk::read_sectors(sector, &mut tail[..full]) {
        return None;
    }
    if full != tail.len() {
        if !blk::read(sector + (full / SECTOR) as u64, &mut scratch) {
            return None;
        }
        let partial = &mut tail[full..];
        partial.copy_from_slice(&scratch[..partial.len()]);
    }
    Some(count)
}
