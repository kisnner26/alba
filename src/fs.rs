// albafs. v1 se monta en solo lectura, v2 usa copias alternas del directorio.

use crate::blk::{self, SECTOR};
use alloc::{string::String, vec::Vec};

#[path = "fs_v2.rs"]
mod v2;

const MAGIC: &[u8; 8] = b"ALBAFS01";
const ENTRY: usize = 64;
const MAX_DIR_SECTORS: usize = 64;
pub const FLAG_EXEC: u32 = 1;

#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub name: String,
    pub start: u32,
    pub size: u32,
    pub flags: u32,
}

enum Fs {
    V1(Vec<Option<Entry>>),
    V2(v2::Volume),
}

impl Fs {
    fn slots(&self) -> &[Option<Entry>] {
        match self {
            Self::V1(files) => files,
            Self::V2(volume) => &volume.slots,
        }
    }
}

static mut FS: Option<Fs> = None;
static mut FS_BUSY: bool = false;
static mut FS_WAITERS: bool = false;

fn with_fs<T>(op: impl FnOnce(&mut Fs) -> T) -> Option<T> {
    let g = crate::irq::save();
    // un solo core. si el propietario se bloquea por el disco, los demas esperan su turno.
    while unsafe { FS_BUSY } {
        unsafe { FS_WAITERS = true };
        crate::sched::wait_fs();
    }
    // la reserva protege el estado hasta terminar, incluso durante una espera de disco.
    unsafe { FS_BUSY = true };
    let result = unsafe { (*core::ptr::addr_of_mut!(FS)).as_mut() }.map(op);
    unsafe { FS_BUSY = false };
    if unsafe { FS_WAITERS } {
        unsafe { FS_WAITERS = false };
        crate::sched::wake_fs();
    }
    crate::irq::restore(g);
    result
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() < 40 && !name.bytes().any(|b| matches!(b, 0 | b'/' | b'\\'))
}

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// monta el disco. el directorio se valida: nunca se confia en lo que dice el disco.
pub fn mount() -> Result<(), &'static str> {
    let mut sb = [0u8; SECTOR];
    let fs = if blk::read(0, &mut sb) && &sb[..8] == MAGIC && le32(&sb, 8) == 1 {
        Fs::V1(mount_v1(&sb)?)
    } else {
        Fs::V2(v2::mount()?)
    };
    // se monta una sola vez, antes de arrancar el planificador.
    unsafe { *core::ptr::addr_of_mut!(FS) = Some(fs) };
    Ok(())
}

fn mount_v1(sb: &[u8]) -> Result<Vec<Option<Entry>>, &'static str> {
    let nfiles = le32(sb, 12) as usize;
    let dir_start = le32(sb, 16) as u64;
    let dir_sectors = le32(sb, 20) as usize;
    let total = le32(sb, 24) as u64;
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
        files.push(Some(Entry { id: (i + 1) as u64, name: String::from(name), start, size, flags }));
    }
    Ok(files)
}

pub fn count() -> usize {
    with_fs(|fs| fs.slots().iter().flatten().count()).unwrap_or(0)
}

pub fn list(index: usize) -> Option<Entry> {
    with_fs(|fs| fs.slots().iter().flatten().nth(index).cloned()).flatten()
}

pub fn open(name: &str) -> Option<u64> {
    with_fs(|fs| fs.slots().iter().flatten().find(|e| e.name == name).map(|e| e.id)).flatten()
}

pub fn size(id: u64) -> Option<u32> {
    with_fs(|fs| fs.slots().iter().flatten().find(|e| e.id == id).map(|e| e.size)).flatten()
}

pub fn read_id(id: u64, offset: u64, out: &mut [u8]) -> Option<usize> {
    with_fs(|fs| {
        let entry = fs.slots().iter().flatten().find(|e| e.id == id)?;
        read_at(entry, offset, out)
    }).flatten()
}

pub fn read_named(name: &str, offset: u64, out: &mut [u8]) -> Option<usize> {
    with_fs(|fs| {
        let entry = fs.slots().iter().flatten().find(|e| e.name == name)?;
        read_at(entry, offset, out)
    }).flatten()
}

pub fn read_exec(name: &str) -> Result<Vec<u8>, &'static str> {
    with_fs(|fs| {
        let e = fs.slots().iter().flatten().find(|e| e.name == name).ok_or("no existe")?;
        if e.flags & FLAG_EXEC == 0 {
            return Err("no es ejecutable");
        }
        let size = e.size as usize;
        let mut data = alloc::vec![0u8; size.div_ceil(SECTOR) * SECTOR];
        if !data.is_empty() && !blk::read_sectors(e.start as u64, &mut data) {
            return Err("error de lectura");
        }
        data.truncate(size);
        Ok(data)
    }).unwrap_or(Err("no existe"))
}

pub fn create(name: &str) -> Option<()> {
    with_fs(|fs| match fs {
        Fs::V2(volume) => volume.create(name).map(|_| ()),
        Fs::V1(_) => None,
    }).flatten()
}

pub fn append(id: u64, data: &[u8]) -> Option<u32> {
    with_fs(|fs| match fs {
        Fs::V2(volume) => volume.append(id, data),
        Fs::V1(_) => None,
    }).flatten()
}

pub fn save_text(name: &str, data: &[u8]) -> Option<()> {
    with_fs(|fs| match fs {
        Fs::V2(volume) => volume.save_text(name, data),
        Fs::V1(_) => None,
    }).flatten()
}

pub fn rename(from: &str, to: &str) -> Option<()> {
    with_fs(|fs| match fs {
        Fs::V2(volume) => volume.rename(from, to),
        Fs::V1(_) => None,
    }).flatten()
}

pub fn unlink(name: &str) -> Option<()> {
    with_fs(|fs| match fs {
        Fs::V2(volume) => volume.unlink(name),
        Fs::V1(_) => None,
    }).flatten()
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
