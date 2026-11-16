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

// entrada a modo usuario: x19 = pc de usuario, x20 = pila de usuario.
// se limpian los registros para no filtrar valores del kernel a EL0.
.global task_user_trampoline
task_user_trampoline:
    bl task_user_start
    msr sp_el0, x20
    msr elr_el1, x19
    msr spsr_el1, xzr
    mov x0, xzr
    mov x1, xzr
    mov x2, xzr
    mov x3, xzr
    mov x4, xzr
    mov x5, xzr
    mov x6, xzr
    mov x7, xzr
    mov x8, xzr
    mov x9, xzr
    mov x10, xzr
    mov x11, xzr
    mov x12, xzr
    mov x13, xzr
    mov x14, xzr
    mov x15, xzr
    mov x16, xzr
    mov x17, xzr
    mov x18, xzr
    mov x19, xzr
    mov x20, xzr
    mov x29, xzr
    mov x30, xzr
    eret
"#
);

extern "C" {
    fn switch_to(old_sp: *mut usize, new_sp: usize);
    fn task_trampoline();
    fn task_user_trampoline();
}

const USER_STACK_PAGES: usize = 2;

#[allow(dead_code)]
struct Task {
    sp: usize,
    stack: usize,  // pila del kernel; 0 = pila de arranque, no se libera
    ustack: usize, // pila de usuario; 0 = tarea de kernel
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
    let t = Box::new(Task { sp: 0, stack: 0, ustack: 0, id: 0, wake: 0 });
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

// crea una tarea con su pila de kernel y un marco inicial para switch_to:
// x19 y x20 llevan datos al trampolin, x30 es la direccion donde "retorna" switch_to.
fn new_task(x19: usize, x20: usize, trampoline: usize, ustack: usize) -> usize {
    let g = irq::save();
    let s = sched();
    let stack = pmm::alloc_pages(STACK_PAGES).expect("sin memoria para pila");
    let sp = stack + STACK_PAGES * pmm::PAGE - FRAME;
    unsafe {
        core::ptr::write_bytes(sp as *mut u8, 0, FRAME);
        *(sp as *mut usize) = x19;
        *((sp + 8) as *mut usize) = x20;
        *((sp + 88) as *mut usize) = trampoline;
    }
    let id = s.next_id;
    s.next_id += 1;
    s.ready.push_back(Box::new(Task { sp, stack, ustack, id, wake: 0 }));
    irq::restore(g);
    id
}

/// tarea de kernel
pub fn spawn(f: fn()) -> usize {
    new_task(f as usize, 0, task_trampoline as *const () as usize, 0)
}

/// tarea que corre en EL0 desde `entry` (direccion en la zona de usuario)
pub fn spawn_user(entry: usize) -> usize {
    let ustack = pmm::alloc_zone(pmm::Zone::User, USER_STACK_PAGES).expect("sin memoria de usuario");
    let top = ustack + USER_STACK_PAGES * pmm::PAGE;
    new_task(entry, top, task_user_trampoline as *const () as usize, ustack)
}

#[no_mangle]
extern "C" fn task_main(entry: usize) -> ! {
    reap();
    let f: fn() = unsafe { core::mem::transmute(entry) };
    f();
    exit()
}

#[no_mangle]
extern "C" fn task_user_start() {
    reap();
}

fn reap() {
    let s = sched();
    for z in s.zombies.drain(..) {
        if z.stack != 0 {
            pmm::free_pages(z.stack, STACK_PAGES);
        }
        if z.ustack != 0 {
            pmm::free_pages(z.ustack, USER_STACK_PAGES);
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
