//! lanza `eco` con argumentos, espera a que termine y muestra su codigo de salida.

#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    match alba::exec("eco", &["eco", "uno", "dos tres"]) {
        Some(pid) => {
            let code = alba::wait(pid);
            println!("[lanzador] hijo {} termino con codigo {}", pid, code);
        }
        None => println!("[lanzador] ERROR: no pude lanzar eco"),
    }
    // un pid que no existe no debe colgar al que espera
    let r = alba::wait(9999);
    println!("[lanzador] esperar un pid inexistente da {}", if r == u64::MAX { "error" } else { "ERROR" });
    alba::exit(0)
}
