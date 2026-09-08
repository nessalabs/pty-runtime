//! Stable PRNG and synthetic parser corpus: changing these changes replay version.
pub struct Random(pub u64);
impl Random {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub fn bounded(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn feed(&mut self) -> Vec<u8> {
        const CORPUS: &[&[u8]] = &[
            b"ordinary output\r\n",
            b"\x1b[38;2;10;",
            b"20;30mstyled\x1b[0m",
            b"\xe2\x82",
            b"\xac",
            b"\x1b]2;title",
            b"\x07",
            b"\x1b\\",
            b"\x1bP1;2qpayload",
            b"\x1b[?1049hALT",
            b"\x1b[?1049l",
            b"\x1b[?2004h\x1b[?1h",
            b"\x1b[6n",
            b"\x1b[H\x1b[2J",
            "wide 界 e\u{301}\r\n".as_bytes(),
            b"\x1b[2;8r\x1b[4L\x1b[3M",
            b"\x1b]4;1;rgb:12/34/56\x07",
            b"\x1b]8;;https://example.test\x07link\x1b]8;;\x07",
            b"\x18\x1a\x00\x7f\xff\xfe",
            b"\x1b[?7lwrapping\x1b[?7h",
        ];
        if self.bounded(5) == 0 {
            let n = self.bounded(128) + 1;
            (0..n).map(|_| self.next() as u8).collect()
        } else {
            CORPUS[self.bounded(CORPUS.len())].to_vec()
        }
    }
    pub fn mutate(&mut self, bytes: &mut Vec<u8>, kind: usize) {
        match kind % 6 {
            0 => bytes.truncate(self.bounded(bytes.len())),
            1 => {
                let i = self.bounded(bytes.len());
                bytes[i] ^= 1 << self.bounded(8);
            }
            2 => bytes.extend_from_slice(&self.next().to_le_bytes()),
            3 => {
                bytes.clear();
                let n = self.bounded(4096);
                bytes.extend((0..n).map(|_| self.next() as u8));
            }
            4 => {
                let start = self.bounded(bytes.len());
                let n = self.bounded(64).min(bytes.len() - start);
                bytes[start..start + n].fill(0xff);
            }
            _ => {
                let n = self.bounded(bytes.len());
                bytes.drain(..n);
            }
        }
    }
}
