//! muestra sus argumentos y sale con codigo 7 (para probar argc/argv y el codigo de salida).

#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    println!("[eco] argc={}", argc);
    for (i, a) in alba::args(argc, argv).enumerate() {
        println!("[eco] argv[{}]={}", i, a);
    }
    alba::exit(7)
}
