#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut names = alba::args(argc, argv).skip(1).peekable();
    if names.peek().is_none() {
        alba::println!("uso: touch <archivo>...");
        alba::exit(2);
    }
    let mut status = 0;
    for name in names {
        if let Some(fd) = alba::open(name) {
            alba::close(fd);
        } else if !alba::create(name) {
            alba::println!("touch: no se pudo crear: {}", name);
            status = 1;
        }
    }
    alba::exit(status)
}
