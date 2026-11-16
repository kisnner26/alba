// transporte virtio sobre mmio (virtio 1.x, "moderno"), como lo expone QEMU `virt`:
// 32 ranuras de 0x200 bytes desde 0x0a00_0000. cada ranura puede tener un dispositivo.
// QEMU debe lanzarse con `-global virtio-mmio.force-legacy=false` para usar la version 2.

const BASE: usize = 0x0a00_0000;
const STRIDE: usize = 0x200;
const SLOTS: usize = 32;
const MAGIC: u32 = 0x7472_6976; // "virt"

// registros
const MAGIC_VALUE: usize = 0x000;
const VERSION: usize = 0x004;
const DEVICE_ID: usize = 0x008;

#[derive(Clone, Copy)]
pub struct Mmio(usize);

impl Mmio {
    fn read(&self, reg: usize) -> u32 {
        unsafe { ((self.0 + reg) as *const u32).read_volatile() }
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
