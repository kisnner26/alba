// prueba los lectores reales con datos truncados y mutaciones deterministas.
#![allow(dead_code)]
extern crate alloc;

#[path = "../src/elf.rs"]
mod elf;
#[path = "../src/fs.rs"]
mod fs;

mod irq {
    pub fn save() -> u64 { 0 }
    pub fn restore(_: u64) {}
}

mod sched {
    pub fn wait_fs() { panic!("el montaje de pruebas no debe bloquear") }
    pub fn wake_fs() {}
}

mod blk {
    use std::cell::RefCell;

    pub const SECTOR: usize = 512;
    thread_local! {
        pub static IMAGE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    pub fn sectors() -> u64 {
        IMAGE.with_borrow(|image| (image.len() / SECTOR) as u64)
    }

    pub fn read(sector: u64, out: &mut [u8; SECTOR]) -> bool {
        read_sectors(sector, out)
    }

    pub fn read_sectors(sector: u64, out: &mut [u8]) -> bool {
        IMAGE.with_borrow(|image| {
            let Some(start) = (sector as usize).checked_mul(SECTOR) else { return false };
            let Some(end) = start.checked_add(out.len()) else { return false };
            let Some(data) = image.get(start..end) else { return false };
            out.copy_from_slice(data);
            true
        })
    }

    pub fn can_persist() -> bool { false }
    pub fn flush() -> bool { false }
    pub fn write(_: u64, _: &[u8; SECTOR]) -> bool { false }
    pub fn write_sectors(_: u64, _: &[u8]) -> bool { false }
}

fn next(seed: &mut u32) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed as usize
}

fn mutate(data: &mut [u8], seed: &mut u32) {
    let n = 1 + next(seed) % 8;
    for _ in 0..n {
        let index = next(seed) % data.len();
        data[index] ^= 1 << (next(seed) % 8);
    }
}

fn checksum(data: &[u8]) -> u32 {
    let mut state = u32::MAX;
    for &b in data {
        state ^= u32::from(b);
        for _ in 0..8 {
            state = (state >> 1) ^ if state & 1 == 0 { 0 } else { 0xedb88320 };
        }
    }
    !state
}

fn fuzz_elf(original: &[u8], seed: &mut u32) {
    assert!(elf::parse(original).is_ok());
    let mut data = vec![0; original.len()];
    for i in 0..4096 {
        data.copy_from_slice(original);
        // cubre cabeceras y tabla antes de alterar contenido de segmentos.
        let end = if i % 3 == 0 { data.len() } else { 256 };
        mutate(&mut data[..end], seed);
        let end = if i % 5 == 0 { next(seed) % data.len() } else { data.len() };
        if let Ok(binary) = elf::parse(&data[..end]) {
            for segment in binary.segments {
                assert!(segment.offset.checked_add(segment.filesz).is_some_and(|last| last <= end));
                assert!(segment.filesz <= segment.memsz);
            }
        }
    }
}

fn fuzz_albafs(mut original: Vec<u8>, seed: &mut u32) {
    const BANK: usize = 9 * blk::SECTOR;
    let original_len = original.len();
    let metadata = original[..2 * BANK].to_vec();
    blk::IMAGE.with_borrow_mut(|image| std::mem::swap(image, &mut original));
    assert!(fs::mount().is_ok());
    assert!(fs::size(fs::open("hola.txt").unwrap()).unwrap() > 0);
    for i in 0..2048 {
        blk::IMAGE.with_borrow_mut(|image| {
            image.resize(original_len, 0);
            image[..2 * BANK].copy_from_slice(&metadata);
            if i % 3 == 0 {
                // recalcular sumas permite llegar a las comprobaciones de las entradas.
                for bank in image[..2 * BANK].chunks_exact_mut(BANK) {
                    mutate(&mut bank[512..], seed);
                    let sum = checksum(&bank[512..]);
                    bank[24..28].copy_from_slice(&sum.to_le_bytes());
                    let sum = checksum(&bank[..28]);
                    bank[28..32].copy_from_slice(&sum.to_le_bytes());
                }
            } else {
                mutate(&mut image[..2 * BANK], seed);
            }
            if i % 5 == 0 {
                image.truncate(next(seed) % original_len);
            }
        });
        if fs::mount().is_ok() {
            for index in 0..fs::count() {
                let entry = fs::list(index).unwrap();
                let mut out = [0; 12];
                assert!(fs::read_id(entry.id, 0, &mut out).is_some());
            }
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let elf = std::fs::read(args.next().unwrap()).unwrap();
    let disk = std::fs::read(args.next().unwrap()).unwrap();
    let mut seed = 0x5f4cd623;
    fuzz_elf(&elf, &mut seed);
    fuzz_albafs(disk, &mut seed);
    println!("ok: 4096 elf y 2048 albafs mutados sin pánico");
}
