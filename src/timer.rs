// temporizador fisico generico de ARM (PPI 30), SIN tics periodicos.
//
// el tiempo se lee del contador de hardware (CNTPCT), que cuenta siempre sin consumir interrupciones.
// el comparador (CNTP_CVAL) solo se programa para el proximo evento que lo necesite: que venza un
// `sleep` o que termine el turno de otra tarea lista. si no hay nada que esperar, se apaga y el
// procesador duerme (wfi) hasta que llegue una interrupcion de verdad: cero despertares en reposo.
//
// la unidad de tiempo del sistema es el "tick" (100 ms), pero ya no es una interrupcion: es solo
// una forma de contar el tiempo.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

pub const IRQ: u32 = 30;
const HZ: u64 = 10;

/// pasos del contador por tick (frecuencia / HZ)
static BOOT_START: AtomicU64 = AtomicU64::new(0);
static FREQUENCY: AtomicU64 = AtomicU64::new(0);

static INTERVAL: AtomicU64 = AtomicU64::new(0);
/// interrupciones de timer atendidas desde el arranque (para medir cuanto despierta al procesador)
static IRQS: AtomicU64 = AtomicU64::new(0);

fn count() -> u64 {
    let c: u64;
    // isb: la lectura no puede adelantarse a instrucciones anteriores
    unsafe { asm!("isb", "mrs {}, cntpct_el0", out(reg) c) };
    c
}

pub fn init(boot_start: u64) {
    let freq: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) freq) };
    BOOT_START.store(boot_start, Ordering::Relaxed);
    FREQUENCY.store(freq, Ordering::Relaxed);
    INTERVAL.store(freq / HZ, Ordering::Relaxed);
    disarm();
    crate::gic::enable(IRQ);
}

/// microsegundos del contador físico desde la entrada en _start.
pub fn micros() -> u64 {
    let elapsed = count().wrapping_sub(BOOT_START.load(Ordering::Relaxed));
    let freq = FREQUENCY.load(Ordering::Relaxed);
    (u128::from(elapsed) * 1_000_000 / u128::from(freq)) as u64
}

/// tiempo desde el arranque, en ticks de 100 ms
pub fn ticks() -> u64 {
    match INTERVAL.load(Ordering::Relaxed) {
        0 => 0,
        i => count() / i,
    }
}

/// programa la interrupcion para el instante `deadline` (en ticks). un instante pasado
/// dispara de inmediato.
pub fn arm(deadline: u64) {
    let cval = deadline.saturating_mul(INTERVAL.load(Ordering::Relaxed));
    unsafe {
        asm!("msr cntp_cval_el0, {}", in(reg) cval);
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64); // habilitado, sin mascara
    }
}

/// apaga el comparador: no habra mas interrupciones de timer hasta el proximo `arm`
pub fn disarm() {
    unsafe { asm!("msr cntp_ctl_el0, {}", in(reg) 0u64) };
}

/// interrupcion del comparador. se apaga para soltar la linea (seguiria activa mientras el
/// contador supere el comparador); el planificador decide si hay que reprogramarlo.
pub fn tick() {
    IRQS.fetch_add(1, Ordering::Relaxed);
    disarm();
    crate::sched::on_tick();
}

pub fn irq_count() -> u64 {
    IRQS.load(Ordering::Relaxed)
}
