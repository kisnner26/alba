//! escritorio que actualiza la pantalla al recibir eventos.
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
    scene(&mut canvas, 0, 0, "archivos y consola", &[]);
    if alba::syscall(22, 0, 0, w as u64, h as u64) != 0 {
        alba::exit(1);
    }
    // comprueba que una actualización pequeña no dependa de refrescar toda la pantalla.
    canvas.rounded(18, 11, 8, 8, 4, 0x5d95ff);
    if alba::syscall(22, 18, 11, 8, 8) != 0 {
        alba::exit(1);
    }
    alba::println!("desktop: listo");
    interact(&mut canvas);
    alba::exit(0)
}

fn flush(x: i32, y: i32, w: i32, h: i32) -> bool {
    alba::syscall(22, x as u64, y as u64, w as u64, h as u64) == 0
}

fn interact(c: &mut Canvas<'_>) {
    let (width, height) = (c.w as i32, c.h as i32);
    let (mut x, mut y) = (width / 2, height / 2);
    let (mut drawn_x, mut drawn_y) = (x, y);
    let (mut shift, mut spanish, mut pressed) = (false, false, false);
    let (mut window_x, mut window_y) = (0, 0);
    let mut drag = false;
    let mut panel = "archivos y consola";
    let mut redraw = false;
    let mut cursor = false;
    let mut reported = [false; 4];
    let mut keys = [b' '; 132];
    let mut notes = false;
    let mut key_len = 0usize;
    let mut saved = [0u32; 36];
    loop {
        let event = alba::syscall(23, 0, 0, 0, 0);
        if event == u64::MAX {
            return;
        }
        let (kind, code, val) = (event as u16, (event >> 16) as u16, (event >> 32) as i32);
        match (kind, code) {
            (3, 0) => x = val.clamp(0, 32767) * (width - 6) / 32767,
            (3, 1) => y = val.clamp(0, 32767) * (height - 6) / 32767,
            (1, 42 | 54) => shift = val != 0,
            (1, 60) if val == 1 => {
                spanish = !spanish;
                if !notes {
                    panel = if spanish { "es latino" } else { "en us" };
                }
                redraw = true;
            }
            (1, 1) if val == 1 && notes => {
                notes = false;
                key_len = 0;
                panel = "archivos y consola";
                redraw = true;
            }
            (1, 61) if val == 1 && !notes => {
                notes = true;
                key_len = load_note(&mut keys).unwrap_or_else(|| {
                    notes = false;
                    0
                });
                panel = if notes { "notas f4 guardar esc salir" } else { "nota no disponible" };
                redraw = true;
            }
            (1, 62) if val == 1 && notes => {
                if save_note(&keys[..key_len]) {
                    panel = "nota guardada";
                    alba::write(b"notas: guardado\n");
                } else {
                    panel = "error al guardar";
                }
                redraw = true;
            }
            (1, 272) => {
                pressed = val != 0;
                if !pressed {
                    drag = false;
                }
                let left = ((width - (width - 64).min(420)) / 2) + window_x;
                let top = (height - 200) / 2 - 10 + window_y;
                if pressed && (left..left + (width - 64).min(420)).contains(&x)
                    && (top..top + 42).contains(&y)
                {
                    drag = true;
                }
                let dock = (width - 188) / 2;
                if pressed && (dock..dock + 188).contains(&x)
                    && (height - 82..height - 30).contains(&y)
                {
                    if notes {
                        continue; // conserva la sesión de edición hasta volver a abrir la nota.
                    }
                    panel = if x < dock + 94 { "archivos" } else { "consola" };
                    report(&mut reported[2], "entrada: dock");
                    redraw = true;
                }
            }
            (1, _) if val >= 1 => {
                if code == 14 {
                    key_len = key_len.saturating_sub(1);
                } else if let Some(ch) = key(code, shift, spanish) {
                    let max = if notes { keys.len() } else { 24 };
                    if key_len < max {
                        keys[key_len] = ch;
                        key_len += 1;
                    }
                } else {
                    continue;
                }
                redraw = true;
                report(&mut reported[0], "entrada: tecla");
            }
            (0, 0) => {
                let moved = x != drawn_x || y != drawn_y;
                if drag && pressed && moved {
                    window_x = (window_x + x - drawn_x).clamp(-width / 4, width / 4);
                    window_y = (window_y + y - drawn_y).clamp(-height / 5, height / 5);
                    report(&mut reported[3], "entrada: ventana");
                    redraw = true;
                }
                if redraw {
                    scene(c, window_x, window_y, panel, &keys[..key_len]);
                    cursor = false;
                } else if moved && cursor {
                    pointer(c, drawn_x, drawn_y, &mut saved, false);
                    flush(drawn_x, drawn_y, 6, 6);
                }
                if redraw || moved {
                    pointer(c, x, y, &mut saved, true);
                    if redraw { flush(0, 0, width, height); } else { flush(x, y, 6, 6); }
                    cursor = true;
                    drawn_x = x;
                    drawn_y = y;
                    redraw = false;
                }
                if moved {
                    report(&mut reported[1], "entrada: puntero");
                }
            }
            _ => {}
        }
    }
}

const NOTE: &str = "notas.txt";

