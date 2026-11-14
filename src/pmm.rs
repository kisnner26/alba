// gestor de memoria fisica: bitmap de paginas de 4 KiB (1 = ocupada).
// admite rangos contiguos, que se necesitan para buffers de DMA y el framebuffer.
// por ahora asume 128 MiB de RAM (-m 128M en QEMU); luego se lee del DTB.

use core::ptr::{addr_of, addr_of_mut};

pub const PAGE: usize = 4096;
const RAM_BASE: usize = 0x4000_0000;
const RAM_SIZE: usize = 128 << 20;
const NPAGES: usize = RAM_SIZE / PAGE;

extern "C" {
    static __stack_top: u8;
}

static mut BITMAP: [u64; NPAGES / 64] = [!0; NPAGES / 64];
static mut HINT: usize = 0;
static mut FREE: usize = 0;

unsafe fn bitmap() -> &'static mut [u64; NPAGES / 64] {
    &mut *addr_of_mut!(BITMAP)
}

unsafe fn used(p: usize) -> bool {
    bitmap()[p / 64] >> (p % 64) & 1 == 1
}

unsafe fn set(p: usize, v: bool) {
    if v {
        bitmap()[p / 64] |= 1 << (p % 64);
    } else {
        bitmap()[p / 64] &= !(1 << (p % 64));
    }
}

pub fn init() {
    unsafe {
        let end = (addr_of!(__stack_top) as usize + PAGE - 1) & !(PAGE - 1);
        let first = (end - RAM_BASE) / PAGE;
        for p in first..NPAGES {
            set(p, false);
        }
        FREE = NPAGES - first;
        HINT = first;
    }
}

/// reserva `n` paginas contiguas; devuelve la direccion fisica (= virtual, mapeo identidad)
pub fn alloc_pages(n: usize) -> Option<usize> {
    unsafe {
        let mut run = 0;
        let mut start = 0;
        for off in 0..NPAGES {
            let p = (HINT + off) % NPAGES;
            if p == 0 {
                run = 0;
            }
            if used(p) {
                run = 0;
                continue;
            }
            if run == 0 {
                start = p;
            }
            run += 1;
            if run == n {
                for q in start..start + n {
                    set(q, true);
                }
                FREE -= n;
                HINT = (start + n) % NPAGES;
                return Some(RAM_BASE + start * PAGE);
            }
        }
        None
    }
}

pub fn free_pages(addr: usize, n: usize) {
    unsafe {
        let start = (addr - RAM_BASE) / PAGE;
        for q in start..start + n {
            set(q, false);
        }
        FREE += n;
        if start < HINT {
            HINT = start;
        }
    }
}

pub fn free_kib() -> usize {
    unsafe { FREE * PAGE / 1024 }
}

pub fn total_kib() -> usize {
    RAM_SIZE / 1024
}
