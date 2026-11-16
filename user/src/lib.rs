//! biblioteca minima para programas de usuario de alba.
//!
//! llamadas al sistema: `svc #0`, numero en x8, argumentos en x0-x2, resultado en x0.
//! la tabla vive en el kernel (src/syscall.rs).

#![no_std]

use core::arch::asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

#[inline(always)]
pub fn syscall(nr: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            options(nostack),
        );
    }
    ret
}

/// escribe bytes en la consola
pub fn write(bytes: &[u8]) -> u64 {
    syscall(0, 1, bytes.as_ptr() as u64, bytes.len() as u64)
}

/// termina el proceso
pub fn exit() -> ! {
    loop {
        syscall(1, 0, 0, 0);
    }
}

/// duerme `ticks` ticks (1 tick = 100 ms)
pub fn sleep(ticks: u64) {
    syscall(2, ticks, 0, 0);
}

pub fn getpid() -> u64 {
    syscall(3, 0, 0, 0)
}

pub fn yield_now() {
    syscall(4, 0, 0, 0);
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
    exit()
}
