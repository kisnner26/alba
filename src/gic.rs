// GICv2 de la maquina virt de QEMU.

const GICD: usize = 0x0800_0000;
const GICC: usize = 0x0801_0000;

const GICD_CTLR: usize = GICD;
const GICD_ISENABLER: usize = GICD + 0x100; // un bit por interrupcion, 32 por registro
const GICD_ITARGETSR: usize = GICD + 0x800; // un byte por interrupcion: cores destino
const GICC_CTLR: usize = GICC;
const GICC_PMR: usize = GICC + 0x004;
const GICC_IAR: usize = GICC + 0x00c;
const GICC_EOIR: usize = GICC + 0x010;

fn write(addr: usize, v: u32) {
    unsafe { (addr as *mut u32).write_volatile(v) };
}

fn read(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

pub fn init() {
    write(GICD_CTLR, 1);
    write(GICC_PMR, 0xff);
    write(GICC_CTLR, 1);
}

/// habilita una interrupcion. las SPI (32 en adelante) ademas necesitan un core destino.
pub fn enable(irq: u32) {
    if irq >= 32 {
        unsafe { ((GICD_ITARGETSR + irq as usize) as *mut u8).write_volatile(0x01) }; // core 0
    }
    write(GICD_ISENABLER + 4 * (irq as usize / 32), 1 << (irq % 32));
}

pub fn dispatch() {
    let iar = read(GICC_IAR);
    let id = iar & 0x3ff;
    match id {
        crate::timer::IRQ => crate::timer::tick(),
        crate::console::IRQ => crate::console::irq(),
        48..=79 => {
            if !crate::gpu::irq(id) && !crate::input::irq(id) {
                crate::blk::irq(id);
            }
        }
        1020..=1023 => return, // espuria
        _ => crate::println!("irq sin manejador: {}", id),
    }
    write(GICC_EOIR, iar);
}
