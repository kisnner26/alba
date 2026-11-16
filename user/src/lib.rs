//! biblioteca minima para programas de usuario de alba.
//!
//! llamadas al sistema: `svc #0`, numero en x8, argumentos en x0-x2, resultado en x0.
//! la tabla vive en el kernel (src/syscall.rs).

#![no_std]

use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

#[inline(always)]
pub fn syscall(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            options(nostack),
        );
    }
    ret
}

/// escribe bytes en la consola
pub fn write(bytes: &[u8]) -> u64 {
    syscall(0, 1, bytes.as_ptr() as u64, bytes.len() as u64, 0)
}

/// termina el proceso con un codigo de salida (0 = bien)
pub fn exit(code: u64) -> ! {
    loop {
        syscall(1, code, 0, 0, 0);
    }
}

/// duerme `ticks` ticks (1 tick = 100 ms)
pub fn sleep(ticks: u64) {
    syscall(2, ticks, 0, 0, 0);
}

pub fn getpid() -> u64 {
    syscall(3, 0, 0, 0, 0)
}

pub fn yield_now() {
    syscall(4, 0, 0, 0, 0);
}

/// lee una linea de la consola (con su '\n'); bloquea hasta que haya una. devuelve los bytes leidos.
pub fn read_line(buf: &mut [u8]) -> usize {
    syscall(7, 0, buf.as_mut_ptr() as u64, buf.len() as u64, 0) as usize
}

/// espera a que termine el proceso `pid` y devuelve su codigo de salida (u64::MAX si no existe)
pub fn wait(pid: u64) -> u64 {
    syscall(5, pid, 0, 0, 0)
}

/// lanza un programa del disco con argumentos (`args[0]` es, por convencion, el nombre).
/// devuelve el pid, o None si no existe, no es ejecutable o no se pudo cargar.
pub fn exec(path: &str, args: &[&str]) -> Option<u64> {
    let mut blob = [0u8; 256];
    let mut n = 0;
    for a in args {
        if n + a.len() + 1 > blob.len() {
            return None;
        }
        blob[n..n + a.len()].copy_from_slice(a.as_bytes());
        n += a.len();
        blob[n] = 0;
        n += 1;
    }
    let pid = syscall(6, path.as_ptr() as u64, path.len() as u64, blob.as_ptr() as u64, n as u64);
    if pid == u64::MAX {
        None
    } else {
        Some(pid)
    }
}

/// argumentos del proceso, tal como los deja el kernel: `_start(argc, argv)`
pub struct Args {
    argc: usize,
    argv: *const *const u8,
    i: usize,
}

pub fn args(argc: usize, argv: *const *const u8) -> Args {
    Args { argc, argv, i: 0 }
}

impl Iterator for Args {
    type Item = &'static str;
    fn next(&mut self) -> Option<&'static str> {
        if self.i >= self.argc {
            return None;
        }
        let mut p = unsafe { *self.argv.add(self.i) };
        self.i += 1;
        let start = p;
        let mut len = 0;
        while unsafe { *p } != 0 {
            p = unsafe { p.add(1) };
            len += 1;
        }
        let bytes = unsafe { core::slice::from_raw_parts(start, len) };
        Some(core::str::from_utf8(bytes).unwrap_or(""))
    }
}

/// una linea se arma en la pila y sale con un solo write, para que no se mezcle con la
/// salida de otros procesos
struct Line {
    buf: [u8; 128],
    len: usize,
}

impl Line {
    fn flush(&mut self) {
        if self.len > 0 {
            write(&self.buf[..self.len]);
            self.len = 0;
        }
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &b in s.as_bytes() {
            if self.len == self.buf.len() {
                self.flush();
            }
            self.buf[self.len] = b;
            self.len += 1;
        }
        Ok(())
    }
}

#[doc(hidden)]
pub fn _println(args: fmt::Arguments) {
    let mut line = Line { buf: [0; 128], len: 0 };
    let _ = line.write_fmt(args);
    let _ = line.write_str("\n");
    line.flush();
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => { $crate::_println(format_args!($($arg)*)) };
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic en usuario: {}", info);
    exit(101)
}
