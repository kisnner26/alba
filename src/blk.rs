// driver de bloque virtio (virtio-blk), version moderna. una sola cola, una peticion a la vez.
// cada peticion usa una cadena de 3 descriptores:
//   [cabecera: tipo, sector] -> [datos: hasta 64 KiB] -> [estado: 1 byte]
//
// eficiencia: una peticion puede llevar muchos sectores (un programa de 22 KiB se lee de una vez)
// y, una vez arrancado el planificador, la tarea que espera se BLOQUEA y la cpu duerme hasta la
// interrupcion de fin de peticion. antes de eso (arranque) se espera por sondeo.
//
// la memoria de la cola y de la peticion sale del gestor de paginas (identidad: fisica = virtual)
// y es cacheable: QEMU la ve coherente. en hardware real haria falta mantenimiento de cache o
// memoria no cacheable para DMA.

use crate::pmm;
use crate::virtio::{self, Mmio};
use core::ptr::{read_volatile, write_bytes, write_volatile};
use core::sync::atomic::{fence, Ordering};

pub const SECTOR: usize = 512;
const QSIZE: usize = 8;
/// maximo de datos por peticion
const MAX_BYTES: usize = 64 * 1024;

const T_IN: u32 = 0; // leer
const T_OUT: u32 = 1; // escribir
const F_NEXT: u16 = 1;
const F_WRITE: u16 = 2; // el dispositivo escribe en este buffer

// disposicion dentro de la pagina de la cola
const DESC_OFF: usize = 0; // 8 descriptores de 16 bytes
const AVAIL_OFF: usize = 128; // flags, idx, ring[8]
const USED_OFF: usize = 256; // flags, idx, ring[8] de {id, len}
// disposicion de la zona de la peticion: 1 pagina de cabecera/estado y despues los datos
const HDR_OFF: usize = 0; // tipo u32, reservado u32, sector u64
const STATUS_OFF: usize = 16;
const DATA_OFF: usize = pmm::PAGE;
const REQ_PAGES: usize = 1 + MAX_BYTES / pmm::PAGE;

struct Blk {
    mmio: Mmio,
    queue: usize,
    req: usize,
    avail_idx: u16,
    used_idx: u16,
    sectors: u64,
    irq_mode: bool, // true: las tareas se bloquean y la interrupcion las despierta
}

static mut BLK: Option<Blk> = None;

fn blk() -> Option<&'static mut Blk> {
    unsafe { (*core::ptr::addr_of_mut!(BLK)).as_mut() }
}

fn w16(addr: usize, v: u16) {
    unsafe { write_volatile(addr as *mut u16, v) };
}
fn r16(addr: usize) -> u16 {
    unsafe { read_volatile(addr as *const u16) }
}
fn w32(addr: usize, v: u32) {
    unsafe { write_volatile(addr as *mut u32, v) };
}
fn w64(addr: usize, v: u64) {
    unsafe { write_volatile(addr as *mut u64, v) };
}

fn pages(n: usize) -> Option<usize> {
    let p = pmm::alloc_pages(n)?;
    unsafe { write_bytes(p as *mut u8, 0, n * pmm::PAGE) };
    Some(p)
}

/// inicializa el primer dispositivo de bloque. devuelve false si no hay o falla.
pub fn init() -> bool {
    let Some(m) = virtio::find(virtio::ID_BLOCK) else {
        return false;
    };
    m.write(virtio::STATUS, 0); // reinicio
    m.write(virtio::STATUS, virtio::ST_ACKNOWLEDGE);
    m.write(virtio::STATUS, virtio::ST_ACKNOWLEDGE | virtio::ST_DRIVER);

    // unica caracteristica aceptada: VIRTIO_F_VERSION_1 (bit 32 = bit 0 de la palabra 1)
    m.write(virtio::DEVICE_FEATURES_SEL, 1);
    if m.read(virtio::DEVICE_FEATURES) & 1 == 0 {
        m.write(virtio::STATUS, virtio::ST_FAILED);
        return false;
    }
    m.write(virtio::DRIVER_FEATURES_SEL, 0);
    m.write(virtio::DRIVER_FEATURES, 0);
    m.write(virtio::DRIVER_FEATURES_SEL, 1);
    m.write(virtio::DRIVER_FEATURES, 1);
    let st = virtio::ST_ACKNOWLEDGE | virtio::ST_DRIVER | virtio::ST_FEATURES_OK;
    m.write(virtio::STATUS, st);
    if m.read(virtio::STATUS) & virtio::ST_FEATURES_OK == 0 {
        m.write(virtio::STATUS, virtio::ST_FAILED);
        return false;
    }

    // cola 0
    m.write(virtio::QUEUE_SEL, 0);
    if m.read(virtio::QUEUE_READY) != 0 || (m.read(virtio::QUEUE_NUM_MAX) as usize) < QSIZE {
        m.write(virtio::STATUS, virtio::ST_FAILED);
        return false;
    }
    let (Some(queue), Some(req)) = (pages(1), pages(REQ_PAGES)) else {
        return false;
    };
    m.write(virtio::QUEUE_NUM, QSIZE as u32);
    let (desc, avail, used) = ((queue + DESC_OFF) as u64, (queue + AVAIL_OFF) as u64, (queue + USED_OFF) as u64);
    m.write(virtio::QUEUE_DESC_LOW, desc as u32);
    m.write(virtio::QUEUE_DESC_HIGH, (desc >> 32) as u32);
    m.write(virtio::QUEUE_DRIVER_LOW, avail as u32);
    m.write(virtio::QUEUE_DRIVER_HIGH, (avail >> 32) as u32);
    m.write(virtio::QUEUE_DEVICE_LOW, used as u32);
    m.write(virtio::QUEUE_DEVICE_HIGH, (used >> 32) as u32);
    m.write(virtio::QUEUE_READY, 1);
    m.write(virtio::STATUS, st | virtio::ST_DRIVER_OK);

    let sectors = m.config_u64(0);
    unsafe {
        *core::ptr::addr_of_mut!(BLK) =
            Some(Blk { mmio: m, queue, req, avail_idx: 0, used_idx: 0, sectors, irq_mode: false });
    }
    true
}

