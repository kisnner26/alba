#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut args = alba::args(argc, argv).skip(1);
    let (Some(source), Some(dest), None) = (args.next(), args.next(), args.next()) else {
        alba::println!("uso: cp <origen> <destino nuevo>");
        alba::exit(2);
    };
    let Some(src) = alba::open(source) else {
        alba::println!("cp: no se pudo abrir: {}", source);
        alba::exit(1);
    };
    if !alba::create(dest) {
        alba::println!("cp: no se pudo crear: {}", dest);
        alba::exit(1);
    }
    if copy(src, dest) {
        alba::exit(0);
    }
    alba::unlink(dest);
    alba::println!("cp: no se pudo completar la copia");
    alba::exit(1)
}

fn copy(src: u64, dest: &str) -> bool {
    let Some(dst) = alba::open(dest) else { return false };
    let mut buf = [0; 4096];
    loop {
        match alba::read(src, &mut buf) {
            Some(0) => return true,
            Some(n) if alba::append(dst, &buf[..n]) => {}
            _ => return false,
        }
    }
}
