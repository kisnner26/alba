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

use crate::vm::AddrSpace;
use crate::fs;
use crate::{irq, mmu, pmm, timer};
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

// entrada a modo usuario: x19 = pc de usuario, x20 = pila de usuario, x21 = argc, x22 = argv.
// se limpian los registros para no filtrar valores del kernel a EL0.
.global task_user_trampoline
task_user_trampoline:
    bl task_user_start
    msr sp_el0, x20
    msr elr_el1, x19
    msr spsr_el1, xzr
    mov x0, x21
    mov x1, x22
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
    mov x21, xzr
    mov x22, xzr
    mov x29, xzr
    mov x30, xzr
    eret
"#
);

extern "C" {
    fn switch_to(old_sp: *mut usize, new_sp: usize);
    #[cfg_attr(not(feature = "autotest"), allow(dead_code))]
    fn task_trampoline();
    fn task_user_trampoline();
}

#[allow(dead_code)]
struct Task {
    sp: usize,
    stack: usize,  // pila del kernel; 0 = pila de arranque, no se libera
    space: Option<AddrSpace>, // espacio de direcciones de usuario; None = tarea de kernel
    ttbr: u64,     // espacio de direcciones (TTBR0 con ASID)
    id: usize,
    wake: u64,
    wait: Option<Wait>, // por que esta bloqueada
    ret: u64,           // resultado que recibe al despertar de una espera
}

/// motivo de un bloqueo
#[derive(PartialEq)]
enum Wait {
    Child(usize), // espera a que termine el proceso con ese id
    Input,        // espera entrada de la consola
}

enum Mode {
    Ready,
    Sleep,
    Block,
    Dead,
}

struct Sched {
    current: Option<Box<Task>>,
    ready: VecDeque<Box<Task>>,
    sleeping: Vec<Box<Task>>,
    blocked: Vec<Box<Task>>,
    zombies: Vec<Box<Task>>,
    exited: Vec<(usize, u64)>, // codigos de salida que nadie ha recogido todavia
    next_id: usize,
}

static mut S: Option<Sched> = None;
static NEED: AtomicBool = AtomicBool::new(false);

fn sched() -> &'static mut Sched {
    unsafe { (*addr_of_mut!(S)).as_mut().unwrap() }
}

/// convierte el hilo de arranque en la tarea 0
pub fn init() {
    let t = Box::new(Task { sp: 0, stack: 0, space: None, ttbr: mmu::kernel_ttbr(), id: 0, wake: 0, wait: None, ret: 0 });
    unsafe {
        *addr_of_mut!(S) = Some(Sched {
            current: Some(t),
            ready: VecDeque::new(),
            sleeping: Vec::new(),
            blocked: Vec::new(),
            zombies: Vec::new(),
            exited: Vec::new(),
            next_id: 1,
        });
    }
}

// crea una tarea con su pila de kernel y un marco inicial para switch_to:
// x19-x22 llevan datos al trampolin, x30 es la direccion donde "retorna" switch_to.
fn new_task(regs: [usize; 4], trampoline: usize, space: Option<AddrSpace>) -> usize {
    let g = irq::save();
    let s = sched();
    let stack = pmm::alloc_pages(STACK_PAGES).expect("sin memoria para pila");
    let sp = stack + STACK_PAGES * pmm::PAGE - FRAME;
    unsafe {
        core::ptr::write_bytes(sp as *mut u8, 0, FRAME);
        for (i, r) in regs.iter().enumerate() {
            *((sp + 8 * i) as *mut usize) = *r;
        }
        *((sp + 88) as *mut usize) = trampoline;
    }
    let id = s.next_id;
    s.next_id += 1;
    let ttbr = space.as_ref().map_or(mmu::kernel_ttbr(), |sp| sp.ttbr());
    s.ready.push_back(Box::new(Task { sp, stack, space, ttbr, id, wake: 0, wait: None, ret: 0 }));
    irq::restore(g);
    id
}

/// tarea de kernel (solo la usa la bateria de pruebas)
#[cfg_attr(not(feature = "autotest"), allow(dead_code))]
pub fn spawn(f: fn()) -> usize {
    new_task([f as usize, 0, 0, 0], task_trampoline as *const () as usize, None)
}

