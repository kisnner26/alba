//! lista los archivos del disco: nombre, tamano y si son ejecutables.

#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut name = [0u8; 48];
    let mut i = 0;
    while let Some((n, size, exec)) = alba::list_dir(i, &mut name) {
        let kind = if exec { "  ejecutable" } else { "" };
        if size < 1024 {
            println!("{:<16} {:>8} bytes{}", n, size, kind);
        } else {
            println!("{:<16} {:>8}.{} kib{}", n, size / 1024, (size % 1024) * 10 / 1024, kind);
        }
        i += 1;
    }
    alba::exit(0)
}
