// espacios de direcciones de procesos de usuario y cargador de ejecutables ELF.
//
// cada proceso tiene sus propias tablas (nivel 1, 2 y 3) y un ASID. las dos primeras entradas
// del nivel 1 (dispositivos y RAM identidad) se comparten con el kernel; la tercera apunta a las
// tablas propias del proceso, que cubren 2 MiB:
//
//   0x8000_0000 .. 0x801f_c000   segmentos del programa, cada uno con sus permisos (W^X)
//   0x801f_c000                  pagina de guarda (sin mapear)
//   0x801f_e000 .. 0x8020_0000   pila (2 paginas, RW sin ejecucion); el tope es la direccion inicial
//
// todo lo demas esta sin mapear: una lectura fuera de estas regiones da un fallo de pagina.
// el cargador valida el ELF, copia cada segmento a paginas nuevas y puestas a cero (lo que
// sobra de memsz sobre filesz, el .bss, queda en ceros) y rechaza segmentos que sean a la vez
// escribibles y ejecutables.

use crate::elf::{self, Elf, PF_W, PF_X};
use crate::mmu::{self, AF, AP_EL0, AP_RO, NG, PAGE_DESC, PXN, TABLE, UXN};
use crate::pmm::{self, Zone, PAGE};
use alloc::vec::Vec;
use core::arch::asm;
use core::ptr::write_bytes;

pub const CODE_VA: usize = 0x8000_0000;
pub const STACK_TOP: usize = 0x8020_0000;
const STACK_PAGES: usize = 2;
const STACK_BOTTOM: usize = STACK_TOP - STACK_PAGES * PAGE;
/// los segmentos deben acabar antes de la pagina de guarda que precede a la pila
const SEGMENTS_END: usize = STACK_BOTTOM - PAGE;
const SPAN_PAGES: usize = (STACK_TOP - CODE_VA) / PAGE; // 512: lo que cubre un nivel 3

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
    pages: Vec<(usize, usize)>,   // (direccion fisica, paginas): todo lo que hay que liberar
    regions: Vec<(usize, usize, bool)>, // (direccion virtual, bytes, escribible) mapeados: para validar punteros
    asid: u16,
    ttbr: u64,
    stack_pa: usize, // pagina fisica mas baja de la pila de usuario
}

fn desc(pa: usize, flags: u64) -> u64 {
    pa as u64 | PAGE_DESC | (1 << 2) | (3 << 8) | AF | NG | flags
}

fn put(table: usize, index: usize, value: u64) {
    unsafe { (table as *mut u64).add(index).write_volatile(value) };
}

