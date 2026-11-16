// programas de usuario de demostracion. corren en EL0.
//
// reglas, porque EL0 solo puede ejecutar y leer la seccion .user (ver linker.ld):
//   - todo codigo va en .user.text y todo dato en .user.rodata
//   - nada de llamadas al kernel salvo `svc`; los ayudantes son #[inline(always)]
//   - sin memcpy/memset ni comprobaciones de limites (llamarian a codigo del kernel):
//     se escribe con punteros y volatile
// cuando haya cargador de programas y espacios de direcciones por proceso, esto desaparece.

use core::arch::asm;
use core::mem::MaybeUninit;
use core::ptr::{read_volatile, write_volatile};

macro_rules! ustr {
    ($name:ident, $s:expr) => {
        #[link_section = ".user.rodata"]
        static $name: [u8; $s.len()] = *$s;
    };
}

ustr!(PREFIJO, b"[usuario pid=");
ustr!(ITER, b"] iteracion ");
ustr!(FIN, b"] fin");
ustr!(NL, b"\n");
ustr!(INTRUSO, b"[intruso] intentando leer memoria del kernel (debe fallar)\n");
ustr!(NO_FALLO, b"[intruso] ERROR: la lectura no fallo\n");

#[inline(always)]
fn syscall(nr: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") a0 => ret,
            in("x1") a1,
            in("x2") a2,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
fn write(s: &[u8]) {
    syscall(0, 1, s.as_ptr() as u64, s.len() as u64);
}

#[inline(always)]
fn exit() -> ! {
    loop {
        syscall(1, 0, 0, 0);
    }
}

/// linea armada en la pila y enviada con un solo write, para que no se mezcle con otras tareas
struct Line {
    b: MaybeUninit<[u8; 96]>,
    n: usize,
}

impl Line {
    #[inline(always)]
    fn new() -> Self {
        Line { b: MaybeUninit::uninit(), n: 0 }
    }

    #[inline(always)]
    fn push(&mut self, c: u8) {
        unsafe { write_volatile((self.b.as_mut_ptr() as *mut u8).add(self.n), c) };
        self.n += 1;
    }

    #[inline(always)]
    fn bytes(&mut self, s: &[u8]) {
        let mut i = 0;
        while i < s.len() {
            self.push(unsafe { *s.as_ptr().add(i) });
            i += 1;
        }
    }

    #[inline(always)]
    fn num(&mut self, mut v: u64) {
        let mut d = 1u64;
        while v / d >= 10 {
            d *= 10;
        }
        loop {
            self.push(b'0' + (v / d) as u8);
            v %= d;
            if d == 1 {
                break;
            }
            d /= 10;
        }
    }

    #[inline(always)]
    fn flush(&self) {
        syscall(0, 1, self.b.as_ptr() as u64, self.n as u64);
    }
}

/// programa bien portado: usa solo syscalls
#[link_section = ".user.text"]
pub extern "C" fn demo() -> ! {
    let pid = syscall(3, 0, 0, 0);
    let mut i = 1u64;
    while i <= 3 {
        let mut l = Line::new();
        l.bytes(&PREFIJO);
        l.num(pid);
        l.bytes(&ITER);
        l.num(i);
        l.bytes(&NL);
        l.flush();
        syscall(2, 7, 0, 0); // dormir 7 ticks
        i += 1;
    }
    let mut l = Line::new();
    l.bytes(&PREFIJO);
    l.num(pid);
    l.bytes(&FIN);
    l.bytes(&NL);
    l.flush();
    exit()
}

/// programa que intenta leer memoria del kernel: el procesador debe pararlo
#[link_section = ".user.text"]
pub extern "C" fn intruso() -> ! {
    write(&INTRUSO);
    let _ = unsafe { read_volatile(0x4008_0000 as *const u32) };
    write(&NO_FALLO);
    exit()
}
