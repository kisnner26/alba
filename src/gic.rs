// GICv2 de la maquina virt de QEMU.

const GICD: usize = 0x0800_0000;
const GICC: usize = 0x0801_0000;

const GICD_CTLR: usize = GICD;
const GICD_ISENABLER0: usize = GICD + 0x100;
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

pub fn enable(irq: u32) {
    // solo SGI/PPI (0..31) por ahora
    write(GICD_ISENABLER0, 1 << irq);
}

pub fn dispatch() {
    let iar = read(GICC_IAR);
    let id = iar & 0x3ff;
    match id {
        crate::timer::IRQ => crate::timer::tick(),
        1020..=1023 => return, // espuria
        _ => crate::println!("irq sin manejador: {}", id),
    }
    write(GICC_EOIR, iar);
}
