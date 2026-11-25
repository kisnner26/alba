// cola de control virtio-gpu 2d. una sola tarea envía comandos y espera su irq.
use crate::{gic, irq, pmm, sched, virtio::{self, Mmio}};
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{fence, Ordering};

const QUEUE: usize = 8;
const AVAIL: usize = 128;
const USED: usize = 256;
const COMMAND: usize = 512;
const RESPONSE: usize = 1024;
const RESPONSE_BYTES: u32 = 408;
const OK: u32 = 0x1100;

struct Gpu {
    mmio: Mmio,
    page: usize,
}

static mut GPU: Option<Gpu> = None;

#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub addr: usize,
    pub width: u32,
    pub height: u32,
}

static mut FB: Option<(usize, Framebuffer)> = None;

// solo el proceso inicial del escritorio recibe acceso a las páginas de dma.
pub fn framebuffer() -> Option<Framebuffer> {
    let g = irq::save();
    let pid = sched::current_id();
    // se publica con irq enmascarada; ninguna referencia sobrevive a la consulta.
    let fb = unsafe { addr_of!(FB).read() }.filter(|(owner, _)| *owner == pid).map(|(_, fb)| fb);
    irq::restore(g);
    fb
}

pub fn flush(rect: [u64; 4]) -> Option<()> {
    let fb = framebuffer()?;
    let [x, y, w, h] = rect;
    if x.checked_add(w)? > u64::from(fb.width) || y.checked_add(h)? > u64::from(fb.height) {
        return None;
    }
    if w == 0 || h == 0 {
        return Some(());
    }
    device()?.damage(fb.width, [x as u32, y as u32, w as u32, h as u32])
}

#[inline(never)]
pub fn init() -> bool {
    let Some(mmio) = virtio::find(virtio::ID_GPU) else { return false };
    let Some(page) = pmm::alloc_pages(1) else {
        mmio.write(virtio::STATUS, virtio::ST_FAILED);
        return false;
    };
    // página propia del driver, mapeada por identidad y coherente en qemu.
    unsafe { core::ptr::write_bytes(page as *mut u8, 0, pmm::PAGE) };
    let Some(ready) = mmio.event_queue(page, QUEUE as u32, AVAIL, USED) else {
        pmm::free_pages(page, 1);
        return false;
    };
    // se publica una vez, antes de habilitar la interrupción de la ranura.
    unsafe { addr_of_mut!(GPU).write(Some(Gpu { mmio, page })) };
    gic::enable(mmio.irq());
    mmio.write(virtio::STATUS, ready | virtio::ST_DRIVER_OK);
    true
}

fn device() -> Option<&'static Gpu> {
    // una vez publicado, el estado no se modifica.
    unsafe { (*addr_of!(GPU)).as_ref() }
}

pub fn irq(id: u32) -> bool {
    let Some(gpu) = device().filter(|gpu| gpu.mmio.irq() == id) else { return false };
    let status = gpu.mmio.read(virtio::INTERRUPT_STATUS);
    gpu.mmio.write(virtio::INTERRUPT_ACK, status);
    if status & 1 != 0 {
        sched::wake_gpu();
    }
    true
}

impl Gpu {
    fn damage(&self, stride: u32, [x, y, w, h]: [u32; 4]) -> Option<()> {
        let offset = (y * stride + x) * 4;
        if self.command(0x105, &[x, y, w, h, offset, 0, 1, 0]) != OK
            || self.command(0x104, &[x, y, w, h, 1, 0]) != OK
        {
            return None;
        }
        Some(())
    }

    #[inline(never)]
    fn command(&self, kind: u32, body: &[u32]) -> u32 {
        self.exchange(kind, body, false)
    }

    #[inline(never)]
    fn exchange(&self, kind: u32, body: &[u32], fatal: bool) -> u32 {
        let p = self.page;
        let masked = irq::save();
        // la única tarea que usa la cola escribe antes de publicar y lee después de completarse.
        let response = unsafe {
            let word = |off: usize| (p + off) as *mut u32;
            let half = |off: usize| (p + off) as *mut u16;
            let desc = |off, addr: usize, len, flags| {
                ((p + off) as *mut u64).write_volatile(addr as u64);
                word(off + 8).write_volatile(len);
                word(off + 12).write_volatile(flags);
            };
            let current = half(AVAIL + 2).read_volatile();
            // en una falla no se puede esperar al planificador ni pisar un dma pendiente.
            if fatal && half(USED + 2).read_volatile() != current {
                irq::restore(masked);
                return 0;
            }
            word(COMMAND).write_volatile(kind);
            word(COMMAND + 4).write_volatile(1); // fence: terminar antes de responder
            word(COMMAND + 8).write_volatile(u32::from(current));
            for (i, &value) in body.iter().enumerate() {
                word(COMMAND + 24 + i * 4).write_volatile(value);
            }
            word(RESPONSE).write_volatile(0);
            desc(0, p + COMMAND, (24 + body.len() * 4) as u32, (1 << 16) | 1);
            desc(16, p + RESPONSE, RESPONSE_BYTES, 2);
            half(AVAIL + 4 + usize::from(current) % QUEUE * 2).write_volatile(0);
            fence(Ordering::SeqCst);
            half(AVAIL + 2).write_volatile(current.wrapping_add(1));
            fence(Ordering::SeqCst);
            self.mmio.write(virtio::QUEUE_NOTIFY, 0);
            let mut limit = 0;
            while half(USED + 2).read_volatile() == current {
                if fatal {
                    limit += 1;
                    if limit == 1_000_000 {
                        irq::restore(masked);
                        return 0;
                    }
                    core::hint::spin_loop();
                } else {
                    sched::wait_gpu();
                }
            }
            fence(Ordering::SeqCst);
            let done = USED + 4 + usize::from(current) % QUEUE * 8;
            let len = word(done + 4).read_volatile();
            if word(done).read_volatile() != 0 || len < 24 || len > RESPONSE_BYTES {
                0
            } else {
                let kind = word(RESPONSE).read_volatile();
                if kind == 0x1101 && len != RESPONSE_BYTES { 0 } else { kind }
            }
        };
        irq::restore(masked);
        response
    }
}

