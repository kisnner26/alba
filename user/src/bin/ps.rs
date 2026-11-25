#![no_std]
#![no_main]

#[no_mangle]
pub extern "C" fn _start() -> ! {
    alba::println!("pid  estado  cpu(us)  bloqueos  despertares  nombre");
    for index in 0.. {
        let mut record = [0; 56];
        let result = alba::syscall(24, index, record.as_mut_ptr() as u64, 56, 0);
        if result == 0 {
            alba::exit(0);
        }
        if result != 56 {
            alba::exit(1);
        }
        let id = u64::from_le_bytes(record[..8].try_into().unwrap());
        let cpu = u64::from_le_bytes(record[8..16].try_into().unwrap());
        let stopped = u32::from_le_bytes(record[16..20].try_into().unwrap());
        let wakes = u32::from_le_bytes(record[20..24].try_into().unwrap());
        let state = match record[24] {
            0 => "listo",
            1 => "corriendo",
            2 => "dormido",
            3 => "bloqueado",
            _ => "?",
        };
        let bytes = &record[32..];
        let n = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
        let name = core::str::from_utf8(&bytes[..n]).unwrap_or("?");
        alba::println!("{}  {}  {}  {}  {}  {}", id, state, cpu, stopped, wakes, name);
    }
    unreachable!()
}