impl AddrSpace {
    /// carga un ejecutable ELF en un espacio nuevo. devuelve el espacio y el punto de entrada.
    pub fn from_elf(data: &[u8]) -> Result<(AddrSpace, usize), &'static str> {
        let elf = elf::parse(data)?;
        let asid = alloc_asid().ok_or("no quedan ASIDs")?;
        let mut sp = AddrSpace { pages: Vec::new(), regions: Vec::new(), asid, ttbr: 0, stack_pa: 0 };
        match sp.build(data, &elf) {
            Ok(()) => Ok((sp, elf.entry)),
            Err(e) => {
                sp.destroy();
                Err(e)
            }
        }
    }

    /// reserva paginas contiguas ya puestas a cero (no se filtra contenido de otros procesos)
    fn alloc(&mut self, zone: Zone, n: usize) -> Result<usize, &'static str> {
        let pa = pmm::alloc_zone(zone, n).ok_or("sin memoria")?;
        unsafe { write_bytes(pa as *mut u8, 0, n * PAGE) };
        self.pages.push((pa, n));
        Ok(pa)
    }

    fn build(&mut self, data: &[u8], elf: &Elf) -> Result<(), &'static str> {
        let l1 = self.alloc(Zone::Kernel, 1)?;
        let l2 = self.alloc(Zone::Kernel, 1)?;
        let l3 = self.alloc(Zone::Kernel, 1)?;
        let (dev, ram) = mmu::kernel_l1();
        put(l1, 0, dev);
        put(l1, 1, ram);
        put(l1, 2, l2 as u64 | TABLE);
        put(l2, 0, l3 as u64 | TABLE);

        let mut mapped = [false; SPAN_PAGES];
        let mut entry_ok = false;
        for seg in &elf.segments {
            if seg.flags & PF_W != 0 && seg.flags & PF_X != 0 {
                return Err("segmento escribible y ejecutable (viola W^X)");
            }
            let end = seg.vaddr.checked_add(seg.memsz).ok_or("segmento invalido")?;
            if seg.vaddr % PAGE != 0 || seg.vaddr < CODE_VA || end > SEGMENTS_END {
                return Err("segmento fuera de la region de usuario o sin alinear");
            }
            let first = (seg.vaddr - CODE_VA) / PAGE;
            let n = seg.memsz.div_ceil(PAGE);
            if mapped[first..first + n].iter().any(|&m| m) {
                return Err("segmentos solapados");
            }
            mapped[first..first + n].iter_mut().for_each(|m| *m = true);

            let pa = self.alloc(Zone::User, n)?;
            unsafe {
                core::ptr::copy_nonoverlapping(data[seg.offset..].as_ptr(), pa as *mut u8, seg.filesz);
            }
            let flags = if seg.flags & PF_X != 0 {
                mmu::sync_icache(pa, n * PAGE);
                if (seg.vaddr..end).contains(&elf.entry) {
                    entry_ok = true;
                }
                AP_EL0 | AP_RO | PXN // texto: lectura y ejecucion
            } else if seg.flags & PF_W != 0 {
                AP_EL0 | PXN | UXN // datos: lectura y escritura
            } else {
                AP_EL0 | AP_RO | PXN | UXN // rodata: solo lectura
            };
            for i in 0..n {
                put(l3, first + i, desc(pa + i * PAGE, flags));
            }
            self.regions.push((seg.vaddr, seg.memsz, seg.flags & elf::PF_W != 0));
        }
        if !entry_ok {
            return Err("el punto de entrada no cae en un segmento ejecutable");
        }

        // pila: lectura y escritura, sin ejecucion
        let stack = self.alloc(Zone::User, STACK_PAGES)?;
        self.stack_pa = stack;
        let first = (STACK_BOTTOM - CODE_VA) / PAGE;
        for i in 0..STACK_PAGES {
            put(l3, first + i, desc(stack + i * PAGE, AP_EL0 | PXN | UXN));
        }
        self.regions.push((STACK_BOTTOM, STACK_PAGES * PAGE, true));

        unsafe { asm!("dsb ishst") }; // las tablas deben ser visibles para el recorrido de tablas
        self.ttbr = l1 as u64 | (self.asid as u64) << 48;
        Ok(())
    }

    /// deja argc y argv en la pila de usuario (cadenas, luego el vector de punteros terminado en
    /// nulo). devuelve (sp inicial, argc, direccion de argv). la pila se escribe por la
    /// identidad del kernel: el proceso todavia no ha corrido.
    pub fn push_args(&mut self, args: &[&[u8]]) -> Result<(usize, usize, usize), &'static str> {
        const MAX_ARGS: usize = 16;
        const MAX_BYTES: usize = 2048;
        let total: usize = args.iter().map(|a| a.len() + 1).sum::<usize>() + 8 * (args.len() + 1) + 32;
        if args.len() > MAX_ARGS || total > MAX_BYTES {
            return Err("demasiados argumentos");
        }
        let pa = |va: usize| self.stack_pa + (va - STACK_BOTTOM);
        let mut cursor = STACK_TOP;
        let mut ptrs: Vec<usize> = Vec::new();
        for a in args {
            cursor -= a.len() + 1;
            unsafe {
                core::ptr::copy_nonoverlapping(a.as_ptr(), pa(cursor) as *mut u8, a.len());
                (pa(cursor + a.len()) as *mut u8).write(0);
            }
            ptrs.push(cursor);
        }
        cursor &= !15;
        cursor -= 8 * (ptrs.len() + 1);
        cursor &= !15;
        let argv = cursor;
        for (i, p) in ptrs.iter().enumerate() {
            unsafe { (pa(argv + 8 * i) as *mut usize).write(*p) };
        }
        unsafe { (pa(argv + 8 * ptrs.len()) as *mut usize).write(0) };
        Ok((argv, ptrs.len(), argv))
    }

    /// valor para TTBR0 (tabla base + ASID)
    pub fn ttbr(&self) -> u64 {
        self.ttbr
    }

    /// el rango [ptr, ptr+len) cae entero dentro de una region mapeada del proceso
    pub fn contains(&self, ptr: usize, len: usize, write: bool) -> bool {
        let Some(end) = ptr.checked_add(len) else {
            return false;
        };
        self.regions.iter().any(|&(start, size, writable)| (!write || writable) && ptr >= start && end <= start + size)
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
