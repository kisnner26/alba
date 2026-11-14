// asignador del kernel: clases de tamano en potencias de 2 (16..2048 bytes)
// con listas libres, y paginas contiguas directas para lo grande.
// alloc y dealloc son O(1) en el caso comun. cada bloque queda alineado a su
// propio tamano porque se corta de una pagina alineada.
// no es seguro desde interrupciones: no asignar memoria en manejadores de irq.

use crate::pmm::{self, PAGE};
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{addr_of_mut, null_mut};

const MIN_SHIFT: usize = 4; // 16 bytes
const CLASSES: usize = 8; // hasta 2048
const MAX_SMALL: usize = PAGE / 2;

static mut FREE: [usize; CLASSES] = [0; CLASSES];

pub struct KernelHeap;

fn norm(layout: &Layout) -> usize {
    layout.size().max(layout.align()).max(1 << MIN_SHIFT)
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = norm(&layout);
        if size <= MAX_SMALL {
            let c = size.next_power_of_two();
            let idx = c.trailing_zeros() as usize - MIN_SHIFT;
            let free = &mut *addr_of_mut!(FREE);
            if free[idx] == 0 {
                let Some(page) = pmm::alloc_pages(1) else {
                    return null_mut();
                };
                // corta la pagina: el primer bloque se devuelve, el resto a la lista
                for i in (1..PAGE / c).rev() {
                    let b = page + i * c;
                    *(b as *mut usize) = free[idx];
                    free[idx] = b;
                }
                return page as *mut u8;
            }
            let b = free[idx];
            free[idx] = *(b as *const usize);
            b as *mut u8
        } else {
            if layout.align() > PAGE {
                return null_mut();
            }
            match pmm::alloc_pages(size.div_ceil(PAGE)) {
                Some(a) => a as *mut u8,
                None => null_mut(),
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = norm(&layout);
        if size <= MAX_SMALL {
            let c = size.next_power_of_two();
            let idx = c.trailing_zeros() as usize - MIN_SHIFT;
            let free = &mut *addr_of_mut!(FREE);
            *(ptr as *mut usize) = free[idx];
            free[idx] = ptr as usize;
        } else {
            pmm::free_pages(ptr as usize, size.div_ceil(PAGE));
        }
    }
}
