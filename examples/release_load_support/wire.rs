//! Fixed, bounded fixture control records; never mixed into the PTY byte stream.
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
};
pub const BYTES: usize = 64;
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub kind: u8,
    pub values: [u64; 7],
}
impl Frame {
    pub fn new(kind: u8, values: &[u64]) -> Self {
        assert!(values.len() <= 7);
        let mut frame = Self {
            kind,
            values: [0; 7],
        };
        frame.values[..values.len()].copy_from_slice(values);
        frame
    }
    pub fn write(self, stream: &mut UnixStream) -> io::Result<()> {
        let mut bytes = [0; BYTES];
        bytes[0] = self.kind;
        for (index, value) in self.values.iter().enumerate() {
            bytes[8 + index * 8..16 + index * 8].copy_from_slice(&value.to_le_bytes());
        }
        stream.write_all(&bytes)
    }
    pub fn read(stream: &mut UnixStream) -> io::Result<Self> {
        let mut bytes = [0; BYTES];
        stream.read_exact(&mut bytes)?;
        Self::decode(bytes)
    }
    fn decode(bytes: [u8; BYTES]) -> io::Result<Self> {
        if bytes[1..8] != [0; 7] {
            return Err(io::Error::other("invalid fixture record"));
        }
        let mut values = [0; 7];
        for (index, value) in values.iter_mut().enumerate() {
            *value = u64::from_le_bytes(bytes[8 + index * 8..16 + index * 8].try_into().unwrap());
        }
        Ok(Self {
            kind: bytes[0],
            values,
        })
    }
}
pub struct Reader {
    bytes: [u8; BYTES],
    used: usize,
}
impl Default for Reader {
    fn default() -> Self {
        Self {
            bytes: [0; BYTES],
            used: 0,
        }
    }
}
impl Reader {
    pub fn next(&mut self, stream: &mut UnixStream) -> io::Result<Option<Frame>> {
        match stream.read(&mut self.bytes[self.used..]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => self.used += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
        if self.used != BYTES {
            return Ok(None);
        }
        self.used = 0;
        Frame::decode(self.bytes).map(Some)
    }
}