fn load_note(keys: &mut [u8; 132]) -> Option<usize> {
    let Some(fd) = alba::open(NOTE) else { return Some(0) };
    let mut raw = [0; 264];
    let n = alba::read(fd, &mut raw);
    let end = alba::read(fd, &mut [0; 1]);
    alba::close(fd);
    let raw = core::str::from_utf8(&raw[..n?]).ok()?;
    if end != Some(0) {
        return None;
    }
    let mut size = 0;
    for ch in raw.chars() {
        let byte = keys.get_mut(size)?;
        *byte = match ch {
            'ñ' | 'Ñ' => 0x80,
            ch if ch.is_ascii() && !ch.is_ascii_control() => ch as u8,
            _ => return None,
        };
        size += 1;
    }
    Some(size)
}

fn save_note(keys: &[u8]) -> bool {
    let mut raw = [0; 264];
    let mut len = 0;
    for &key in keys {
        if key == 0x80 {
            raw[len..len + 2].copy_from_slice("ñ".as_bytes());
            len += 2;
        } else {
            raw[len] = key;
            len += 1;
        }
    }
    alba::syscall(25, NOTE.as_ptr() as u64, NOTE.len() as u64, raw.as_ptr() as u64, len as u64) == 0
}

fn report(reported: &mut bool, message: &str) {
    if !*reported {
        *reported = true;
        alba::write(message.as_bytes());
        alba::write(b"\n");
    }
}

fn pointer(c: &mut Canvas<'_>, x: i32, y: i32, saved: &mut [u32; 36], draw: bool) {
    for (i, saved) in saved.iter_mut().enumerate() {
        let pos = (y as usize + i / 6) * c.w + x as usize + i % 6;
        if draw {
            *saved = c.pixels[pos];
            c.pixels[pos] = 0x5d95ff;
        } else {
            c.pixels[pos] = *saved;
        }
    }
}

fn key(code: u16, shift: bool, spanish: bool) -> Option<u8> {
    let letter = match code {
        2..=11 => {
            let row = if shift { if spanish { b"!\"#$%&/()=" } else { b"!@#$%^&*()" } } else { b"1234567890" };
            return Some(row[usize::from(code - 2)]);
        }
        16..=25 => b"qwertyuiop"[usize::from(code - 16)],
        30..=38 => b"asdfghjkl"[usize::from(code - 30)],
        44..=50 => b"zxcvbnm"[usize::from(code - 44)],
        39 if spanish => return Some(0x80),
        39 => return Some(if shift { b':' } else { b';' }),
        57 => b' ',
        _ => return None,
    };
    Some(if shift { letter.to_ascii_uppercase() } else { letter })
}

fn scene(c: &mut Canvas<'_>, dx: i32, dy: i32, panel: &str, keys: &[u8]) {
    let (w, h) = (c.w as i32, c.h as i32);
    for y in 0..h {
        let top = 12 + 14 * (h - y) as u32 / h as u32;
        c.line(0, y, w, (top << 16) | ((top + 6) << 8) | (top + 18));
    }
    c.rounded(0, 0, w, 32, 0, 0x171c28);
    c.text(34, 9, "alba", 2, 0xf2f3f9);
    c.text(w - 116, 12, "escritorio", 1, 0xa7b4cd);

    let pw = (w - 64).min(420);
    let px = (w - pw) / 2 + dx;
    let py = (h - 200) / 2 - 10 + dy;
    c.rounded(px - 4, py + 9, pw + 8, 200, 24, 0x0b0e18);
    c.rounded(px, py, pw, 200, 22, 0x485367);
    c.rounded(px + 1, py + 1, pw - 2, 198, 21, 0x262e3f);
    c.rounded(px + 1, py + 1, pw - 2, 42, 20, 0x303a4f);
    c.text(px + 20, py + 17, "sistema", 1, 0xb0c0dd);
    if panel.starts_with("nota") || panel == "error al guardar" {
        c.text(px + 22, py + 65, "notas", 3, 0xf2f3f9);
        for (row, line) in keys.chunks(44).enumerate() {
            c.bytes(px + 23, py + 101 + row as i32 * 10, line, 1, 0x8cabef);
        }
    } else {
        c.text(px + 22, py + 69, "alba", 6, 0xf2f3f9);
        c.bytes(px + 24, py + 174, if keys.is_empty() { b"aarch64" } else { keys }, 1, 0x8cabef);
    }
    c.text(px + 23, py + 134, panel, 1, 0xb0c0dd);
    c.rounded(px + 24, py + 156, pw - 48, 2, 1, 0x3d4a63);

    let dock_x = (w - 188) / 2;
    let dock_y = h - 82;
    c.rounded(dock_x, dock_y + 5, 188, 52, 22, 0x0b0e18);
    c.rounded(dock_x, dock_y, 188, 52, 22, 0x47516a);
    c.rounded(dock_x + 1, dock_y + 1, 186, 50, 21, 0x252e40);
    c.rounded(dock_x + 14, dock_y + 9, 160, 34, 14, 0x34425b);
    c.text(dock_x + 35, dock_y + 22, "archivos", 1, 0xe1e7f6);
    c.text(dock_x + 104, dock_y + 22, "consola", 1, 0xe1e7f6);
}