pub fn start() {
    if draw_test_pattern().is_none() {
        crate::println!("gpu: no se pudo preparar la salida");
    }
}

fn draw_test_pattern() -> Option<()> {
    let gpu = device()?;
    if gpu.command(0x100, &[]) != 0x1101 {
        return None;
    }
    // la respuesta pertenece a esta tarea y la cola no tiene ningún comando en vuelo.
    let read = |offset: usize| unsafe { ((gpu.page + RESPONSE + offset) as *const u32).read_volatile() };
    let scanout = (0..16).find(|&i| read(24 + i * 24 + 16) == 1).unwrap_or(0);
    let mode = 24 + scanout * 24;
    let (width, height) = match (read(mode + 8), read(mode + 12)) {
        (0, _) | (_, 0) => (800, 600),
        (w, h) => (w.min(1024), h.min(768)),
    };
    let bytes = width * height * 4;
    let buffer = pmm::alloc_pages((bytes as usize).div_ceil(pmm::PAGE))?;
    // el buffer aún no se ha expuesto a la gpu. el patrón identifica dos regiones del scanout.
    // las páginas completas se exponen a usuario, incluido el relleno de la última.
    unsafe { core::ptr::write_bytes(buffer as *mut u8, 0, (bytes as usize).div_ceil(pmm::PAGE) * pmm::PAGE) };
    let pixels = unsafe { core::slice::from_raw_parts_mut(buffer as *mut u32, (bytes / 4) as usize) };
    for (y, row) in pixels.chunks_exact_mut(width as usize).enumerate() {
        row.fill(if y < 32 { 0x001e2c42 } else { 0x00121927 });
    }
    let (lo, hi) = (buffer as u32, (buffer as u64 >> 32) as u32);
    if gpu.command(0x101, &[1, 2, width, height]) != OK
        || gpu.command(0x106, &[1, 1, lo, hi, bytes, 0]) != OK
        || gpu.command(0x103, &[0, 0, width, height, scanout as u32, 1]) != OK
        || gpu.damage(width, [0, 0, width, height]).is_none()
    {
        // detener el dispositivo antes de devolver páginas que pudo conservar por dma.
        gpu.mmio.write(virtio::STATUS, 0);
        if gpu.mmio.read(virtio::STATUS) == 0 {
            pmm::free_pages(buffer, (bytes as usize).div_ceil(pmm::PAGE));
        }
        return None;
    }
    crate::println!("gpu: scanout {}x{}", width, height);
    let masked = irq::save();
    let frame = Framebuffer { addr: buffer, width, height };
    // el framebuffer sólo se publica después de configurar la salida.
    unsafe { addr_of_mut!(FB).write(Some((0, frame))) };
    if cfg!(feature = "gpu-panic-test") {
        panic!("fallo de prueba de alba");
    }
    if let Ok(pid) = sched::exec("desktop") {
        // el proceso nuevo todavía no ha podido correr en este core.
        unsafe { addr_of_mut!(FB).write(Some((pid, frame))) };
    }
    irq::restore(masked);
    Some(())
}

struct FatalText<'a> {
    canvas: crate::gfx::Canvas<'a>,
    x: i32,
    y: i32,
}

impl core::fmt::Write for FatalText<'_> {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        for byte in text.bytes() {
            if self.x + 12 > self.canvas.w as i32 || byte == b'\n' {
                self.x = 24;
                self.y += 20;
            }
            if self.y + 14 > self.canvas.h as i32 {
                break;
            }
            if byte != b'\n' {
                self.canvas.bytes(self.x, self.y, &[byte], 2, 0xf1f2f6);
                self.x += 12;
            }
        }
        Ok(())
    }
}

pub fn fatal_screen(message: core::fmt::Arguments<'_>) {
    use core::fmt::Write;
    let Some(gpu) = device() else { return };
    // fatal() ya enmascaró irq. el núcleo actual es el único que puede tocar el framebuffer.
    let Some((_, fb)) = (unsafe { addr_of!(FB).read() }) else { return };
    // con irq enmascarada, sólo el dispositivo podría tocar la cola.
    let idle = unsafe {
        ((gpu.page + AVAIL + 2) as *const u16).read_volatile()
            == ((gpu.page + USED + 2) as *const u16).read_volatile()
    };
    if !idle {
        return;
    }
    // las páginas pertenecen al driver mientras existe el scanout.
    let pixels = unsafe { core::slice::from_raw_parts_mut(fb.addr as *mut u32, (fb.width * fb.height) as usize) };
    pixels.fill(0x171c28);
    let mut content = FatalText {
        canvas: crate::gfx::Canvas { pixels, w: fb.width as usize, h: fb.height as usize },
        x: 24, y: 80,
    };
    content.canvas.line(0, 10, fb.width as i32, 0xea786d);
    content.canvas.text(24, 27, "alba fallo", 3, 0xea786d);
    let _ = content.write_fmt(message);
    if gpu.exchange(0x105, &[0, 0, fb.width, fb.height, 0, 0, 1, 0], true) == OK {
        if gpu.exchange(0x104, &[0, 0, fb.width, fb.height, 1, 0], true) == OK {
            crate::println!("gpu: fallo visible");
        }
    }
}
