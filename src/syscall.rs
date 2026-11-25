// llamadas al sistema. convencion: `svc #0`, numero en x8, argumentos en x0-x2,
// resultado en x0 (u64::MAX = error).
//
//   0  write(fd, ptr, len)   escribe consola (fd 1 y 2) o anexa a un archivo (fd >= 3)
//   1  exit(code)            termina el proceso con un codigo de salida
//   2  sleep(ticks)          duerme (1 tick = 100 ms)
//   3  getpid()              id de la tarea
//   4  yield()               cede la cpu
//   5  wait(pid)             espera a un proceso y devuelve su codigo de salida
//   8  readfile(nombre, nnombre, buf, nbuf, desplazamiento)  lee de un archivo del disco
//   9  listdir(indice, buf, nbuf)   nombre de la entrada del directorio; devuelve
//                                   tamano<<32 | banderas<<16 | largo del nombre
//   10 ticks()               ticks desde el arranque (1 tick = 100 ms)
//   11 timer_irqs()          interrupciones de timer desde el arranque (mide cuanto se despierta la cpu)
//   12 disk_requests()      peticiones enviadas al disco desde el arranque
//   7  read(fd, ptr, len)    lee consola (fd 0) o archivo (fd >= 3)
//   13 open(nombre, len)     abre un archivo
//   14 seek(fd, offset)      fija el desplazamiento absoluto hasta el fin del archivo
//   15 close(fd)             libera el descriptor
//   16 create(nombre, len)   crea un archivo vacio
//   17 unlink(nombre, len)   elimina un archivo
//   6  exec(ruta, nruta, args, nargs)  lanza un programa del disco; args son cadenas
//                                      terminadas en cero; devuelve el pid

use crate::{console, fs, irq, sched};
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
        11 => crate::timer::irq_count(),
        12 => crate::blk::disk_requests(),
        13 => open(a[0], a[1]).unwrap_or(ERR),
        14 => seek(a[0], a[1]).unwrap_or(ERR),
        15 => sched::close_file(a[0]).map_or(ERR, |_| 0),
        16 => name_op(a[0], a[1], fs::create),
        17 => name_op(a[0], a[1], fs::unlink),
        18 => meminfo(a[0]),
        19 => crate::timer::micros(),
        20 => crate::gpu::framebuffer()
            .map_or(ERR, |fb| (u64::from(fb.width) << 32) | u64::from(fb.height)),
        21 => crate::gpu::framebuffer()
            .and_then(|fb| sched::map_framebuffer(fb.addr, (fb.width * fb.height * 4) as usize))
            .map_or(ERR, |addr| addr as u64),
        22 => crate::gpu::flush([a[0], a[1], a[2], a[3]]).map_or(ERR, |_| 0),
        23 => crate::input::read().unwrap_or(ERR),
        24 => procinfo(a[0], a[1], a[2]),
        _ => ERR,
    }
}

fn read(fd: u64, ptr: u64, len: u64) -> u64 {
    if fd == 0 {
        return output_slice(ptr, len).map_or(ERR, |buf| console::read(buf) as u64);
    }
    let Some(file) = sched::file(fd) else { return ERR };
    let Some(buf) = output_slice(ptr, len) else { return ERR };
    let Some(n) = fs::read_id(file.entry.get(), u64::from(file.offset), buf) else { return ERR };
    sched::set_file_offset(fd, file.offset + n as u32).map_or(ERR, |_| n as u64)
}

fn open(ptr: u64, len: u64) -> Option<u64> {
    let name = copy_in(ptr, len, 40)?;
    let name = core::str::from_utf8(&name).ok()?;
    sched::open_file(core::num::NonZeroU64::new(fs::open(name)?)?)
}

fn seek(fd: u64, offset: u64) -> Option<u64> {
    let file = sched::file(fd)?;
    if offset > u64::from(fs::size(file.entry.get())?) {
        return None;
    }
    sched::set_file_offset(fd, offset as u32)?;
    Some(offset)
}

fn output_slice(ptr: u64, len: u64) -> Option<&'static mut [u8]> {
    if len == 0 {
        return Some(&mut []);
    }
    if len > 4096 || !sched::user_range_ok(ptr, len, true) {
        return None;
    }
    // solo se usa en la syscall actual; su espacio de usuario sigue vivo al bloquear.
    Some(unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) })
}

fn write(fd: u64, ptr: u64, len: u64) -> u64 {
    if fd >= 3 {
        let Some(file) = sched::file(fd) else { return ERR };
        let Some(data) = copy_in(ptr, len, 4096) else { return ERR };
        let Some(end) = fs::append(file.entry.get(), &data) else { return ERR };
        return sched::set_file_offset(fd, end).map_or(ERR, |_| len);
    }
    if !matches!(fd, 1 | 2) || len > 4096 || !sched::user_range_ok(ptr, len, false) {
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
    if len == 0 {
        return Some(alloc::vec::Vec::new());
    }
    if len > max || !sched::user_range_ok(ptr, len, false) {
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
    let Some(out) = output_slice(buf, buf_len) else {
        return ERR;
    };
    fs::read_named(name, offset, out).map_or(ERR, |n| n as u64)
}

fn listdir(index: u64, buf: u64, buf_len: u64) -> u64 {
    let Some(e) = fs::list(index as usize) else {
        return ERR;
    };
    let name = e.name.as_bytes();
    if name.len() as u64 > buf_len || !sched::user_range_ok(buf, name.len() as u64, true) {
        return ERR;
    }
    unsafe { core::ptr::copy_nonoverlapping(name.as_ptr(), buf as *mut u8, name.len()) };
    (e.size as u64) << 32 | ((e.flags & 0xff) as u64) << 16 | name.len() as u64
}

fn name_op(ptr: u64, len: u64, op: fn(&str) -> Option<()>) -> u64 {
    let Some(name) = copy_in(ptr, len, 39) else { return ERR };
    let Ok(name) = core::str::from_utf8(&name) else { return ERR };
    op(name).map_or(ERR, |_| 0)
}

/// zonas 0 y 1 devuelven (libres, usadas); 2 devuelve (kernel, usuario) de la tarea.
fn meminfo(selector: u64) -> u64 {
    let (low, high) = match selector {
        0 | 1 => crate::pmm::zone_counts(selector == 1),
        2 => sched::current_page_counts(),
        _ => return ERR,
    };
    u64::from(low) | (u64::from(high) << 32)
}

fn procinfo(index: u64, ptr: u64, len: u64) -> u64 {
    if len != sched::PROC_RECORD as u64 {
        return ERR;
    }
    let Some(out) = output_slice(ptr, len) else { return ERR };
    let Some(data) = sched::procinfo(index as usize) else { return 0 };
    out.copy_from_slice(&data);
    len
}
