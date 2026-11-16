//! intenta escribir en su propio codigo: esta mapeado como solo lectura (W^X).

#![no_std]
#![no_main]

use alba::println;
use core::ptr::write_volatile;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("[intruso2] intentando escribir en su propio codigo (debe fallar)");
    unsafe { write_volatile(_start as *const () as *mut u8, 0) };
    println!("[intruso2] ERROR: la escritura no fallo");
    alba::exit()
}
