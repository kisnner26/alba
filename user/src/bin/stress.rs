//! prueba de reutilización de páginas y asid tras terminar procesos.
#![no_std]
#![no_main]

fn launch() -> bool {
    alba::exec("touch", &["touch", "hola.txt"]).is_some_and(|pid| alba::wait(pid) == 0)
}

fn free() -> (u32, u32) {
    (alba::meminfo(0).unwrap().0, alba::meminfo(1).unwrap().0)
}

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let seconds = alba::args(argc, argv).nth(1).and_then(|arg| arg.parse::<u64>().ok()).unwrap_or(0);
    for _ in 0..32 {
        assert!(launch());
    }
    // la primera tanda llena las listas reutilizables del heap antes de medir.
    let before = free();
    let until = alba::clock_us().saturating_add(seconds.saturating_mul(1_000_000));
    let mut count = 0u64;
    while count < 1000 || alba::clock_us() < until {
        if !launch() {
            alba::println!("stress: fallo al lanzar despues de {}", count);
            alba::exit(1);
        }
        count += 1;
    }
    let after = free();
    if before != after {
        alba::println!("stress: paginas libres antes={:?} despues={:?}", before, after);
        alba::exit(1);
    }
    alba::println!("stress: {} procesos sin fugas de paginas", count);
    alba::exit(0)
}
