// transporte virtio sobre mmio (virtio 1.x, "moderno"), como lo expone QEMU `virt`:
// 32 ranuras de 0x200 bytes desde 0x0a00_0000. cada ranura puede tener un dispositivo.
// QEMU debe lanzarse con `-global virtio-mmio.force-legacy=false` para usar la version 2.

const BASE: usize = 0x0a00_0000;
const STRIDE: usize = 0x200;
const SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976; // "virt"

pub const ID_GPU: u32 = 16;
pub const ID_BLOCK: u32 = 2;

// registros
pub const MAGIC_VALUE: usize = 0x000;
pub const VERSION: usize = 0x004;
pub const DEVICE_ID: usize = 0x008;
pub const DEVICE_FEATURES: usize = 0x010;
pub const DEVICE_FEATURES_SEL: usize = 0x014;
pub const DRIVER_FEATURES: usize = 0x020;
pub const DRIVER_FEATURES_SEL: usize = 0x024;
pub const QUEUE_SEL: usize = 0x030;
pub const QUEUE_NUM_MAX: usize = 0x034;
pub const QUEUE_NUM: usize = 0x038;
pub const QUEUE_READY: usize = 0x044;
pub const QUEUE_NOTIFY: usize = 0x050;
pub const INTERRUPT_STATUS: usize = 0x060;
pub const INTERRUPT_ACK: usize = 0x064;
pub const STATUS: usize = 0x070;
pub const QUEUE_DESC_LOW: usize = 0x080;
pub const QUEUE_DESC_HIGH: usize = 0x084;
pub const QUEUE_DRIVER_LOW: usize = 0x090;
pub const QUEUE_DRIVER_HIGH: usize = 0x094;
pub const QUEUE_DEVICE_LOW: usize = 0x0a0;
pub const QUEUE_DEVICE_HIGH: usize = 0x0a4;
pub const CONFIG: usize = 0x100;

// bits del registro de estado
pub const ST_ACKNOWLEDGE: u32 = 1;
pub const ST_DRIVER: u32 = 2;
pub const ST_DRIVER_OK: u32 = 4;
pub const ST_FEATURES_OK: u32 = 8;
pub const ST_FAILED: u32 = 128;

#[derive(Clone, Copy)]
pub struct Mmio(usize);

impl Mmio {
    pub fn read(&self, reg: usize) -> u32 {
        unsafe { ((self.0 + reg) as *const u32).read_volatile() }
    }

    pub fn write(&self, reg: usize, value: u32) {
        unsafe { ((self.0 + reg) as *mut u32).write_volatile(value) };
    }

    /// numero de interrupcion (SPI) de esta ranura en QEMU virt: la primera ranura es la SPI 16,
    /// o sea INTID 48
    pub fn irq(&self) -> u32 {
        48 + ((self.0 - BASE) / STRIDE) as u32
    }

    /// lee un campo de 64 bits de la configuracion del dispositivo
    pub fn config_u64(&self, off: usize) -> u64 {
        let lo = self.read(CONFIG + off) as u64;
        let hi = self.read(CONFIG + off + 4) as u64;
        hi << 32 | lo
    }
}

fn slot(i: usize) -> Mmio {
    Mmio(BASE + i * STRIDE)
}

fn name(id: u32) -> &'static str {
    match id {
        1 => "red",
        2 => "bloque",
        3 => "consola",
        4 => "entropia",
        16 => "gpu",
        18 => "teclado/raton",
        _ => "otro",
    }
}

/// recorre las ranuras y muestra los dispositivos presentes
pub fn scan() {
    for i in 0..SLOTS {
        let m = slot(i);
        if m.read(MAGIC_VALUE) != MAGIC {
            continue;
        }
        let id = m.read(DEVICE_ID);
        if id == 0 {
            continue; // ranura vacia
        }
        crate::println!("virtio: ranura {} dispositivo {} ({}) version {}", i, id, name(id), m.read(VERSION));
    }
}

/// primer dispositivo moderno del tipo pedido
pub fn find(device_id: u32) -> Option<Mmio> {
    (0..SLOTS).map(slot).find(|m| {
        m.read(MAGIC_VALUE) == MAGIC && m.read(VERSION) == 2 && m.read(DEVICE_ID) == device_id
    })
}
