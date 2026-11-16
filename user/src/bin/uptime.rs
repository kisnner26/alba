//! tiempo desde el arranque.

#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let t = alba::ticks();
    println!("arriba desde hace {}.{} s", t / 10, t % 10);
    alba::exit(0)
}
