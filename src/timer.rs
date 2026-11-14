// timer fisico generico de ARM (PPI 30). 10 ticks por segundo.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

pub const IRQ: u32 = 30;
const HZ: u64 = 10;

static INTERVAL: AtomicU64 = AtomicU64::new(0);
static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn init() {
    let freq: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) freq) };
    let interval = freq / HZ;
    INTERVAL.store(interval, Ordering::Relaxed);
    unsafe {
        asm!("msr cntp_tval_el0, {}", in(reg) interval);
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64); // enable, sin mascara
    }
    crate::gic::enable(IRQ);
}

pub fn tick() {
    let interval = INTERVAL.load(Ordering::Relaxed);
    unsafe { asm!("msr cntp_tval_el0, {}", in(reg) interval) };
    let t = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    if t % HZ == 0 {
        crate::println!("uptime: {}s", t / HZ);
    }
}