/// ejecuta un programa del disco con argumentos (args[0] suele ser el nombre): lo lee de
/// albafs, lo carga en un espacio de direcciones propio, deja argc/argv en su pila y crea el
/// proceso, que arranca en EL0 en el punto de entrada del ELF
pub fn exec_args(name: &str, args: &[&[u8]]) -> Result<usize, &'static str> {
    let file = fs::find(name).ok_or("no existe")?;
    if file.flags & fs::FLAG_EXEC == 0 {
        return Err("no es ejecutable");
    }
    let data = fs::read(name).ok_or("error de lectura")?;
    let (mut space, entry) = AddrSpace::from_elf(&data)?;
    let (sp, argc, argv) = match space.push_args(args) {
        Ok(v) => v,
        Err(e) => {
            space.destroy();
            return Err(e);
        }
    };
    Ok(new_task([entry, sp, argc, argv], task_user_trampoline as *const () as usize, Some(space)))
}

/// ejecuta un programa del disco sin argumentos
pub fn exec(name: &str) -> Result<usize, &'static str> {
    exec_args(name, &[name.as_bytes()])
}

/// el rango de usuario cae dentro de una region mapeada del proceso actual
pub fn user_range_ok(ptr: u64, len: u64) -> bool {
    sched()
        .current
        .as_ref()
        .and_then(|t| t.space.as_ref())
        .is_some_and(|sp| sp.contains(ptr as usize, len as usize))
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
    for mut z in s.zombies.drain(..) {
        if z.stack != 0 {
            pmm::free_pages(z.stack, STACK_PAGES);
        }
        if let Some(space) = z.space.take() {
            space.destroy();
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
    mmu::switch_ttbr(next.ttbr);
    s.current = Some(next);
    match mode {
        Mode::Ready => s.ready.push_back(cur),
        Mode::Sleep => s.sleeping.push(cur),
        Mode::Block => s.blocked.push(cur),
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

const ERR: u64 = u64::MAX;
const MAX_EXITED: usize = 32;

/// termina la tarea actual con un codigo de salida. despierta a quien la esperaba; si nadie la
/// espera, el codigo queda guardado (hasta 32) para un `wait_child` posterior.
pub fn exit_with(code: u64) -> ! {
    irq::save();
    let s = sched();
    let id = s.current.as_ref().unwrap().id;
    let mut waited = false;
    let mut i = 0;
    while i < s.blocked.len() {
        if s.blocked[i].wait == Some(Wait::Child(id)) {
            let mut t = s.blocked.swap_remove(i);
            t.wait = None;
            t.ret = code;
            s.ready.push_back(t);
            waited = true;
        } else {
            i += 1;
        }
    }
    if !waited {
        if s.exited.len() == MAX_EXITED {
            s.exited.remove(0);
        }
        s.exited.push((id, code));
    }
    switch_away(Mode::Dead);
    unreachable!()
}

pub fn exit() -> ! {
    exit_with(0)
}

fn is_alive(s: &Sched, id: usize) -> bool {
    s.current.as_ref().is_some_and(|t| t.id == id)
        || s.ready.iter().any(|t| t.id == id)
        || s.sleeping.iter().any(|t| t.id == id)
        || s.blocked.iter().any(|t| t.id == id)
}

/// espera a que termine el proceso `pid` y devuelve su codigo de salida (u64::MAX si no existe)
pub fn wait_child(pid: usize) -> u64 {
    let g = irq::save();
    let s = sched();
    let ret = if let Some(i) = s.exited.iter().position(|e| e.0 == pid) {
        s.exited.remove(i).1
    } else if pid == s.current.as_ref().unwrap().id || !is_alive(s, pid) {
        ERR
    } else {
        s.current.as_mut().unwrap().wait = Some(Wait::Child(pid));
        switch_away(Mode::Block);
        sched().current.as_ref().unwrap().ret
    };
    irq::restore(g);
    ret
}

/// bloquea la tarea actual hasta que `wake_input` la despierte.
pub fn wait_input() {
    let g = irq::save();
    sched().current.as_mut().unwrap().wait = Some(Wait::Input);
    switch_away(Mode::Block);
    irq::restore(g);
}

/// despierta a las tareas que esperan entrada de la consola (se llama desde la irq)
pub fn wake_input() {
    let s = sched();
    let mut i = 0;
    while i < s.blocked.len() {
        if s.blocked[i].wait == Some(Wait::Input) {
            let mut t = s.blocked.swap_remove(i);
            t.wait = None;
            s.ready.push_back(t);
        } else {
            i += 1;
        }
    }
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
