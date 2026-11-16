//! escribe sus argumentos separados por espacios.

#![no_std]
#![no_main]

use alba::{print, println};

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    for (i, a) in alba::args(argc, argv).skip(1).enumerate() {
        if i > 0 {
            print!(" ");
        }
        print!("{}", a);
    }
    println!();
    alba::exit(0)
}
