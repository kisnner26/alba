// tareas del kernel y planificador round-robin con apropiacion por timer.
//
// - cada tarea tiene su propia pila de 16 KiB
// - cambio de contexto: solo se guardan los registros callee-saved (x19-x30) y sp
// - el timer marca NEED cada tick (100 ms); tras el EOI de la irq se llama a
//   preempt_check, que cambia de tarea dentro del propio manejador
// - tareas dormidas no consumen cpu: solo vuelven a la cola cuando vence su tick
// - kmain es la tarea 0 y hace de idle (wfe)
//
// toda mutacion del planificador ocurre con irq enmascaradas (un solo core).

use crate::{irq, pmm, timer};
use alloc::{boxed::Box, collections::VecDeque, vec::Vec};
use core::arch::global_asm;
use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicBool, Ordering};

const STACK_PAGES: usize = 4;
const FRAME: usize = 96; // x19..x30

global_asm!(
    r#"
.section .text
.global switch_to
switch_to:
    sub sp, sp, #96
    stp x19, x20, [sp, #0]
    stp x21, x22, [sp, #16]
    stp x23, x24, [sp, #32]
    stp x25, x26, [sp, #48]
    stp x27, x28, [sp, #64]
    stp x29, x30, [sp, #80]
    mov x2, sp
    str x2, [x0]
    mov sp, x1
    ldp x19, x20, [sp, #0]
    ldp x21, x22, [sp, #16]
    ldp x23, x24, [sp, #32]
    ldp x25, x26, [sp, #48]
    ldp x27, x28, [sp, #64]
    ldp x29, x30, [sp, #80]
    add sp, sp, #96
    ret

.global task_trampoline
task_trampoline:
    msr daifclr, #2
    mov x0, x19
    bl task_main
    b .
"#
);

extern "C" {
    fn switch_to(old_sp: *mut usize, new_sp: usize);
    fn task_trampoline();
}

#[allow(dead_code)]
struct Task {
    sp: usize,
    stack: usize, // 0 = pila de arranque, no se libera
    id: usize,
    wake: u64,
}

enum Mode {
    Ready,
    Sleep,
    Dead,
}

struct Sched {
    current: Option<Box<Task>>,
    ready: VecDeque<Box<Task>>,
    sleeping: Vec<Box<Task>>,
    zombies: Vec<Box<Task>>,
    next_id: usize,
}

static mut S: Option<Sched> = None;
static NEED: AtomicBool = AtomicBool::new(false);

fn sched() -> &'static mut Sched {
    unsafe { (*addr_of_mut!(S)).as_mut().unwrap() }
}

/// convierte el hilo de arranque en la tarea 0
pub fn init() {
    let t = Box::new(Task { sp: 0, stack: 0, id: 0, wake: 0 });
    unsafe {
        *addr_of_mut!(S) = Some(Sched {
            current: Some(t),
            ready: VecDeque::new(),
            sleeping: Vec::new(),
            zombies: Vec::new(),
            next_id: 1,
        });
    }
}

pub fn spawn(f: fn()) -> usize {
    let g = irq::save();
    let s = sched();
    let stack = pmm::alloc_pages(STACK_PAGES).expect("sin memoria para pila");
    let sp = stack + STACK_PAGES * crate::pmm::PAGE - FRAME;
    unsafe {
        core::ptr::write_bytes(sp as *mut u8, 0, FRAME);
        *(sp as *mut usize) = f as usize; // x19: funcion de entrada
        *((sp + 88) as *mut usize) = task_trampoline as *const () as usize; // x30: direccion de retorno
    }
    let id = s.next_id;
    s.next_id += 1;
    s.ready.push_back(Box::new(Task { sp, stack, id, wake: 0 }));
    irq::restore(g);
    id
}

#[no_mangle]
extern "C" fn task_main(entry: usize) -> ! {
    reap();
    let f: fn() = unsafe { core::mem::transmute(entry) };
    f();
    exit()
}

fn reap() {
    let s = sched();
    for z in s.zombies.drain(..) {
        if z.stack != 0 {
            pmm::free_pages(z.stack, STACK_PAGES);
        }
    }
}

fn wake_sleepers(s: &mut Sched) {
    let now = timer::ticks();
    let mut i = 0;
    while i < s.sleeping.len() {
        if s.sleeping[i].wake <= now {
            let t = s.sleeping.swap_remove(i);
            s.ready.push_back(t);
        } else {
            i += 1;
        }
    }
}

// irq debe estar enmascarada al llamar
fn switch_away(mode: Mode) {
    let s = sched();
    wake_sleepers(s);
    let Some(next) = s.ready.pop_front() else {
        match mode {
            Mode::Ready => return,
            _ => panic!("no hay tareas listas"),
        }
    };
    let mut cur = s.current.take().unwrap();
    let old_sp: *mut usize = &mut cur.sp;
    let new_sp = next.sp;
    s.current = Some(next);
    match mode {
        Mode::Ready => s.ready.push_back(cur),
        Mode::Sleep => s.sleeping.push(cur),
        Mode::Dead => s.zombies.push(cur),
    }
    unsafe { switch_to(old_sp, new_sp) };
    // aqui se vuelve cuando esta tarea retoma la cpu
    reap();
}

pub fn current_id() -> usize {
    sched().current.as_ref().unwrap().id
}

pub fn yield_now() {
    let g = irq::save();
    switch_away(Mode::Ready);
    irq::restore(g);
}

pub fn sleep(ticks: u64) {
    let g = irq::save();
    sched().current.as_mut().unwrap().wake = timer::ticks() + ticks;
    switch_away(Mode::Sleep);
    irq::restore(g);
}

pub fn exit() -> ! {
    irq::save();
    switch_away(Mode::Dead);
    unreachable!()
}

/// llamado por el timer en cada tick
pub fn on_tick() {
    NEED.store(true, Ordering::Relaxed);
}

/// llamado al final del manejador de irq, ya hecho el EOI
pub fn preempt_check() {
    if NEED.swap(false, Ordering::Relaxed) {
        switch_away(Mode::Ready);
    }
}
