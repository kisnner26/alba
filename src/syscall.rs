// llamadas al sistema. convencion: `svc #0`, numero en x8, argumentos en x0-x2,
// resultado en x0 (u64::MAX = error).
//
//   0  write(fd, ptr, len)   escribe en la consola (solo fd 1)
//   1  exit(code)            termina el proceso con un codigo de salida
//   2  sleep(ticks)          duerme (1 tick = 100 ms)
//   3  getpid()              id de la tarea
//   4  yield()               cede la cpu
//   5  wait(pid)             espera a un proceso y devuelve su codigo de salida
//   8  readfile(nombre, nnombre, buf, nbuf, desplazamiento)  lee de un archivo del disco
//   9  listdir(indice, buf, nbuf)   nombre de la entrada del directorio; devuelve
//                                   tamano<<32 | banderas<<16 | largo del nombre
//   10 ticks()               ticks desde el arranque (1 tick = 100 ms)
//   7  read(fd, ptr, len)    lee una linea de la consola (fd 0); bloquea hasta tenerla
//   6  exec(ruta, nruta, args, nargs)  lanza un programa del disco; args son cadenas
//                                      terminadas en cero; devuelve el pid

use crate::{console, irq, sched};
use core::fmt::Write;

const ERR: u64 = u64::MAX;

pub fn dispatch(nr: u64, a: [u64; 5]) -> u64 {
    match nr {
        0 => write(a[0], a[1], a[2]),
        1 => sched::exit_with(a[0]),
        2 => {
            sched::sleep(a[0]);
            0
        }
        3 => sched::current_id() as u64,
        4 => {
            sched::yield_now();
            0
        }
        5 => sched::wait_child(a[0] as usize),
        6 => exec(a[0], a[1], a[2], a[3]),
        7 => read(a[0], a[1], a[2]),
        8 => readfile(a[0], a[1], a[2], a[3], a[4]),
        9 => listdir(a[0], a[1], a[2]),
        10 => crate::timer::ticks(),
        _ => ERR,
    }
}

fn read(fd: u64, ptr: u64, len: u64) -> u64 {
    if fd != 0 || len > 4096 || !sched::user_range_ok(ptr, len) {
        return ERR;
    }
    let buf = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) };
    console::read(buf) as u64
}

fn write(fd: u64, ptr: u64, len: u64) -> u64 {
    if fd != 1 || len > 4096 || !sched::user_range_ok(ptr, len) {
        return ERR;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let g = irq::save();
    let _ = match core::str::from_utf8(bytes) {
        Ok(s) => crate::Uart.write_str(s),
        Err(_) => crate::Uart.write_str("?"),
    };
    irq::restore(g);
    len
}

/// copia una cadena o bloque de usuario al kernel, validando el rango
fn copy_in(ptr: u64, len: u64, max: u64) -> Option<alloc::vec::Vec<u8>> {
    if len > max || (len > 0 && !sched::user_range_ok(ptr, len)) {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) }.to_vec())
}

fn exec(path: u64, path_len: u64, args: u64, args_len: u64) -> u64 {
    let (Some(path), Some(blob)) = (copy_in(path, path_len, 64), copy_in(args, args_len, 2048)) else {
        return ERR;
    };
    let Ok(name) = core::str::from_utf8(&path) else {
        return ERR;
    };
    // argumentos: cadenas terminadas en cero; si no hay, argv = [nombre]
    let mut list: alloc::vec::Vec<&[u8]> = blob.split(|&b| b == 0).collect();
    if list.last().is_some_and(|l| l.is_empty()) {
        list.pop(); // el ultimo terminador deja un trozo vacio
    }
    if list.is_empty() {
        list.push(name.as_bytes());
    }
    match sched::exec_args(name, &list) {
        Ok(pid) => pid as u64,
        Err(_) => ERR,
    }
}

fn readfile(name: u64, name_len: u64, buf: u64, buf_len: u64, offset: u64) -> u64 {
    let Some(name) = copy_in(name, name_len, 64) else {
        return ERR;
    };
    let Ok(name) = core::str::from_utf8(&name) else {
        return ERR;
    };
    if buf_len > 4096 || !sched::user_range_ok(buf, buf_len) {
        return ERR;
    }
    let Some(data) = crate::fs::read(name) else {
        return ERR;
    };
    let offset = offset as usize;
    if offset >= data.len() {
        return 0;
    }
    let n = (buf_len as usize).min(data.len() - offset);
    unsafe { core::ptr::copy_nonoverlapping(data[offset..].as_ptr(), buf as *mut u8, n) };
    n as u64
}

fn listdir(index: u64, buf: u64, buf_len: u64) -> u64 {
    let Some(e) = crate::fs::files().get(index as usize) else {
        return ERR;
    };
    let name = e.name.as_bytes();
    if name.len() as u64 > buf_len || !sched::user_range_ok(buf, buf_len) {
        return ERR;
    }
    unsafe { core::ptr::copy_nonoverlapping(name.as_ptr(), buf as *mut u8, name.len()) };
    (e.size as u64) << 32 | ((e.flags & 0xff) as u64) << 16 | name.len() as u64
}
