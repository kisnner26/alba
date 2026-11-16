#![no_std]
#![no_main]

use core::arch::global_asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

// punto de entrada: solo el core 0 sigue, los demas se duermen.
// pone la pila, limpia .bss y salta a kmain.
global_asm!(
    r#"
.section .text.boot
.global _start
_start:
    mrs x0, mpidr_el1
    and x0, x0, #3
    cbnz x0, 1f
    ldr x0, =__stack_top
    mov sp, x0
    ldr x0, =__bss_start
    ldr x1, =__bss_end
2:  cmp x0, x1
    b.hs 3f
    str xzr, [x0], #8
    b 2b
3:  bl kmain
1:  wfe
    b 1b
"#
);

// UART PL011 de la maquina virt de QEMU
const UART0: *mut u32 = 0x0900_0000 as *mut u32;

struct Uart;

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                unsafe { UART0.write_volatile(b'\r' as u32) };
            }
            unsafe { UART0.write_volatile(b as u32) };
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        let g = $crate::irq::save();
        let _ = core::fmt::Write::write_fmt(&mut $crate::Uart, format_args!($($arg)*));
        let _ = core::fmt::Write::write_str(&mut $crate::Uart, "\n");
        $crate::irq::restore(g);
    }};
}

mod gic;
mod heap;
mod irq;
mod mmu;
mod pmm;
mod sched;
mod syscall;
mod timer;
mod trap;
mod user;

fn demo_a() {
    for i in 1..=4 {
        println!("[a] paso {}", i);
        sched::sleep(10);
    }
    println!("[a] fin");
}

fn demo_b() {
    for i in 1..=4 {
        println!("[b] paso {}", i);
        sched::sleep(15);
    }
    println!("[b] fin");
}

// nunca cede la cpu: demuestra que el timer la recupera
fn busy(name: &str) -> ! {
    let mut n: u64 = 0;
    loop {
        n = core::hint::black_box(n.wrapping_add(1));
        if n % 100_000_000 == 0 {
            println!("[{}] n={} ticks={}", name, n, timer::ticks());
        }
    }
}

fn demo_busy_1() {
    busy("ocupada-1")
}

// codigo distinto a demo_busy_1 a proposito: si el estado de excepcion de una
// tarea se mezclara con el de otra, esta tarea saltaria al codigo ajeno
fn demo_busy_2() {
    let mut m: u64 = 5;
    loop {
        m = core::hint::black_box(m.wrapping_add(3));
        if m % 300_000_000 == 5 {
            println!("[ocupada-2] m={} ticks={}", m, timer::ticks());
        }
    }
}

extern crate alloc;

#[global_allocator]
static HEAP: heap::KernelHeap = heap::KernelHeap;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    mmu::init();
    println!("alba 0.0.1");
    println!("hola desde mi propio sistema operativo");

    pmm::init();
    println!(
        "memoria: {} KiB libres de {} KiB",
        pmm::free_kib(),
        pmm::total_kib()
    );
    {
        use alloc::{boxed::Box, vec::Vec};
        let before = pmm::free_kib();
        let mut v = Vec::new();
        for i in 0..10_000u64 {
            v.push(i);
        }
        let sum: u64 = v.iter().sum();
        let b = Box::new(42u32);
        let big = alloc::vec![7u8; 1 << 20];
        println!(
            "heap: suma={} box={} big={} usado={} KiB",
            sum,
            b,
            big.len(),
            before - pmm::free_kib()
        );
        drop(big);
        drop(v);
        println!("heap: tras liberar, usado={} KiB", before - pmm::free_kib());
    }

    trap::init();

    #[cfg(feature = "wx-test")]
    unsafe {
        (kmain as *const () as *mut u8).write_volatile(0)
    };

    gic::init();
    timer::init();

    sched::init();
    sched::spawn(demo_a);
    sched::spawn(demo_b);
    sched::spawn(demo_busy_1);
    sched::spawn(demo_busy_2);
    sched::spawn_user(user::demo as *const () as usize);
    sched::spawn_user(user::demo as *const () as usize);
    sched::spawn_user(user::intruso as *const () as usize);

    unsafe { core::arch::asm!("msr daifclr, #2") }; // desenmascara irq

    #[cfg(feature = "crash-test")]
    unsafe {
        core::arch::asm!("brk #0")
    };

    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("panic: {}", info);
    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}
