// seccion critica en un solo core: enmascarar irq y restaurar el estado previo.

use core::arch::asm;

pub fn save() -> u64 {
    let daif: u64;
    unsafe {
        asm!("mrs {}, daif", out(reg) daif);
        asm!("msr daifset, #2");
    }
    daif
}

pub fn restore(daif: u64) {
    // bit 7 = I (irq enmascarada). solo se reactiva si estaba activa antes.
    if daif & 0x80 == 0 {
        unsafe { asm!("msr daifclr, #2") };
    }
}
