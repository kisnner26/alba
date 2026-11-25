#![no_std]
#![no_main]

use core::arch::global_asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

// cabecera de imagen arm64 (protocolo de arranque de linux): con ella QEMU y los
// firmwares reales entregan la direccion del DTB en x0 y cargan la imagen en text_offset.
// punto de entrada: solo el core 0 sigue, los demas se duermen.
// pone la pila, limpia .bss y salta a kmain con el DTB en x0.
global_asm!(
    r#"
.section .text.boot
.global _start
_start:
    b 4f                        // code0: salta al codigo real
    .long 0                     // code1
    .quad 0x80000               // text_offset
    .quad __image_size          // image_size
    .quad 2                     // flags: little endian, paginas de 4 KiB
    .quad 0                     // reservados
    .quad 0
    .quad 0
    .long 0x644d5241            // magic "ARM\x64"
    .long 0
4:  mrs x21, cntpct_el0
    mov x20, x0
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
3:  mov x0, x20
    mov x1, x21
    bl kmain
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

mod blk;
mod cache;
mod console;
mod elf;
mod fdt;
mod fs;
mod gic;
#[path = "../user/src/gfx.rs"]
#[allow(dead_code)]
mod gfx;
mod gpu;
mod heap;
mod input;
mod irq;
mod mmu;
mod pipe;
mod pmm;
mod sched;
mod syscall;
mod timer;
mod trap;
mod virtio;
mod vm;

#[cfg(feature = "autotest")]
fn disk_concurrent() {
    // el sector de relleno aun no esta en cache, asi se fuerza un acceso a virtio.
    let sector = blk::sectors() - 2;
    for _ in 0..8 {
        let mut data = [0xff; blk::SECTOR];
        if !blk::read(sector, &mut data) || data != [0; blk::SECTOR] {
            println!("FALLO: lectura concurrente de disco");
            return;
        }
    }
    println!("disco concurrente: lecturas correctas, esperas={}", blk::contentions());
}

#[cfg(feature = "autotest")]
fn demo_a() {
    for i in 1..=4 {
        println!("[a] paso {}", i);
        sched::sleep(10);
    }
    println!("[a] fin");
}

#[cfg(feature = "autotest")]
fn demo_b() {
    for i in 1..=4 {
        println!("[b] paso {}", i);
        sched::sleep(15);
    }
    println!("[b] fin");
}

#[cfg(feature = "autotest")]
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

#[cfg(feature = "autotest")]
fn demo_busy_1() {
    busy("ocupada-1")
}

#[cfg(feature = "autotest")]
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
pub extern "C" fn kmain(dtb: usize, boot_start: u64) -> ! {
    println!("alba 0.0.1");
    #[cfg(feature = "autotest")]
    println!("hola desde mi propio sistema operativo");
    if fdt::is_valid(dtb) {
        println!("dtb: {:#x} ({} bytes)", dtb, fdt::total_size(dtb));
    }
    match fdt::ram(dtb) {
        Some(r) => println!("ram segun dtb: base={:#x} tamano={} MiB", r.base, r.size >> 20),
        None => println!("sin dtb valido"),
    }
    let ram_size = fdt::ram(dtb).map_or(128 << 20, |r| r.size);
    mmu::init(ram_size);

    pmm::init();
    if fdt::is_valid(dtb) {
        pmm::reserve(dtb, fdt::total_size(dtb)); // el DTB vive en RAM; no entregarlo a nadie
    }
    println!(
        "memoria: {} KiB libres de {} KiB",
        pmm::free_kib(),
        pmm::total_kib()
    );
    #[cfg(feature = "autotest")]
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

    #[cfg(feature = "autotest")]
    virtio::scan();
    if blk::init() {
        println!("disco: {} sectores de {} bytes", blk::sectors(), blk::SECTOR);
        // diagnóstico sobre el disco desechable de la batería.
        #[cfg(feature = "autotest")]
        {
            let mut sector = [0u8; blk::SECTOR];
            if blk::read(0, &mut sector) {
                println!("disco: sector 0 = {:?}", core::str::from_utf8(&sector[..8]).unwrap_or("?"));
            }
            let last = blk::sectors() - 1;
            let pattern = [0xa5u8; blk::SECTOR];
            let mut back = [0u8; blk::SECTOR];
            let ok = blk::read(last, &mut back)
                && blk::write(last, &pattern)
                && blk::read(last, &mut back)
                && back == pattern;
            println!("disco: escritura y lectura del sector {} {}", last, if ok { "ok" } else { "FALLO" });
        }

        match fs::mount() {
            Ok(()) => {
                println!("albafs: {} archivos", fs::count());
                #[cfg(feature = "autotest")]
                for e in (0..fs::count()).filter_map(fs::list) {
                    let x = if e.flags & fs::FLAG_EXEC != 0 { " ejecutable" } else { "" };
                    println!("  {} ({} bytes{})", e.name, e.size, x);
                }
                #[cfg(feature = "autotest")]
                {
                    let mut text = [0; 64];
                    if let Some(n) = fs::read_named("hola.txt", 0, &mut text) {
                        println!("hola.txt: {}", core::str::from_utf8(&text[..n]).unwrap_or("?").trim_end());
                    }
                }
            }
            Err(e) => println!("albafs: FALLO al montar: {}", e),
        }
    } else {
        println!("disco: no hay dispositivo de bloque");
    }

    trap::init();

    #[cfg(feature = "wx-test")]
    unsafe {
        (kmain as *const () as *mut u8).write_volatile(0)
    };

    gic::init();
    timer::init(boot_start);
    console::init();

    sched::init();
    if gpu::init() {
        input::init();
        sched::spawn(gpu::start);
    }

    // bateria de pruebas: tareas de kernel, procesos de usuario y casos que el cargador debe
    // rechazar. solo con la feature autotest; el arranque normal va directo a la shell.
    #[cfg(feature = "autotest")]
    {
        sched::spawn(disk_concurrent);
        sched::spawn(disk_concurrent);
        sched::spawn(demo_a);
        sched::spawn(demo_b);
        sched::spawn(demo_busy_1);
        sched::spawn(demo_busy_2);
        // dos copias de hola: cada una en su propio espacio de direcciones
        for prog in ["hola", "hola", "intruso", "intruso2", "lanzador", "basura", "hola.txt", "noexiste"] {
            match sched::exec(prog) {
                Ok(pid) => println!("exec {}: pid {}", prog, pid),
                Err(e) => println!("exec {}: rechazado ({})", prog, e),
            }
        }
    }

    // proceso inicial: la shell, cargada desde el disco
    match sched::exec("sh") {
        Ok(pid) => println!("init: shell lanzada (pid {})", pid),
        Err(e) => println!("init: no se pudo lanzar la shell ({})", e),
    }

    blk::enable_irq(); // las tareas nuevas ya pueden esperar al disco por irq.
    unsafe { core::arch::asm!("msr daifclr, #2") };

    #[cfg(feature = "crash-test")]
    unsafe {
        core::arch::asm!("brk #0")
    };

    // la primera tarea ya está lista. no hay que esperar al primer turno del timer.
    sched::yield_now();

    // tarea idle: el procesador duerme (wfi) hasta que llegue una interrupcion. con el timer
    // apagado cuando no hay eventos, en reposo no se despierta ni una vez.
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let (sp, lr): (u64, u64);
    // guarda el contexto del manejador sin tocar la pila.
    unsafe { core::arch::asm!("mov {sp}, sp", "mov {lr}, x30",
        sp = out(reg) sp, lr = out(reg) lr, options(nomem, nostack, preserves_flags)); }
    fatal(format_args!("panic: {}\nsp={:#x} lr={:#x}", info, sp, lr))
}

pub fn fatal(message: fmt::Arguments<'_>) -> ! {
    irq::save();
    println!("{}", message);
    gpu::fatal_screen(message);
    loop {
        unsafe { core::arch::asm!("wfe") };
    }
}
