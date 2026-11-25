// conducto de un solo núcleo. sin temporizadores: despertar depende del otro extremo.
use crate::{irq, sched};
use alloc::boxed::Box;
use core::ptr::addr_of_mut;

const CAPACITY: usize = 512;
const MAX_PIPES: usize = 8;

struct Pipe {
    ends: [usize; 2],
    data: [u8; CAPACITY],
    read: usize,
    len: usize,
}

static mut PIPES: [Option<Box<Pipe>>; MAX_PIPES] = [const { None }; MAX_PIPES];

fn pipes() -> &'static mut [Option<Box<Pipe>>; MAX_PIPES] {
    // solo se toca en el core principal con irq enmascarada; nunca se conserva al bloquear.
    unsafe { &mut *addr_of_mut!(PIPES) }
}

/// crea un conducto. el solicitante conserva ambos extremos hasta arrancar los procesos.
pub fn create() -> Option<usize> {
    let g = irq::save();
    let id = pipes().iter().position(Option::is_none);
    if let Some(id) = id {
        pipes()[id] = Some(Box::new(Pipe { ends: [1, 1], data: [0; CAPACITY], read: 0, len: 0 }));
    }
    irq::restore(g);
    id
}

#[inline(never)]
pub fn acquire(id: usize, end: usize) {
    let g = irq::save();
    pipes()[id].as_mut().unwrap().ends[end] += 1;
    irq::restore(g);
}

#[inline(never)]
pub fn close(id: usize, end: usize) {
    let g = irq::save();
    let pipe = pipes()[id].as_mut().unwrap();
    pipe.ends[end] -= 1;
    if pipe.ends == [0, 0] {
        pipes()[id] = None;
    } else if pipe.ends[end] == 0 {
        sched::wake_pipe(id, 1 - end);
    }
    irq::restore(g);
}

pub fn read(id: usize, out: &mut [u8]) -> Option<usize> {
    if out.is_empty() {
        return Some(0);
    }
    let g = irq::save();
    let len = loop {
        let p = pipes()[id].as_mut().unwrap();
        if p.len != 0 {
            let n = out.len().min(p.len);
            for byte in &mut out[..n] {
                *byte = p.data[p.read];
                p.read = (p.read + 1) % CAPACITY;
            }
            p.len -= n;
            sched::wake_pipe(id, 1);
            break n;
        }
        if p.ends[1] == 0 {
            break 0;
        }
        sched::wait_pipe(id, 0);
    };
    irq::restore(g);
    Some(len)
}

pub fn write(id: usize, data: &[u8]) -> Option<usize> {
    let g = irq::save();
    let mut done = 0;
    while done < data.len() {
        let p = pipes()[id].as_mut().unwrap();
        if p.ends[0] == 0 {
            irq::restore(g);
            return (done != 0).then_some(done);
        }
        let count = (data.len() - done).min(CAPACITY - p.len);
        if count != 0 {
            for &byte in &data[done..done + count] {
                let at = (p.read + p.len) % CAPACITY;
                p.data[at] = byte;
                p.len += 1;
            }
            done += count;
            sched::wake_pipe(id, 0);
        } else {
            sched::wait_pipe(id, 1);
        }
    }
    irq::restore(g);
    Some(done)
}
