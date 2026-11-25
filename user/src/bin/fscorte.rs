//! prueba de corte de energia de albafs.
//!
//! `fscorte inundar` reescribe sin parar tres archivos con contenido que se puede verificar.
//! el arnes de pruebas mata qemu en un momento cualquiera. despues de volver a arrancar con el
//! mismo disco, `fscorte verificar` comprueba que cada archivo esta ausente, vacio o COMPLETO:
//! nunca a medias. asi se prueba que cada commit del directorio es atomico.
#![no_std]
#![no_main]

const NAMES: [&str; 3] = ["corte0.txt", "corte1.txt", "corte2.txt"];
const HEADER: usize = 19; // "K=" + 16 digitos hexadecimales + salto de linea
const FOOTER: &[u8] = b"\nFIN\n";

/// longitud del cuerpo de la generacion k: entre 700 y 2700 bytes, para cruzar varios sectores
fn body_len(k: u64) -> usize {
    700 + (k % 5) as usize * 500
}

fn pattern(k: u64, i: usize) -> u8 {
    b'a' + ((k as usize + i) % 26) as u8
}

/// escribe el contenido de la generacion k y devuelve cuantos bytes ocupa
fn fill(k: u64, buf: &mut [u8; 4096]) -> usize {
    buf[0] = b'K';
    buf[1] = b'=';
    for d in 0..16 {
        let nibble = ((k >> (60 - 4 * d)) & 0xf) as u8;
        buf[2 + d] = if nibble < 10 { b'0' + nibble } else { b'a' + nibble - 10 };
    }
    buf[18] = b'\n';
    let body = body_len(k);
    for i in 0..body {
        buf[HEADER + i] = pattern(k, i);
    }
    buf[HEADER + body..HEADER + body + FOOTER.len()].copy_from_slice(FOOTER);
    HEADER + body + FOOTER.len()
}

fn valid(data: &[u8]) -> bool {
    if data.len() < HEADER + FOOTER.len() || &data[..2] != b"K=" || data[18] != b'\n' {
        return false;
    }
    let mut k = 0u64;
    for &c in &data[2..18] {
        let digit = match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => return false,
        };
        k = (k << 4) | u64::from(digit);
    }
    let body = data.len() - HEADER - FOOTER.len();
    body == body_len(k)
        && (0..body).all(|i| data[HEADER + i] == pattern(k, i))
        && &data[HEADER + body..] == FOOTER
}

fn flood() -> ! {
    let mut buf = [0u8; 4096];
    let mut k = 0u64;
    loop {
        let name = NAMES[(k % 3) as usize];
        let _ = alba::unlink(name); // al principio no existe
        let n = fill(k, &mut buf);
        let written = alba::create(name)
            && alba::open(name).is_some_and(|fd| {
                let ok = alba::append(fd, &buf[..n]);
                alba::close(fd);
                ok
            });
        if !written {
            alba::println!("fscorte: no se pudo escribir la generacion {}", k);
            alba::exit(2);
        }
        k += 1;
    }
}

fn verify() -> ! {
    let (mut complete, mut empty, mut absent) = (0, 0, 0);
    let mut buf = [0u8; 4096];
    for name in NAMES {
        match alba::read_file(name, &mut buf, 0) {
            None => absent += 1,
            Some(0) => empty += 1,
            Some(n) if valid(&buf[..n]) => complete += 1,
            Some(n) => {
                alba::println!("fscorte: CORRUPTO {} ({} bytes)", name, n);
                alba::exit(1);
            }
        }
    }
    alba::println!("fscorte: ok {} completos, {} vacios, {} ausentes", complete, empty, absent);
    alba::exit(0)
}

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    match alba::args(argc, argv).nth(1) {
        Some("inundar") => flood(),
        Some("verificar") => verify(),
        _ => {
            alba::println!("uso: fscorte inundar | verificar");
            alba::exit(2)
        }
    }
}
