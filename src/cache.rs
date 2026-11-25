// cache de sectores. cada conjunto usa lru para que buscar un sector cueste como maximo ocho comparaciones.
use crate::blk::SECTOR;

const SETS: usize = 16;
const WAYS: usize = 8;

#[derive(Clone, Copy)]
struct Entry {
    sector: u64,
    stamp: u64,
    valid: bool,
    data: [u8; SECTOR],
}

impl Entry {
    const EMPTY: Self = Self { sector: 0, stamp: 0, valid: false, data: [0; SECTOR] };
}

pub struct Cache {
    sets: [[Entry; WAYS]; SETS],
    clock: u64,
}

impl Cache {
    pub const fn new() -> Self {
        Self { sets: [[Entry::EMPTY; WAYS]; SETS], clock: 0 }
    }

    fn set(sector: u64) -> usize {
        sector as usize % SETS
    }

    fn find(&self, sector: u64) -> Option<usize> {
        self.sets[Self::set(sector)].iter().position(|entry| entry.valid && entry.sector == sector)
    }

    pub fn read(&mut self, start: u64, data: &mut [u8]) -> bool {
        for (i, chunk) in data.chunks_exact_mut(SECTOR).enumerate() {
            let sector = start + i as u64;
            let Some(slot) = self.find(sector) else { return false };
            let entry = &mut self.sets[Self::set(sector)][slot];
            self.clock += 1;
            entry.stamp = self.clock;
            chunk.copy_from_slice(&entry.data);
        }
        true
    }

    pub fn insert(&mut self, start: u64, data: &[u8]) {
        for (i, chunk) in data.chunks_exact(SECTOR).enumerate() {
            let sector = start + i as u64;
            let existing = self.find(sector);
            let set = &mut self.sets[Self::set(sector)];
            let slot = existing
                .or_else(|| set.iter().position(|entry| !entry.valid))
                .unwrap_or_else(|| (0..WAYS).min_by_key(|&slot| set[slot].stamp).unwrap());
            self.clock += 1;
            let entry = &mut set[slot];
            entry.sector = sector;
            entry.stamp = self.clock;
            entry.valid = true;
            entry.data.copy_from_slice(chunk);
        }
    }

    pub fn invalidate(&mut self, start: u64, count: u64) {
        for sector in start..start + count {
            if let Some(slot) = self.find(sector) {
                self.sets[Self::set(sector)][slot].valid = false;
            }
        }
    }
}
