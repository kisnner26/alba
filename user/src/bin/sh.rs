//! shell de alba: lee una linea, la parte en palabras y lanza el programa del disco con ese
//! nombre, esperando a que termine. incorporados: help, exit.

#![no_std]
#![no_main]

use alba::{print, println};

const MAX_ARGS: usize = 8;

#[no_mangle]
pub extern "C" fn _start() -> ! {
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
            "help" => {
                println!("incorporados: help, exit");
                println!("el resto son programas del disco: prueba ls");
            }
            cmd => match alba::exec(cmd, &words[..count]) {
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
