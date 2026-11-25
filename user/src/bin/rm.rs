#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut names = alba::args(argc, argv).skip(1).peekable();
    if names.peek().is_none() {
        alba::println!("uso: rm <archivo>...");
        alba::exit(2);
    }
    let mut status = 0;
    for name in names {
        if !alba::unlink(name) {
            alba::println!("rm: no se pudo borrar: {}", name);
            status = 1;
        }
    }
    alba::exit(status)
}
