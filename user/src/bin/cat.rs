//! muestra el contenido de uno o mas archivos del disco.

#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut status = 0;
    let mut any = false;
    for name in alba::args(argc, argv).skip(1) {
        any = true;
        let mut buf = [0u8; 512];
        let mut offset = 0u64;
        loop {
            match alba::read_file(name, &mut buf, offset) {
                Some(0) => break,
                Some(n) => {
                    alba::write(&buf[..n]);
                    offset += n as u64;
                }
                None => {
                    println!("cat: no existe: {}", name);
                    status = 1;
                    break;
                }
            }
        }
    }
    if !any {
        println!("uso: cat <archivo>...");
        status = 2;
    }
    alba::exit(status)
}
