// MMU con mapeo identidad. nivel 1 (bloques de 1 GiB) + nivel 2 (bloques de 2 MiB) para la RAM.
//
//   0x0000_0000..0x4000_0000  dispositivos (UART, GIC, ...): sin cache, sin ejecucion, sin acceso EL0
//   0x4000_0000..0x4100_0000  zona del kernel (16 MiB): imagen, heap, pilas. solo EL1
//   0x4100_0000..0x4120_0000  codigo de usuario (seccion .user): EL0 lectura/escritura/ejecucion
//   0x4120_0000..0x4800_0000  datos de usuario: EL0 lectura/escritura, sin ejecucion
//
// regla del procesador: una pagina escribible desde EL0 nunca se ejecuta en EL1.
// por eso el kernel y el codigo de usuario viven en bloques distintos.
// mas adelante se reemplaza por tablas de 4 KiB con W^X y un espacio por proceso.

use core::arch::asm;

#[repr(C, align(4096))]
struct Table([u64; 512]);

static mut L1: Table = Table([0; 512]);
static mut L2: Table = Table([0; 512]);

const VALID_BLOCK: u64 = 1;
const TABLE: u64 = 3;
const AF: u64 = 1 << 10;
const AP_EL0: u64 = 1 << 6;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

pub const RAM_BASE: usize = 0x4000_0000;
pub const KERNEL_ZONE_END: usize = 0x4100_0000;
pub const RAM_END: usize = 0x4800_0000;
const BLOCK: usize = 2 << 20;

pub fn init() {
    unsafe {
        let l1 = &mut *core::ptr::addr_of_mut!(L1);
        let l2 = &mut *core::ptr::addr_of_mut!(L2);

        // attr 0 = Device-nGnRnE, attr 1 = Normal WB
        l1.0[0] = VALID_BLOCK | (0 << 2) | (2 << 8) | AF | PXN | UXN;
        l1.0[1] = core::ptr::addr_of!(L2) as u64 | TABLE;

        let kernel_blocks = (KERNEL_ZONE_END - RAM_BASE) / BLOCK;
        let all_blocks = (RAM_END - RAM_BASE) / BLOCK;
        for i in 0..all_blocks {
            let pa = (RAM_BASE + i * BLOCK) as u64;
            let base = pa | VALID_BLOCK | (1 << 2) | (3 << 8) | AF;
            l2.0[i] = if i < kernel_blocks {
                base | UXN // kernel: solo EL1, EL0 no ejecuta
            } else if i == kernel_blocks {
                base | AP_EL0 | PXN // codigo de usuario
            } else {
                base | AP_EL0 | PXN | UXN // datos de usuario
            };
        }

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
