// MMU con mapeo identidad. nivel 1 (bloques de 1 GiB) + nivel 2 (bloques de 2 MiB) para la RAM.
//
//   0x0000_0000..0x4000_0000  dispositivos (UART, GIC, ...): sin cache, sin ejecucion, sin acceso EL0
//   0x4000_0000..0x4100_0000  zona del kernel (16 MiB). solo EL1, con W^X:
//       texto RX, rodata R, datos/bss/pilas/heap RW sin ejecucion (paginas de 4 KiB en el primer bloque)
//   0x4100_0000..0x4120_0000  codigo de usuario (seccion .user): EL0 lectura/escritura/ejecucion
//   0x4120_0000..fin de RAM   datos de usuario: EL0 lectura/escritura, sin ejecucion
//
// regla del procesador: una pagina escribible desde EL0 nunca se ejecuta en EL1.
// por eso el kernel y el codigo de usuario viven en bloques distintos.
// mas adelante se reemplaza por tablas de 4 KiB con W^X y un espacio por proceso.

use core::arch::asm;

#[repr(C, align(4096))]
struct Table([u64; 512]);

static mut L1: Table = Table([0; 512]);
static mut L2: Table = Table([0; 512]);
// primer bloque de 2 MiB de la RAM, dividido en paginas de 4 KiB para dar permisos
// distintos al texto, a los datos de solo lectura y a los datos del kernel
static mut L3: Table = Table([0; 512]);
// TTBR0 activo: solo se reescribe al cambiar a una tarea con otro espacio de direcciones
static mut CUR_TTBR: u64 = 0;

extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __stack_top: u8;
}

const VALID_BLOCK: u64 = 1;
const TABLE: u64 = 3;
const PAGE_DESC: u64 = 3;
const AF: u64 = 1 << 10;
const AP_EL0: u64 = 1 << 6;
const AP_RO: u64 = 1 << 7;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

pub const RAM_BASE: usize = 0x4000_0000;
pub const KERNEL_ZONE_END: usize = 0x4100_0000;
/// maximo que cubre la tabla de nivel 2 (un bloque de 1 GiB)
pub const MAX_RAM: usize = 1 << 30;
static mut RAM_END: usize = RAM_BASE + (128 << 20);
const BLOCK: usize = 2 << 20;

fn addr(sym: &u8) -> usize {
    sym as *const u8 as usize
}

/// valor de TTBR0 de las tareas del kernel (tabla base, ASID 0)
pub fn kernel_ttbr() -> u64 {
    core::ptr::addr_of!(L1) as u64
}

/// activa un espacio de direcciones (TTBR0 con el ASID en los bits 63:48)
pub fn switch_ttbr(ttbr: u64) {
    unsafe {
        if CUR_TTBR != ttbr {
            CUR_TTBR = ttbr;
            asm!("msr ttbr0_el1, {t}", "isb", t = in(reg) ttbr);
        }
    }
}

/// fin de la RAM (se fija en init a partir del tamano que informa el DTB)
pub fn ram_end() -> usize {
    unsafe { RAM_END }
}

pub fn init(ram_size: usize) {
    unsafe {
        let ram_size = (ram_size.min(MAX_RAM)) & !(BLOCK - 1);
        assert!(ram_size >= 32 << 20, "se necesitan al menos 32 MiB de RAM");
        RAM_END = RAM_BASE + ram_size;

        let l1 = &mut *core::ptr::addr_of_mut!(L1);
        let l2 = &mut *core::ptr::addr_of_mut!(L2);

        // attr 0 = Device-nGnRnE, attr 1 = Normal WB
        l1.0[0] = VALID_BLOCK | (0 << 2) | (2 << 8) | AF | PXN | UXN;
        l1.0[1] = core::ptr::addr_of!(L2) as u64 | TABLE;

        let kernel_blocks = (KERNEL_ZONE_END - RAM_BASE) / BLOCK;
        let all_blocks = ram_size / BLOCK;
        // primer bloque: paginas de 4 KiB con W^X. nada es a la vez escribible y ejecutable.
        let l3 = &mut *core::ptr::addr_of_mut!(L3);
        let (text_s, text_e) = (addr(&__text_start), addr(&__text_end));
        let (ro_s, ro_e) = (addr(&__rodata_start), addr(&__rodata_end));
        assert!(addr(&__stack_top) <= RAM_BASE + BLOCK, "kernel no cabe en el primer bloque");
        for i in 0..512 {
            let pa = RAM_BASE + i * 4096;
            let page = pa as u64 | PAGE_DESC | (1 << 2) | (3 << 8) | AF | UXN;
            l3.0[i] = if pa >= text_s && pa < text_e {
                page | AP_RO // texto: lectura y ejecucion
            } else if pa >= ro_s && pa < ro_e {
                page | AP_RO | PXN // rodata: solo lectura
            } else {
                page | PXN // datos, bss, pilas y memoria libre: lectura y escritura, sin ejecucion
            };
        }
        l2.0[0] = core::ptr::addr_of!(L3) as u64 | TABLE;

        for i in 1..all_blocks {
            let pa = (RAM_BASE + i * BLOCK) as u64;
            let base = pa | VALID_BLOCK | (1 << 2) | (3 << 8) | AF;
            l2.0[i] = if i < kernel_blocks {
                base | PXN | UXN // resto de la zona del kernel: datos, sin ejecucion
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
        let ttbr = kernel_ttbr();
        CUR_TTBR = ttbr;
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
