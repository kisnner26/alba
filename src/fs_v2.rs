use super::{blk, le32, Entry, FLAG_EXEC, SECTOR};
use alloc::{string::String, vec, vec::Vec};

const MAGIC: &[u8; 8] = b"ALBAFS02";
const DIRECTORY: usize = 8 * SECTOR;
const SLOTS: usize = DIRECTORY / 64;
const DATA_START: u32 = 18;

pub struct Volume {
    pub slots: Vec<Option<Entry>>,
    total: u32,
    generation: u64,
    bank: u32,
    writable: bool,
}

fn crc32(buf: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in buf {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

fn load(bank: u32) -> Option<Volume> {
    let base = u64::from(bank * 9);
    let mut header = [0; SECTOR];
    if !blk::read(base, &mut header) || &header[..8] != MAGIC || le32(&header, 8) != 2 {
        return None;
    }
    let total = le32(&header, 12);
    let generation = u64::from_le_bytes(header[16..24].try_into().ok()?);
    if total <= DATA_START || u64::from(total) > blk::sectors() || generation == 0
        || crc32(&header[..28]) != le32(&header, 28) || header[32..].iter().any(|&b| b != 0)
    {
        return None;
    }
    let mut dir = vec![0; DIRECTORY];
    if !blk::read_sectors(base + 1, &mut dir) || crc32(&dir) != le32(&header, 24) {
        return None;
    }
    let mut slots: Vec<Option<Entry>> = Vec::with_capacity(SLOTS);
    for row in dir.chunks_exact(64) {
        if row.iter().all(|&b| b == 0) {
            slots.push(None);
            continue;
        }
        let len = row[..40].iter().position(|&b| b == 0)?;
        let name = core::str::from_utf8(&row[..len]).ok()?;
        let id = u64::from_le_bytes(row[52..60].try_into().ok()?);
        let (start, size, flags) = (le32(row, 40), le32(row, 44), le32(row, 48));
        let end = start.checked_add(sectors(size))?;
        if !super::valid_name(name) || row[len..40].iter().any(|&b| b != 0)
            || id == 0 || id > generation || flags & !FLAG_EXEC != 0
            || row[60..].iter().any(|&b| b != 0)
            || (size == 0 && start != 0)
            || (size != 0 && (start < DATA_START || end > total - 1))
        {
            return None;
        }
        if slots.iter().flatten().any(|e| e.name == name || e.id == id ||
            (size != 0 && e.size != 0 && start < e.start + sectors(e.size) && e.start < end))
        {
            return None;
        }
        slots.push(Some(Entry { id, name: String::from(name), start, size, flags }));
    }
    Some(Volume { slots, total, generation, bank, writable: blk::can_persist() })
}

pub fn mount() -> Result<Volume, &'static str> {
    match (load(0), load(1)) {
        (Some(a), Some(b)) if a.generation == b.generation
            && (a.total != b.total || a.slots != b.slots) => Err("copias albafs v2 en conflicto"),
        (Some(a), Some(b)) => Ok(if a.generation >= b.generation { a } else { b }),
        (Some(a), None) | (None, Some(a)) => Ok(a),
        (None, None) => Err("ninguna copia albafs v2 valida"),
    }
}

fn sectors(bytes: u32) -> u32 {
    bytes.div_ceil(SECTOR as u32)
}

impl Volume {
    // reconstruye espacio libre a partir del directorio activo en vez de persistir un bitmap.
    // solo los archivos activos reservan espacio. el commit conserva sus datos hasta publicar.
    fn extent(&self, count: u32) -> Option<u32> {
        let mut start = DATA_START;
        loop {
            let end = start.checked_add(count)?;
            if end > self.total - 1 {
                return None;
            }
            let next = self.slots.iter().flatten()
                .filter(|e| e.size != 0 && e.start < end && start < e.start + sectors(e.size))
                .map(|e| e.start + sectors(e.size))
                .max();
            match next {
                Some(next) => start = next,
                None => return Some(start),
            }
        }
    }

    fn commit(&mut self, slot: usize, replacement: Option<Entry>) -> Option<()> {
        let generation = self.generation.checked_add(1)?;
        let bank = 1 - self.bank;
        let base = u64::from(bank * 9);
        let mut dir = vec![0; DIRECTORY];
        for (i, row) in dir.chunks_exact_mut(64).enumerate() {
            let entry = if i == slot { &replacement } else { &self.slots[i] };
            if let Some(entry) = entry {
                let name = entry.name.as_bytes();
                row[..name.len()].copy_from_slice(name);
                row[40..44].copy_from_slice(&entry.start.to_le_bytes());
                row[44..48].copy_from_slice(&entry.size.to_le_bytes());
                row[48..52].copy_from_slice(&entry.flags.to_le_bytes());
                row[52..60].copy_from_slice(&entry.id.to_le_bytes());
            }
        }
        let mut header = [0; SECTOR];
        header[..8].copy_from_slice(MAGIC);
        header[8..12].copy_from_slice(&2u32.to_le_bytes());
        header[12..16].copy_from_slice(&self.total.to_le_bytes());
        header[16..24].copy_from_slice(&generation.to_le_bytes());
        header[24..28].copy_from_slice(&crc32(&dir).to_le_bytes());
        let checksum = crc32(&header[..28]);
        header[28..32].copy_from_slice(&checksum.to_le_bytes());
        if !blk::write_sectors(base + 1, &dir) || !blk::write(base, &header) || !blk::flush() {
            self.writable = false;
            return None;
        }
        self.slots[slot] = replacement;
        self.generation = generation;
        self.bank = bank;
        Some(())
    }

    pub fn create(&mut self, name: &str) -> Option<u64> {
        if !self.writable || !super::valid_name(name)
            || self.slots.iter().flatten().any(|e| e.name == name)
        {
            return None;
        }
        let slot = self.slots.iter().position(Option::is_none)?;
        let id = self.generation.checked_add(1)?;
        self.commit(slot, Some(Entry { id, name: String::from(name), start: 0, size: 0, flags: 0 }))?;
        Some(id)
    }

    pub fn append(&mut self, id: u64, data: &[u8]) -> Option<u32> {
        if !self.writable {
            return None;
        }
        let slot = self.slots.iter().position(|e| e.as_ref().is_some_and(|e| e.id == id))?;
        let mut entry = self.slots[slot].as_ref()?.clone();
        if entry.flags & FLAG_EXEC != 0 {
            return None;
        }
        if data.is_empty() {
            return Some(entry.size);
        }
        self.generation.checked_add(1)?;
        let size = entry.size.checked_add(u32::try_from(data.len()).ok()?)?;
        let start = self.extent(sectors(size))?;
        // usa un solo buffer acotado para la copia, aunque el archivo sea grande.
        let mut chunk = vec![0; 4096];
        let mut offset = 0u32;
        while offset < size {
            let n = (size - offset).min(chunk.len() as u32) as usize;
            let old = (entry.size.saturating_sub(offset) as usize).min(n);
            if old != 0 {
                super::read_at(&entry, u64::from(offset), &mut chunk[..old])?;
            }
            if old < n {
                let from = (offset + old as u32 - entry.size) as usize;
                chunk[old..n].copy_from_slice(&data[from..from + n - old]);
            }
            let padded = n.div_ceil(SECTOR) * SECTOR;
            chunk[n..padded].fill(0);
            if !blk::write_sectors(u64::from(start) + u64::from(offset) / SECTOR as u64, &chunk[..padded]) {
                self.writable = false;
                return None;
            }
            offset += n as u32;
        }
        if !blk::flush() {
            self.writable = false;
            return None;
        }
        entry.start = start;
        entry.size = size;
        self.commit(slot, Some(entry))?;
        Some(size)
    }

    pub fn unlink(&mut self, name: &str) -> Option<()> {
        if !self.writable {
            return None;
        }
        let slot = self.slots.iter().position(|e| e.as_ref().is_some_and(|e| e.name == name))?;
        self.commit(slot, None)
    }
}
