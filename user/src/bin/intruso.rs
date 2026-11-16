//! intenta leer memoria del kernel: el procesador debe pararlo.

#![no_std]
#![no_main]

use alba::println;
use core::ptr::read_volatile;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("[intruso] intentando leer memoria del kernel (debe fallar)");
    let _ = unsafe { read_volatile(0x4008_0000 as *const u32) };
    println!("[intruso] ERROR: la lectura no fallo");
    alba::exit()
}
