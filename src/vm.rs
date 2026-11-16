// espacios de direcciones de procesos de usuario.
//
// cada proceso tiene sus propias tablas (nivel 1, 2 y 3) y un ASID. las dos primeras entradas
// del nivel 1 (dispositivos y RAM identidad) se comparten con el kernel; la tercera apunta a las
// tablas propias del proceso:
//
//   0x8000_0000  codigo y datos de solo lectura de la imagen de usuario   R-X (EL0)
//   0x8010_0000  una pagina de datos                                      RW- (EL0)
//   0x8020_0000  tope de la pila (crece hacia abajo, 2 paginas)           RW- (EL0)
//
// todo lo demas esta sin mapear: una lectura fuera de estas regiones da un fallo de pagina.
// el cargador copia la imagen embebida en el kernel (seccion .user) a paginas nuevas y limpias.

use crate::mmu::{self, AF, AP_EL0, AP_RO, NG, PAGE_DESC, PXN, TABLE, UXN};
use crate::pmm::{self, Zone, PAGE};
use alloc::vec::Vec;
use core::arch::asm;
use core::ptr::write_bytes;

pub const CODE_VA: usize = 0x8000_0000;
pub const DATA_VA: usize = 0x8010_0000;
pub const STACK_TOP: usize = 0x8020_0000;
const STACK_PAGES: usize = 2;
const MAX_CODE: usize = DATA_VA - CODE_VA;

extern "C" {
    static __user_load: u8;
    static __user_size: u8; // simbolo absoluto: su direccion es el tamano
}

/// imagen de los programas de usuario, tal como la dejo el linker dentro del kernel
fn image() -> &'static [u8] {
    unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(__user_load),
            core::ptr::addr_of!(__user_size) as usize,
        )
    }
}

// ASIDs de 8 bits; el 0 es del kernel
static mut ASIDS: [bool; 256] = [false; 256];

fn alloc_asid() -> Option<u16> {
    unsafe {
        let asids = &mut *core::ptr::addr_of_mut!(ASIDS);
        let id = (1..256).find(|&i| !asids[i])?;
        asids[id] = true;
        Some(id as u16)
    }
}

pub struct AddrSpace {
    pages: Vec<(usize, usize)>, // (direccion fisica, paginas): todo lo que hay que liberar
    code_len: usize,
    asid: u16,
    ttbr: u64,
}

fn desc(pa: usize, flags: u64) -> u64 {
    pa as u64 | PAGE_DESC | (1 << 2) | (3 << 8) | AF | NG | flags
}

fn put(table: usize, index: usize, value: u64) {
    unsafe { (table as *mut u64).add(index).write_volatile(value) };
}

impl AddrSpace {
    pub fn new() -> Option<AddrSpace> {
        let asid = alloc_asid()?;
        let mut sp = AddrSpace { pages: Vec::new(), code_len: image().len(), asid, ttbr: 0 };
        if sp.build().is_none() {
            sp.destroy();
            return None;
        }
        Some(sp)
    }

    /// reserva paginas contiguas ya puestas a cero (no se filtra contenido de otros procesos)
    fn alloc(&mut self, zone: Zone, n: usize) -> Option<usize> {
        let pa = pmm::alloc_zone(zone, n)?;
        unsafe { write_bytes(pa as *mut u8, 0, n * PAGE) };
        self.pages.push((pa, n));
        Some(pa)
    }

    fn build(&mut self) -> Option<()> {
        let img = image();
        if img.len() > MAX_CODE {
            return None;
        }
        let l1 = self.alloc(Zone::Kernel, 1)?;
        let l2 = self.alloc(Zone::Kernel, 1)?;
        let l3 = self.alloc(Zone::Kernel, 1)?;
        let (dev, ram) = mmu::kernel_l1();
        put(l1, 0, dev);
        put(l1, 1, ram);
        put(l1, 2, l2 as u64 | TABLE);
        put(l2, 0, l3 as u64 | TABLE);

        // codigo: copiado de la imagen, solo lectura y ejecutable (W^X)
        let n = img.len().div_ceil(PAGE).max(1);
        let code = self.alloc(Zone::User, n)?;
        unsafe { core::ptr::copy_nonoverlapping(img.as_ptr(), code as *mut u8, img.len()) };
        mmu::sync_icache(code, n * PAGE);
        for i in 0..n {
            put(l3, i, desc(code + i * PAGE, AP_EL0 | AP_RO | PXN));
        }

        // una pagina de datos y la pila: lectura y escritura, sin ejecucion
        let rw = AP_EL0 | PXN | UXN;
        let data = self.alloc(Zone::User, 1)?;
        put(l3, (DATA_VA - CODE_VA) / PAGE, desc(data, rw));
        let stack = self.alloc(Zone::User, STACK_PAGES)?;
        let first = (STACK_TOP - CODE_VA) / PAGE - STACK_PAGES;
        for i in 0..STACK_PAGES {
            put(l3, first + i, desc(stack + i * PAGE, rw));
        }

        unsafe { asm!("dsb ishst") }; // las tablas deben ser visibles para el recorrido de tablas
        self.ttbr = l1 as u64 | (self.asid as u64) << 48;
        Some(())
    }

    /// valor para TTBR0 (tabla base + ASID)
    pub fn ttbr(&self) -> u64 {
        self.ttbr
    }

    /// el rango [ptr, ptr+len) cae entero dentro de una region mapeada del proceso
    pub fn contains(&self, ptr: usize, len: usize) -> bool {
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        let regions = [
            (CODE_VA, self.code_len),
            (DATA_VA, PAGE),
            (STACK_TOP - STACK_PAGES * PAGE, STACK_PAGES * PAGE),
        ];
        regions.iter().any(|&(start, size)| ptr >= start && end <= start + size)
    }

    /// libera tablas, paginas y ASID. no puede estar activo en este core.
    pub fn destroy(self) {
        unsafe {
            asm!(
                "dsb ishst",
                "tlbi aside1is, {x}",
                "dsb ish",
                "isb",
                x = in(reg) (self.asid as u64) << 48,
            );
            (*core::ptr::addr_of_mut!(ASIDS))[self.asid as usize] = false;
        }
        for (pa, n) in self.pages {
            pmm::free_pages(pa, n);
        }
    }
}
