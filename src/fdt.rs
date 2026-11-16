// lector minimo de device tree (FDT): recorre el bloque de estructura y devuelve
// el tamano del DTB y la region de RAM del nodo `memory`.
// solo hace lecturas alineadas de 32 bits y de bytes, asi que se puede usar con la MMU apagada.

const MAGIC: u32 = 0xd00d_feed;
const BEGIN_NODE: u32 = 1;
const END_NODE: u32 = 2;
const PROP: u32 = 3;
const NOP: u32 = 4;

pub struct Ram {
    pub base: usize,
    pub size: usize,
}

fn be32(addr: usize) -> u32 {
    u32::from_be(unsafe { (addr as *const u32).read_volatile() })
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// cadena terminada en cero, sin el terminador
fn cstr(addr: usize) -> &'static [u8] {
    let mut len = 0;
    while unsafe { ((addr + len) as *const u8).read_volatile() } != 0 {
        len += 1;
    }
    unsafe { core::slice::from_raw_parts(addr as *const u8, len) }
}

/// lee una celda de `cells` palabras de 32 bits (1 o 2)
fn read_cells(addr: usize, cells: usize) -> usize {
    match cells {
        1 => be32(addr) as usize,
        _ => ((be32(addr) as usize) << 32) | be32(addr + 4) as usize,
    }
}

pub fn is_valid(dtb: usize) -> bool {
    dtb != 0 && dtb % 4 == 0 && be32(dtb) == MAGIC
}

/// tamano total del DTB en bytes (para reservarlo en el gestor de memoria)
pub fn total_size(dtb: usize) -> usize {
    be32(dtb + 4) as usize
}

/// region de RAM descrita por el primer nodo `memory`
pub fn ram(dtb: usize) -> Option<Ram> {
    if !is_valid(dtb) {
        return None;
    }
    let strings = dtb + be32(dtb + 12) as usize;
    let mut p = dtb + be32(dtb + 8) as usize;
    // valores por defecto de la especificacion; el nodo raiz los redefine
    let (mut addr_cells, mut size_cells) = (2usize, 1usize);
    let mut depth = 0;
    let mut in_memory = false;

    loop {
        match be32(p) {
            BEGIN_NODE => {
                let name = cstr(p + 4);
                p = align4(p + 4 + name.len() + 1);
                depth += 1;
                if depth == 2 && name.starts_with(b"memory") {
                    in_memory = true;
                }
            }
            END_NODE => {
                p += 4;
                if depth == 2 {
                    in_memory = false;
                }
                depth -= 1;
            }
            PROP => {
                let len = be32(p + 4) as usize;
                let name = cstr(strings + be32(p + 8) as usize);
                let data = p + 12;
                p = align4(data + len);
                if depth == 1 && name == b"#address-cells" {
                    addr_cells = be32(data) as usize;
                } else if depth == 1 && name == b"#size-cells" {
                    size_cells = be32(data) as usize;
                } else if in_memory && name == b"reg" {
                    let base = read_cells(data, addr_cells);
                    let size = read_cells(data + 4 * addr_cells, size_cells);
                    return Some(Ram { base, size });
                }
            }
            NOP => p += 4,
            _ => return None, // FDT_END u otro token: no hay nodo memory
        }
    }
}
