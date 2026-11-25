//! interpreta los operadores de entrada y salida de la shell.
#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut tokens = alba::args(argc, argv);
    let mut words = [""; 8];
    let mut nargs = 0;
    let mut specs = [""; 2];
    let mut append = false;
    while let Some(word) = tokens.next() {
        let direction = match word {
            "<" => 0,
            ">" | ">>" => 1,
            "|" => alba::exit(1),
            _ if nargs < words.len() => {
                words[nargs] = word;
                nargs += 1;
                continue;
            }
            _ => alba::exit(1),
        };
        let Some(path) = tokens.next() else { alba::exit(1) };
        if !specs[direction].is_empty() || matches!(path, "<" | ">" | ">>" | "|") {
            alba::exit(1);
        }
        specs[direction] = path;
        if direction == 1 {
            append = word == ">>";
        }
    }
    alba::exit(u64::from(nargs == 0 || !redirected(specs, append, &words[..nargs])))
}

fn redirected(specs: [&str; 2], append: bool, args: &[&str]) -> bool {
    let mut spec = [0; 160];
    spec[0] = u8::from(append);
    let mut len = 1;
    for field in specs.into_iter().chain(args.iter().copied()) {
        let end = len + field.len();
        if end >= spec.len() {
            return false;
        }
        spec[len..end].copy_from_slice(field.as_bytes());
        len = end + 1;
    }
    let pid = alba::syscall(27, spec.as_ptr() as u64, (len - 1) as u64, 0, 0);
    if pid == u64::MAX {
        return false;
    }
    if alba::wait(pid) != 0 {
        return false;
    }
    true
}