/// pasa a espera por interrupcion. solo debe llamarse cuando las interrupciones ya estan
/// habilitadas y hay planificador; antes de eso las peticiones esperan por sondeo.
pub fn enable_irq() {
    if let Some(b) = blk() {
        crate::gic::enable(b.mmio.irq());
        b.irq_mode = true;
    }
}

/// interrupcion de la ranura virtio: reconoce al dispositivo y despierta a la tarea que espera
pub fn irq(id: u32) {
    let Some(b) = blk() else {
        return;
    };
    if id != b.mmio.irq() {
        return;
    }
    let status = b.mmio.read(virtio::INTERRUPT_STATUS);
    b.mmio.write(virtio::INTERRUPT_ACK, status);
    crate::sched::wake_disk();
}

pub fn sectors() -> u64 {
    blk().map_or(0, |b| b.sectors)
}

/// lee un sector. false si no hay disco, el sector no existe o el dispositivo falla.
pub fn read(sector: u64, buf: &mut [u8; SECTOR]) -> bool {
    read_sectors(sector, buf)
}

/// escribe un sector.
pub fn write(sector: u64, buf: &[u8; SECTOR]) -> bool {
    write_sectors(sector, buf)
}

/// lee `buf.len() / 512` sectores consecutivos. la longitud debe ser multiplo de 512.
pub fn read_sectors(sector: u64, buf: &mut [u8]) -> bool {
    transfer(T_IN, sector, buf.as_mut_ptr(), buf.len())
}

/// escribe `buf.len() / 512` sectores consecutivos.
pub fn write_sectors(sector: u64, buf: &[u8]) -> bool {
    transfer(T_OUT, sector, buf.as_ptr() as *mut u8, buf.len())
}

/// divide la transferencia en peticiones de hasta 64 KiB
fn transfer(kind: u32, sector: u64, data: *mut u8, len: usize) -> bool {
    if len % SECTOR != 0 {
        return false;
    }
    let mut done = 0;
    while done < len {
        let n = (len - done).min(MAX_BYTES);
        if !request(kind, sector + (done / SECTOR) as u64, unsafe { data.add(done) }, n) {
            return false;
        }
        done += n;
    }
    true
}

fn request(kind: u32, sector: u64, data: *mut u8, len: usize) -> bool {
    let Some(b) = blk() else {
        return false;
    };
    if sector + (len / SECTOR) as u64 > b.sectors {
        return false;
    }
    // una peticion a la vez: ninguna otra tarea puede entrar en medio. las interrupciones
    // quedan enmascaradas; si hay que esperar, la tarea se bloquea y otras corren.
    let g = crate::irq::save();
    let (desc, avail, used) = (b.queue + DESC_OFF, b.queue + AVAIL_OFF, b.queue + USED_OFF);
    let (hdr, status, buf) = (b.req + HDR_OFF, b.req + STATUS_OFF, b.req + DATA_OFF);

    w32(hdr, kind);
    w32(hdr + 4, 0);
    w64(hdr + 8, sector);
    unsafe {
        write_volatile(status as *mut u8, 0xff);
        if kind == T_OUT {
            core::ptr::copy_nonoverlapping(data, buf as *mut u8, len);
        }
    }

    let d = |i: usize, addr: usize, len: u32, flags: u16, next: u16| {
        let at = desc + i * 16;
        w64(at, addr as u64);
        w32(at + 8, len);
        w16(at + 12, flags);
        w16(at + 14, next);
    };
    d(0, hdr, 16, F_NEXT, 1);
    d(1, buf, len as u32, F_NEXT | if kind == T_IN { F_WRITE } else { 0 }, 2);
    d(2, status, 1, F_WRITE, 0);

    // publicar la cadena (cabeza = descriptor 0) en el anillo "avail" y avisar al dispositivo
    w16(avail + 4 + (b.avail_idx as usize % QSIZE) * 2, 0);
    fence(Ordering::SeqCst);
    b.avail_idx = b.avail_idx.wrapping_add(1);
    w16(avail + 2, b.avail_idx);
    fence(Ordering::SeqCst);
    b.mmio.write(virtio::QUEUE_NOTIFY, 0);

    // esperar a que el dispositivo marque la peticion como usada
    let mut spins = 0u64;
    while r16(used + 2) == b.used_idx {
        if b.irq_mode {
            // la cpu duerme hasta la interrupcion de fin; se vuelve a comprobar el anillo
            // por si el despertar fue espurio
            crate::sched::wait_disk();
        } else {
            spins += 1;
            if spins > 50_000_000 {
                crate::irq::restore(g);
                return false;
            }
            core::hint::spin_loop();
        }
    }
    fence(Ordering::SeqCst);
    b.used_idx = b.used_idx.wrapping_add(1);

    let ok = unsafe { read_volatile(status as *const u8) } == 0;
    if ok && kind == T_IN {
        unsafe { core::ptr::copy_nonoverlapping(buf as *const u8, data, len) };
    }
    crate::irq::restore(g);
    ok
}
