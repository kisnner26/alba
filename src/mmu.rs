// MMU con mapeo identidad usando bloques de 1 GiB (nivel 1, 4 KiB de granulo).
//   0x0000_0000..0x4000_0000  dispositivos (UART, GIC, ...), sin cache
//   0x4000_0000..0x8000_0000  RAM, cacheable write-back
// encender MMU y caches es la mayor ganancia de velocidad disponible ahora.
// mas adelante se reemplaza por tablas finas con W^X y espacios por proceso.

use core::arch::asm;

#[repr(C, align(4096))]
struct Table([u64; 512]);

static mut L1: Table = Table([0; 512]);

const VALID_BLOCK: u64 = 1;
const AF: u64 = 1 << 10;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

pub fn init() {
    unsafe {
        let l1 = &mut *core::ptr::addr_of_mut!(L1);
        // attr 0 = Device-nGnRnE, attr 1 = Normal WB
        l1.0[0] = VALID_BLOCK | (0 << 2) | (2 << 8) | AF | PXN | UXN;
        l1.0[1] = 0x4000_0000 | VALID_BLOCK | (1 << 2) | (3 << 8) | AF;

        let mair: u64 = 0xff00;
        let mmfr0: u64;
        asm!("mrs {}, id_aa64mmfr0_el1", out(reg) mmfr0);
        // T0SZ=25 (39 bits), WB inner/outer, inner shareable, TTBR1 apagado
        let tcr: u64 = 25 | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 23) | ((mmfr0 & 7) << 32);
        let ttbr = core::ptr::addr_of!(L1) as u64;
        let bits: u64 = (1 << 0) | (1 << 2) | (1 << 12); // M, C, I

        asm!(
            "msr mair_el1, {mair}",
            "msr tcr_el1, {tcr}",
            "msr ttbr0_el1, {ttbr}",
            "isb",
            "tlbi vmalle1",
            "ic iallu",
            "dsb ish",
            "isb",
            "mrs {t}, sctlr_el1",
            "orr {t}, {t}, {bits}",
            "msr sctlr_el1, {t}",
            "isb",
            mair = in(reg) mair,
            tcr = in(reg) tcr,
            ttbr = in(reg) ttbr,
            bits = in(reg) bits,
            t = out(reg) _,
        );
    }
}
