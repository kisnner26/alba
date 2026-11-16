//! lee una linea de la consola y la repite (para probar la entrada por interrupcion).

#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("[leer] escribe algo:");
    let mut buf = [0u8; 128];
    let n = alba::read_line(&mut buf);
    let line = core::str::from_utf8(&buf[..n]).unwrap_or("?");
    println!("[leer] recibi: {}", line.trim_end());
    alba::exit(0)
}
