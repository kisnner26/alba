//! escritorio estático. dibuja una vez y entrega la región al controlador.
#![no_std]
#![no_main]

#[path = "../gfx.rs"]
mod gfx;
use gfx::Canvas;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mode = alba::syscall(20, 0, 0, 0, 0);
    let addr = alba::syscall(21, 0, 0, 0, 0);
    if mode == u64::MAX || addr == u64::MAX {
        alba::exit(1);
    }
    let (w, h) = ((mode >> 32) as usize, mode as u32 as usize);
    // el kernel mapea y otorga de forma exclusiva las páginas descritas por mode.
    let pixels = unsafe { core::slice::from_raw_parts_mut(addr as *mut u32, w * h) };
    let mut canvas = Canvas { pixels, w, h };
    render(&mut canvas);
    if alba::syscall(22, 0, 0, w as u64, h as u64) != 0 {
        alba::exit(1);
    }
    // comprueba que una actualización pequeña no dependa de refrescar toda la pantalla.
    canvas.rounded(18, 11, 8, 8, 4, 0x5d95ff);
    if alba::syscall(22, 18, 11, 8, 8) != 0 {
        alba::exit(1);
    }
    alba::println!("desktop: listo");
    alba::exit(0)
}

fn render(c: &mut Canvas<'_>) {
    let (w, h) = (c.w as i32, c.h as i32);
    for y in 0..h {
        let top = 12 + 14 * (h - y) as u32 / h as u32;
        c.line(0, y, w, (top << 16) | ((top + 6) << 8) | (top + 18));
    }
    c.rounded(0, 0, w, 32, 0, 0x171c28);
    c.text(34, 9, "alba", 2, 0xf2f3f9);
    c.text(w - 116, 12, "escritorio", 1, 0xa7b4cd);

    let pw = (w - 64).min(420);
    let px = (w - pw) / 2;
    let py = (h - 200) / 2 - 10;
    c.rounded(px - 4, py + 9, pw + 8, 200, 24, 0x0b0e18);
    c.rounded(px, py, pw, 200, 22, 0x485367);
    c.rounded(px + 1, py + 1, pw - 2, 198, 21, 0x262e3f);
    c.rounded(px + 1, py + 1, pw - 2, 42, 20, 0x303a4f);
    c.text(px + 20, py + 17, "sistema", 1, 0xb0c0dd);
    c.text(px + 22, py + 69, "alba", 6, 0xf2f3f9);
    c.text(px + 23, py + 134, "archivos y consola", 1, 0xb0c0dd);
    c.rounded(px + 24, py + 156, pw - 48, 2, 1, 0x3d4a63);
    c.text(px + 24, py + 174, "aarch64", 1, 0x8cabef);

    let dock_x = (w - 188) / 2;
    let dock_y = h - 82;
    c.rounded(dock_x, dock_y + 5, 188, 52, 22, 0x0b0e18);
    c.rounded(dock_x, dock_y, 188, 52, 22, 0x47516a);
    c.rounded(dock_x + 1, dock_y + 1, 186, 50, 21, 0x252e40);
    c.rounded(dock_x + 14, dock_y + 9, 160, 34, 14, 0x34425b);
    c.text(dock_x + 35, dock_y + 22, "archivos", 1, 0xe1e7f6);
    c.text(dock_x + 104, dock_y + 22, "consola", 1, 0xe1e7f6);
}
