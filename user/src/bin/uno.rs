//! lee unos pocos bytes de la entrada estandar y sale. sirve para probar el cierre anticipado del
//! lector de una tuberia: el escritor debe enterarse y terminar en vez de quedarse bloqueado.
#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut buf = [0u8; 16];
    let n = alba::read(0, &mut buf).unwrap_or(0);
    alba::println!("uno: leyo {} bytes", n);
    alba::exit(0)
}
