//! comprueba el limite de descriptores y sus posiciones independientes.
#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut fds = [0; 16];
    for fd in &mut fds {
        *fd = alba::open("hola.txt").expect("falta un descriptor libre");
        assert!(*fd >= 3);
    }
    assert!(alba::open("hola.txt").is_none());
    assert!(!alba::close(2));

    let mut buf = [0; 4];
    assert!(alba::seek(fds[0], 5));
    assert!(!alba::seek(fds[0], 29));
    assert_eq!(alba::read(fds[0], &mut buf), Some(4));
    assert_eq!(&buf, b"desd");
    assert_eq!(alba::read(fds[1], &mut buf), Some(4));
    assert_eq!(&buf, b"hola");
    assert!(alba::seek(fds[0], 28));
    assert_eq!(alba::read(fds[0], &mut buf), Some(0));

    assert!(alba::close(fds[0]));
    assert!(alba::read(fds[0], &mut buf).is_none());
    assert!(!alba::close(fds[0]));
    assert!(alba::open("noexiste").is_none());
    assert_eq!(alba::open("hola.txt"), Some(fds[0]));
    // deja los descriptores abiertos para que exit los libere.
    alba::println!("fdtest: limite, seek y reutilizacion correctos");
    alba::exit(0)
}
