// tabla de vectores de excepciones de EL1 y manejadores.
// el kernel corre en EL1 con SP_EL1, asi que las entradas que importan
// son 0x200 (sync) y 0x280 (irq). todo lo demas va a handle_bad.

use core::arch::{asm, global_asm};

global_asm!(
    r#"
.macro save_regs
    sub sp, sp, #272
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    // estado de excepcion: es por tarea, no global, asi que va en el marco
    mrs x9, elr_el1
    mrs x10, spsr_el1
    stp x9, x10, [sp, #248]
    mrs x9, sp_el0
    str x9, [sp, #264]
.endm

.macro restore_regs
    ldp x9, x10, [sp, #248]
    msr elr_el1, x9
    msr spsr_el1, x10
    ldr x9, [sp, #264]
    msr sp_el0, x9
    ldp x0, x1, [sp, #0]
    ldp x2, x3, [sp, #16]
    ldp x4, x5, [sp, #32]
    ldp x6, x7, [sp, #48]
    ldp x8, x9, [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #272
.endm

.macro ventry target
    .balign 128
    b \target
.endm

.section .text.vectors, "ax"
.balign 2048
.global vectors
vectors:
    ventry h_bad
    ventry h_bad
    ventry h_bad
    ventry h_bad
    ventry h_sync
    ventry h_irq
    ventry h_bad
    ventry h_bad
    ventry h_sync_el0
    ventry h_irq
    ventry h_bad
    ventry h_bad
    ventry h_bad
    ventry h_bad
    ventry h_bad
    ventry h_bad

.section .text
h_sync:
    save_regs
    bl handle_sync
    restore_regs
    eret
h_sync_el0:
    save_regs
    mov x0, sp
    bl handle_sync_el0
    restore_regs
    eret
h_irq:
    save_regs
    bl handle_irq
    restore_regs
    eret
h_bad:
    save_regs
    bl handle_bad
    b .
"#
);

extern "C" {
    static vectors: u8;
}

pub fn init() {
    unsafe {
        asm!(
            "msr vbar_el1, {v}",
            "isb",
            v = in(reg) core::ptr::addr_of!(vectors) as u64,
        );
    }
}

fn read_esr() -> (u64, u64, u64) {
    let (esr, elr, far): (u64, u64, u64);
    unsafe {
        asm!("mrs {}, esr_el1", out(reg) esr);
        asm!("mrs {}, elr_el1", out(reg) elr);
        asm!("mrs {}, far_el1", out(reg) far);
    }
    (esr, elr, far)
}

fn halt() -> ! {
    loop {
        unsafe { asm!("wfe") };
    }
}

#[no_mangle]
extern "C" fn handle_sync() {
    let (esr, elr, far) = read_esr();
    println!(
        "excepcion sincrona: clase={:#04x} esr={:#x} elr={:#x} far={:#x}",
        esr >> 26,
        esr,
        elr,
        far
    );
    halt();
}

/// marco guardado en la pila por save_regs (272 bytes)
#[repr(C)]
pub struct Frame {
    pub x: [u64; 31],
    pub elr: u64,
    pub spsr: u64,
    pub sp_el0: u64,
}

const EC_SVC64: u64 = 0x15;

/// excepciones sincronas que vienen de modo usuario: syscalls y fallos
#[no_mangle]
extern "C" fn handle_sync_el0(f: &mut Frame) {
    let (esr, elr, far) = read_esr();
    let ec = esr >> 26;
    if ec == EC_SVC64 {
        // convencion: numero en x8, argumentos en x0-x4, resultado en x0
        f.x[0] = crate::syscall::dispatch(f.x[8], [f.x[0], f.x[1], f.x[2], f.x[3], f.x[4]]);
        return;
    }
    println!(
        "fallo en modo usuario: tarea {} clase={:#04x} esr={:#x} elr={:#x} far={:#x}; tarea terminada",
        crate::sched::current_id(),
        ec,
        esr,
        elr,
        far
    );
    crate::sched::exit();
}

#[no_mangle]
extern "C" fn handle_irq() {
    crate::gic::dispatch();
    crate::sched::preempt_check();
}

#[no_mangle]
extern "C" fn handle_bad() {
    let (esr, elr, far) = read_esr();
    println!(
        "excepcion inesperada: esr={:#x} elr={:#x} far={:#x}",
        esr, elr, far
    );
    halt();
}
