#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    for (zone, name) in ["kernel", "usuario"].into_iter().enumerate() {
        let (free, used) = alba::meminfo(zone as u64).unwrap();
        alba::println!("{}: {} paginas libres, {} usadas", name, free, used);
    }
    let (kernel, user) = alba::meminfo(2).unwrap();
    alba::println!("proceso actual: {} paginas de kernel, {} de usuario", kernel, user);
    alba::exit(0)
}
