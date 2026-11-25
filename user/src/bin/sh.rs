//! shell de alba: lee una linea, la parte en palabras y lanza el programa del disco con ese
//! nombre, esperando a que termine. incorporados: help, exit, top, files, mv.

#![no_std]
#![no_main]

use alba::{print, println};

const MAX_ARGS: usize = 8;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let boot_us = alba::clock_us();
    println!("alba shell. escribe help para ver los comandos.");
    let mut buf = [0u8; 128];
    loop {
        print!("alba> ");
        let n = alba::read_line(&mut buf);
        if n == 0 {
            break;
        }
        let line = core::str::from_utf8(&buf[..n]).unwrap_or("").trim();

        let mut words = [""; MAX_ARGS];
        let mut count = 0;
        for w in line.split_whitespace() {
            if count == MAX_ARGS {
                break;
            }
            words[count] = w;
            count += 1;
        }
        if count == 0 {
            continue;
        }

        match words[0] {
            "exit" => break,
            "__recursos" => measure(boot_us),
            "__lanzamiento" => measure_launch(),
            "help" => {
                println!("incorporados: help, exit, top, files, mv");
                println!("el resto son programas del disco: prueba ls");
            }
            "mv" => {
                if count != 3 || alba::syscall(26,
                    words[1].as_ptr() as u64, words[1].len() as u64,
                    words[2].as_ptr() as u64, words[2].len() as u64,
                ) != 0 {
                    println!("mv: se requiere un origen y un destino libre");
                }
            }
            cmd => match alba::exec(match cmd {
                "top" => "ps",
                "files" => "ls",
                _ => cmd,
            }, &words[..count]) {
                Some(pid) => {
                    let code = alba::wait(pid);
                    if code != 0 {
                        println!("[{} termino con codigo {}]", cmd, code);
                    }
                }
                None => println!("sh: no encontrado: {}", cmd),
            },
        }
    }
    alba::exit(0)
}

// se ejecuta en la propia shell para medir la memoria sin cargar otro proceso.
fn measure(boot_us: u64) {
    let (_, kernel_used) = alba::meminfo(0).unwrap();
    let (_, user_used) = alba::meminfo(1).unwrap();
    let (kernel, user) = alba::meminfo(2).unwrap();
    println!(
        "recursos: {} {} {} {} {}",
        boot_us, (kernel_used + user_used) as u64 * 4096, kernel, user, alba::timer_irqs(),
    );
}

fn measure_launch() {
    let start = alba::clock_us();
    if let Some(pid) = alba::exec("echo", &["echo"]) {
        if alba::wait(pid) == 0 {
            println!("lanzamiento: {}", alba::clock_us() - start);
        }
    }
}
