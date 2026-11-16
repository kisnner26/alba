//! programa bien portado: usa solo syscalls y comprueba que su memoria es solo suya.

#![no_std]
#![no_main]

use alba::println;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};

// en .bss: el kernel debe entregarla puesta a cero y propia de cada proceso
static mut DATOS: u64 = 0;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let pid = alba::getpid();
    unsafe { write_volatile(addr_of_mut!(DATOS), pid) };

    for i in 1..=3 {
        println!("[usuario pid={}] iteracion {}", pid, i);
        alba::sleep(7);
    }
    println!("[usuario pid={}] fin", pid);

    // si otro proceso compartiera esta pagina, habria pisado el pid
    if unsafe { read_volatile(addr_of!(DATOS)) } == pid {
        println!("[usuario pid={}] datos propios intactos", pid);
    } else {
        println!("[usuario pid={}] ERROR: memoria compartida", pid);
    }
    alba::exit(0)
}
