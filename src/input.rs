// recibe eventos en una cola dma por dispositivo. el consumidor único es el escritorio.
use crate::{gic, irq, pmm, sched, virtio::{self, Mmio}};
use core::ptr::addr_of_mut;
use core::sync::atomic::{fence, Ordering};

const N: usize = 16;
const AVAIL: usize = 256;
const USED: usize = 320;
const EVENTS: usize = 512;
const BUFFER: usize = 64;

struct Device {
    mmio: Mmio,
    page: usize,
    used: u16,
    avail: u16,
}

struct Input {
    devices: [Option<Device>; 2],
    pend: [u64; BUFFER],
    read: usize,
    len: usize,
}

static mut INPUT: Input = Input { devices: [None, None], pend: [0; BUFFER], read: 0, len: 0 };

fn input() -> &'static mut Input {
    // solo lo usa el core principal con irq enmascarada.
    unsafe { &mut *addr_of_mut!(INPUT) }
}

pub fn init() {
    for (index, mmio) in virtio::find_inputs().take(2).enumerate() {
        if let Some(device) = Device::new(mmio) {
            let id = device.mmio.irq();
            input().devices[index] = Some(device);
            gic::enable(id);
        }
    }
}

impl Device {
    fn new(mmio: Mmio) -> Option<Self> {
        let page = pmm::alloc_pages(1)?;
        // página propia, identidad y coherencia dma en qemu.
        unsafe { core::ptr::write_bytes(page as *mut u8, 0, pmm::PAGE) };
        let Some(ready) = mmio.event_queue(page, N as u32, AVAIL, USED) else {
            pmm::free_pages(page, 1);
            return None;
        };
        // el dispositivo aún no puede usar la cola.
        unsafe {
            for i in 0..N {
                let desc = (page + i * 16) as *mut u64;
                desc.write((page + EVENTS + i * 8) as u64);
                desc.add(1).write(8 | (2 << 32)); // solo escritura del dispositivo
                ((page + AVAIL + 4 + i * 2) as *mut u16).write(i as u16);
            }
            fence(Ordering::SeqCst);
            ((page + AVAIL + 2) as *mut u16).write_volatile(N as u16);
        }
        fence(Ordering::SeqCst);
        mmio.write(virtio::STATUS, ready | virtio::ST_DRIVER_OK);
        mmio.write(virtio::QUEUE_NOTIFY, 0);
        Some(Self { mmio, page, used: 0, avail: N as u16 })
    }

    fn receive(&mut self, pending: &mut [u64; BUFFER], read: usize, len: &mut usize) {
        let p = self.page;
        let start = self.avail;
        // los índices del dispositivo se validan antes de acceder a la página.
        unsafe {
            let end = ((p + USED + 2) as *const u16).read_volatile();
            fence(Ordering::SeqCst);
            if usize::from(end.wrapping_sub(self.used)) > N {
                return;
            }
            while self.used != end {
                let row = p + USED + 4 + usize::from(self.used) % N * 8;
                let id = (row as *const u32).read_volatile() as usize;
                let bytes = ((row + 4) as *const u32).read_volatile();
                self.used = self.used.wrapping_add(1);
                if id >= N {
                    continue;
                }
                if bytes == 8 && *len < BUFFER {
                    pending[(read + *len) % BUFFER] = ((p + EVENTS + id * 8) as *const u64).read_volatile();
                    *len += 1;
                }
                ((p + AVAIL + 4 + usize::from(self.avail) % N * 2) as *mut u16).write_volatile(id as u16);
                self.avail = self.avail.wrapping_add(1);
            }
            if self.avail != start {
                fence(Ordering::SeqCst);
                ((p + AVAIL + 2) as *mut u16).write_volatile(self.avail);
                fence(Ordering::SeqCst);
                self.mmio.write(virtio::QUEUE_NOTIFY, 0);
            }
        }
    }
}

pub fn irq(id: u32) -> bool {
    let s = input();
    let Some(dev) = s.devices.iter_mut().flatten().find(|d| d.mmio.irq() == id) else {
        return false;
    };
    let status = dev.mmio.read(virtio::INTERRUPT_STATUS);
    dev.mmio.write(virtio::INTERRUPT_ACK, status);
    if status & 1 != 0 {
        dev.receive(&mut s.pend, s.read, &mut s.len);
        if s.len != 0 {
            sched::wake_graphical_input();
        }
    }
    true
}

pub fn read() -> Option<u64> {
    crate::gpu::framebuffer()?;
    let g = irq::save();
    let value = loop {
        let s = input();
        if s.devices.iter().all(Option::is_none) {
            break None;
        }
        if s.len != 0 {
            let event = s.pend[s.read];
            s.read = (s.read + 1) % BUFFER;
            s.len -= 1;
            break Some(event);
        }
        sched::wait_graphical_input();
    };
    irq::restore(g);
    value
}
