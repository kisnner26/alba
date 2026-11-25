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
        let Some(fd) = alba::open(name) else {
            println!("cat: no se pudo abrir: {}", name);
            status = 1;
            continue;
        };
        let mut buf = [0u8; 512];
        loop {
            match alba::read(fd, &mut buf) {
                Some(0) => break,
                Some(n) => {
                    if alba::write(&buf[..n]) != n as u64 { alba::exit(1) }
                }
                None => {
                    println!("cat: error de lectura: {}", name);
                    status = 1;
                    break;
                }
            }
        }
        alba::close(fd);
    }
    if !any {
        let mut buf = [0; 512];
        loop {
            match alba::read(0, &mut buf) {
                Some(0) => break,
                Some(n) => { if alba::write(&buf[..n]) != n as u64 { alba::exit(1) } }
                None => { status = 1; break; }
            }
        }
    }
    alba::exit(status)
}
