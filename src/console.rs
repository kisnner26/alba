// consola de entrada: UART PL011 con interrupcion de recepcion (SPI 1 = INTID 33 en QEMU virt).
// el kernel hace de "terminal": acumula la linea, hace eco, atiende el retroceso y solo
// entrega la linea completa (con '\n') cuando se pulsa intro. las tareas que leen se bloquean
// y la interrupcion las despierta: no hay sondeo.

use crate::{irq, sched};
use core::fmt::Write;
use core::ptr::{addr_of_mut, read_volatile, write_volatile};

pub const IRQ: u32 = 33;

const UART: usize = 0x0900_0000;
const DR: usize = UART + 0x00;
const FR: usize = UART + 0x18;
const IMSC: usize = UART + 0x38;
const ICR: usize = UART + 0x44;
const FR_RXFE: u32 = 1 << 4; // fifo de recepcion vacia
const INT_RX: u32 = 1 << 4;
const INT_RT: u32 = 1 << 6; // tiempo de espera de recepcion

const LINE_MAX: usize = 255;
const READY_SIZE: usize = 1024;

struct Console {
    line: [u8; LINE_MAX], // linea que se esta editando
    len: usize,
    ready: [u8; READY_SIZE], // lineas completas, listas para leer (anillo)
    head: usize,
    tail: usize,
}

static mut CON: Console = Console { line: [0; LINE_MAX], len: 0, ready: [0; READY_SIZE], head: 0, tail: 0 };

fn con() -> &'static mut Console {
    unsafe { &mut *addr_of_mut!(CON) }
}

fn echo(s: &str) {
    let _ = crate::Uart.write_str(s);
}

pub fn init() {
    unsafe { write_volatile(IMSC as *mut u32, INT_RX | INT_RT) };
    crate::gic::enable(IRQ);
}

impl Console {
    fn push_ready(&mut self, b: u8) {
        let next = (self.head + 1) % READY_SIZE;
        if next != self.tail {
            self.ready[self.head] = b;
            self.head = next;
        }
    }

    fn input(&mut self, c: u8) -> bool {
        match c {
            b'\r' | b'\n' => {
                echo("\n");
                for i in 0..self.len {
                    let b = self.line[i];
                    self.push_ready(b);
                }
                self.push_ready(b'\n');
                self.len = 0;
                return true;
            }
            0x7f | 0x08 => {
                if self.len > 0 {
                    self.len -= 1;
                    echo("\x08 \x08");
                }
            }
            0x20..=0x7e if self.len < LINE_MAX => {
                self.line[self.len] = c;
                self.len += 1;
                let buf = [c];
                echo(core::str::from_utf8(&buf).unwrap_or("?"));
            }
            _ => {}
        }
        false
    }
}

/// interrupcion de recepcion: vacia la fifo y despierta a los lectores si hay una linea completa
pub fn irq() {
    let mut line_done = false;
    while unsafe { read_volatile(FR as *const u32) } & FR_RXFE == 0 {
        let c = unsafe { read_volatile(DR as *const u32) } as u8;
        line_done |= con().input(c);
    }
    unsafe { write_volatile(ICR as *mut u32, INT_RX | INT_RT) };
    if line_done {
        sched::wake_input();
    }
}

/// lee una linea de la consola (con su '\n'); bloquea hasta que haya una completa.
/// si el buffer es mas corto que la linea, el resto se entrega en la llamada siguiente.
/// devuelve cuantos bytes copio.
pub fn read(buf: &mut [u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    loop {
        let g = irq::save();
        let c = con();
        if c.tail != c.head {
            // una sola linea por llamada (hasta su '\n'), como una terminal canonica
            let mut n = 0;
            while n < buf.len() && c.tail != c.head {
                let b = c.ready[c.tail];
                buf[n] = b;
                c.tail = (c.tail + 1) % READY_SIZE;
                n += 1;
                if b == b'\n' {
                    break;
                }
            }
            irq::restore(g);
            return n;
        }
        // sin datos: con las irq enmascaradas no puede llegar la entrada entre la comprobacion
        // y el bloqueo, asi que no se pierde ningun despertar
        sched::wait_input();
        irq::restore(g);
    }
}
