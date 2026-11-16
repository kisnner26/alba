// gestor de memoria fisica: bitmap de paginas de 4 KiB (1 = ocupada).
// admite rangos contiguos, que se necesitan para buffers de DMA y el framebuffer.
// dos zonas: kernel (solo EL1) y usuario (accesible desde EL0), segun el mapa de mmu.rs.
// por ahora asume 128 MiB de RAM (-m 128M en QEMU); luego se lee del DTB.

use crate::mmu::{KERNEL_ZONE_END, RAM_BASE, RAM_END};
use core::ptr::{addr_of, addr_of_mut};

pub const PAGE: usize = 4096;
const NPAGES: usize = (RAM_END - RAM_BASE) / PAGE;

#[derive(Clone, Copy)]
pub enum Zone {
    Kernel = 0,
    User = 1,
}

extern "C" {
    static __stack_top: u8;
    static __user_end: u8;
}

struct ZoneState {
    lo: usize, // paginas [lo, hi)
    hi: usize,
    hint: usize,
}

static mut BITMAP: [u64; NPAGES / 64] = [!0; NPAGES / 64];
static mut ZONES: [ZoneState; 2] = [
    ZoneState { lo: 0, hi: 0, hint: 0 },
    ZoneState { lo: 0, hi: 0, hint: 0 },
];
static mut FREE: usize = 0;

unsafe fn bitmap() -> &'static mut [u64; NPAGES / 64] {
    &mut *addr_of_mut!(BITMAP)
}

unsafe fn zones() -> &'static mut [ZoneState; 2] {
    &mut *addr_of_mut!(ZONES)
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

fn page_of(addr: usize) -> usize {
    (addr - RAM_BASE) / PAGE
}

fn align_up(a: usize) -> usize {
    (a + PAGE - 1) & !(PAGE - 1)
}

pub fn init() {
    unsafe {
        let kfirst = page_of(align_up(addr_of!(__stack_top) as usize));
        let kend = page_of(KERNEL_ZONE_END);
        let ufirst = page_of(align_up(addr_of!(__user_end) as usize));
        let z = zones();
        z[0] = ZoneState { lo: kfirst, hi: kend, hint: kfirst };
        z[1] = ZoneState { lo: ufirst, hi: NPAGES, hint: ufirst };
        for zone in z.iter() {
            for p in zone.lo..zone.hi {
                set(p, false);
            }
            FREE += zone.hi - zone.lo;
        }
    }
}

/// reserva `n` paginas contiguas en la zona; devuelve la direccion fisica (= virtual)
pub fn alloc_zone(zone: Zone, n: usize) -> Option<usize> {
    unsafe {
        let z = &mut zones()[zone as usize];
        let len = z.hi - z.lo;
        let mut run = 0;
        let mut start = 0;
        for off in 0..len {
            let p = z.lo + (z.hint - z.lo + off) % len;
            if p == z.lo {
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
                z.hint = if start + n < z.hi { start + n } else { z.lo };
                return Some(RAM_BASE + start * PAGE);
            }
        }
        None
    }
}

/// paginas del kernel (heap, pilas)
pub fn alloc_pages(n: usize) -> Option<usize> {
    alloc_zone(Zone::Kernel, n)
}

pub fn free_pages(addr: usize, n: usize) {
    unsafe {
        let start = page_of(addr);
        for q in start..start + n {
            set(q, false);
        }
        FREE += n;
        for z in zones().iter_mut() {
            if start >= z.lo && start < z.hi && start < z.hint {
                z.hint = start;
            }
        }
    }
}

pub fn free_kib() -> usize {
    unsafe { FREE * PAGE / 1024 }
}

pub fn total_kib() -> usize {
    (RAM_END - RAM_BASE) / 1024
}
