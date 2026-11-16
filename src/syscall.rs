// llamadas al sistema. convencion: `svc #0`, numero en x8, argumentos en x0-x2,
// resultado en x0 (u64::MAX = error).
//
//   0  write(fd, ptr, len)   escribe en la consola (solo fd 1)
//   1  exit()                termina la tarea
//   2  sleep(ticks)          duerme (1 tick = 100 ms)
//   3  getpid()              id de la tarea
//   4  yield()               cede la cpu

use crate::{irq, sched};
use core::fmt::Write;

const ERR: u64 = u64::MAX;

pub fn dispatch(nr: u64, a: [u64; 3]) -> u64 {
    match nr {
        0 => write(a[0], a[1], a[2]),
        1 => sched::exit(),
        2 => {
            sched::sleep(a[0]);
            0
        }
        3 => sched::current_id() as u64,
        4 => {
            sched::yield_now();
            0
        }
        _ => ERR,
    }
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
