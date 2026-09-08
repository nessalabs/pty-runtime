//! Deterministic per-producer byte ledger with ANSI, wide/combining UTF-8 and queries.
pub const PERIOD: usize = 4096;
const PREFIX: &[u8] =
    b"\x1b[H\x1b[6n\x1b[31mcolor\x1b[0m \xe7\x95\x8c e\xcc\x81 \xf0\x9f\x98\x80\r\n";
pub fn byte(offset: u64, producer: usize) -> u8 {
    let local = offset as usize % PERIOD;
    if local < PREFIX.len() {
        PREFIX[local]
    } else if local % 80 == 79 {
        b'\n'
    } else {
        b'A' + ((local + producer) % 26) as u8
    }
}
pub fn fill(out: &mut [u8], offset: u64, producer: usize) {
    for (index, value) in out.iter_mut().enumerate() {
        *value = byte(offset + index as u64, producer);
    }
}
pub fn queries(bytes: u64) -> u64 {
    // Each full prefix emits its one cursor query, always immediately after HOME.
    (bytes + (PERIOD as u64 - 7)) / PERIOD as u64
}
