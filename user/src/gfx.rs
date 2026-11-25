// primitivas de píxel y alfabeto de mapa de bits propio.
pub struct Canvas<'a> {
    pub pixels: &'a mut [u32],
    pub w: usize,
    pub h: usize,
}

impl Canvas<'_> {
    pub fn line(&mut self, x: i32, y: i32, width: i32, color: u32) {
        if (0..self.h as i32).contains(&y) {
            let start = x.max(0).min(self.w as i32) as usize;
            let end = (x + width).max(start as i32).min(self.w as i32) as usize;
            let pos = y as usize * self.w;
            self.pixels[pos + start..pos + end].fill(color);
        }
    }

    pub fn rounded(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
        for row in 0..h {
            let dy = (r - 1 - row.min(h - 1 - row)).max(0);
            let mut inset = 0;
            while (r - inset) * (r - inset) + dy * dy > r * r {
                inset += 1;
            }
            self.line(x + inset, y + row, w - 2 * inset, color);
        }
    }

    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: i32, color: u32) {
        self.bytes(x, y, text.as_bytes(), scale, color);
    }

    pub fn bytes(&mut self, mut x: i32, y: i32, text: &[u8], scale: i32, color: u32) {
        for &ch in text {
            for (row, bits) in glyph(ch).into_iter().enumerate() {
                for col in 0..5 {
                    if bits & (16 >> col) != 0 {
                        for dy in 0..scale {
                            self.line(x + col * scale, y + row as i32 * scale + dy, scale, color);
                        }
                    }
                }
            }
            x += 6 * scale;
        }
    }
}

fn glyph(ch: u8) -> [u8; 7] {
    match ch.to_ascii_lowercase() {
        0x80 => [10, 20, 0, 22, 25, 17, 17],
        b'u' => [0, 0, 17, 17, 17, 19, 13],
        b'g' => [0, 0, 15, 17, 15, 1, 14],
        b'j' => [2, 0, 6, 2, 2, 18, 12],
        b'k' => [16, 16, 18, 20, 24, 20, 18],
        b'p' => [0, 0, 30, 17, 30, 16, 16],
        b'q' => [0, 0, 15, 17, 15, 1, 1],
        b'w' => [0, 0, 17, 17, 21, 21, 10],
        b'x' => [0, 0, 17, 10, 4, 10, 17],
        b'z' => [0, 0, 31, 2, 4, 8, 31],
        b'a' => [0, 0, 14, 1, 15, 17, 15],
        b'b' => [16, 16, 22, 25, 17, 25, 22],
        b'c' => [0, 0, 14, 17, 16, 17, 14],
        b'd' => [1, 1, 13, 19, 17, 19, 13],
        b'e' => [0, 0, 14, 17, 31, 16, 15],
        b'h' => [16, 16, 22, 25, 17, 17, 17],
        b'i' => [4, 0, 12, 4, 4, 4, 14],
        b'l' => [12, 4, 4, 4, 4, 4, 14],
        b'm' => [0, 0, 26, 21, 21, 21, 21],
        b'n' => [0, 0, 22, 25, 17, 17, 17],
        b'o' => [0, 0, 14, 17, 17, 17, 14],
        b'r' => [0, 0, 22, 25, 16, 16, 16],
        b's' => [0, 0, 15, 16, 14, 1, 30],
        b't' => [8, 8, 30, 8, 8, 9, 6],
        b'v' => [0, 0, 17, 17, 17, 10, 4],
        b'y' => [0, 0, 17, 17, 15, 1, 14],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'6' => [6, 8, 16, 30, 17, 17, 14],
        _ => [0; 7],
    }
}
