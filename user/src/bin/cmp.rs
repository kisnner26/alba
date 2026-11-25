//! compara dos archivos byte a byte. termina con 0 si son iguales, 1 si difieren y 2 si falla.
#![no_std]
#![no_main]

use alba::println;

/// lee hasta llenar el buffer o llegar al final del archivo
fn read_full(fd: u64, buf: &mut [u8]) -> usize {
    let mut n = 0;
    while n < buf.len() {
        match alba::read(fd, &mut buf[n..]) {
            Some(0) | None => break,
            Some(k) => n += k,
        }
    }
    n
}

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut names = alba::args(argc, argv).skip(1);
    let (Some(a), Some(b)) = (names.next(), names.next()) else {
        println!("uso: cmp <archivo> <archivo>");
        alba::exit(2)
    };
    let (Some(fa), Some(fb)) = (alba::open(a), alba::open(b)) else {
        println!("cmp: no se pudo abrir {} o {}", a, b);
        alba::exit(2)
    };
    let (mut buf_a, mut buf_b) = ([0u8; 512], [0u8; 512]);
    let mut offset = 0u64;
    loop {
        let na = read_full(fa, &mut buf_a);
        let nb = read_full(fb, &mut buf_b);
        for i in 0..na.min(nb) {
            if buf_a[i] != buf_b[i] {
                println!("cmp: difieren en el byte {}", offset + i as u64);
                alba::exit(1);
            }
        }
        if na != nb {
            println!("cmp: difieren en el tamano tras el byte {}", offset + na.min(nb) as u64);
            alba::exit(1);
        }
        if na == 0 {
            println!("cmp: iguales ({} bytes)", offset);
            alba::exit(0);
        }
        offset += na as u64;
    }
}
