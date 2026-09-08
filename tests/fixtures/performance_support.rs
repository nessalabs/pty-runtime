use std::io::{Read, Write};

pub fn producer(length: usize, index: usize) {
    super::raw();
    let mut output = std::io::stdout().lock();
    output.write_all(b"READY").unwrap();
    output.flush().unwrap();
    let mut release = [0];
    std::io::stdin().read_exact(&mut release).unwrap();
    assert_eq!(release, [b'G']);
    let mut chunk = [0; 8192];
    let mut offset = 0;
    while offset < length {
        let count = (length - offset).min(chunk.len());
        for (local, byte) in chunk[..count].iter_mut().enumerate() {
            *byte = if (offset + local) % 80 == 79 {
                b'\n'
            } else {
                b'A' + ((offset + local + index) % 26) as u8
            };
        }
        output.write_all(&chunk[..count]).unwrap();
        offset += count;
    }
    write!(output, "\x1b[2J\x1b[HPERF-END-{index}").unwrap();
    output.flush().unwrap();
    std::io::stdin().read_exact(&mut release).unwrap();
    assert_eq!(release, [b'X']);
}
