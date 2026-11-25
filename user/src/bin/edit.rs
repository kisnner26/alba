#![no_std]
#![no_main]

use alba::println;

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut args = alba::args(argc, argv).skip(1);
    let (Some(name), None) = (args.next(), args.next()) else {
        println!("uso: edit <archivo>");
        alba::exit(2);
    };
    let mut text = [0; 4096];
    let mut len = 0;
    if let Some(fd) = alba::open(name) {
        let Some(n) = alba::read(fd, &mut text) else { alba::exit(1) };
        if alba::read(fd, &mut [0; 1]) != Some(0) || core::str::from_utf8(&text[..n]).is_err() {
            println!("edit: se requiere texto utf-8 de hasta 4096 bytes");
            alba::exit(1);
        }
        alba::close(fd);
        len = n;
    }
    println!("edit: :ver, :borrar, :guardar, :salir");
    let mut line = [0; 256];
    loop {
        alba::write(b"edit> ");
        let n = alba::read_line(&mut line);
        if n == 0 {
            alba::exit(0);
        }
        match &line[..n] {
            b":ver\n" => {
                alba::write(&text[..len]);
            }
            b":borrar\n" => len = 0,
            b":salir\n" => alba::exit(0),
            b":guardar\n" => {
                let result = alba::syscall(25, name.as_ptr() as u64, name.len() as u64,
                    text.as_ptr() as u64, len as u64);
                if result == 0 {
                    println!("edit: guardado");
                    alba::exit(0);
                }
                println!("edit: no se pudo guardar");
            }
            _ if n <= text.len() - len => {
                text[len..len+n].copy_from_slice(&line[..n]);
                len += n;
            }
            _ => println!("edit: el texto no puede pasar de 4096 bytes"),
        }
    }
}
