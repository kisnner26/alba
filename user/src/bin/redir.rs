//! interpreta los operadores de entrada y salida de la shell.
#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start(argc: usize, argv: *const *const u8) -> ! {
    let mut words = [""; 8];
    let mut nargs = 0;
    for arg in alba::args(argc, argv) {
        if nargs == words.len() {
            alba::exit(1);
        }
        words[nargs] = arg;
        nargs += 1;
    }
    alba::exit(u64::from(!run(&words[..nargs])))
}

fn run(words: &[&str]) -> bool {
    let mut parts = words.split(|word| *word == "|");
    let mut left = [0; 160];
    let Some(a) = encode(parts.next().unwrap_or_default(), &mut left) else { return false };
    let Some(words) = parts.next() else {
        let pid = alba::syscall(27, left.as_ptr() as u64, a, 0, 0);
        return pid != u64::MAX && alba::wait(pid) == 0;
    };
    let mut right = [0; 160];
    let Some(b) = encode(words, &mut right) else { return false };
    if parts.next().is_some() {
        return false;
    }
    let pids = alba::syscall(28, left.as_ptr() as u64, a, right.as_ptr() as u64, b);
    if pids == u64::MAX {
        return false;
    }
    alba::wait(pids >> 32);
    alba::wait(pids & u64::from(u32::MAX)) == 0
}

fn encode<'a>(words: &'a [&'a str], spec: &mut [u8; 160]) -> Option<u64> {
    let mut tokens = words.iter().copied();
    let mut args = [""; 8];
    let mut nargs = 0;
    let mut paths = [""; 2];
    while let Some(word) = tokens.next() {
        let direction = match word {
            "<" => 0,
            ">" | ">>" => 1,
            _ => {
                *args.get_mut(nargs)? = word;
                nargs += 1;
                continue;
            }
        };
        let path = tokens.next()?;
        if !paths[direction].is_empty() || matches!(path, "<" | ">" | ">>") {
            return None;
        }
        paths[direction] = path;
        if direction == 1 {
            spec[0] = u8::from(word == ">>");
        }
    }
    if nargs == 0 {
        return None;
    }
    let mut len = 1;
    for field in paths.into_iter().chain(args[..nargs].iter().copied()) {
        let end = len + field.len();
        spec.get_mut(len..end)?.copy_from_slice(field.as_bytes());
        len = end + 1;
    }
    Some((len - 1) as u64)
}
