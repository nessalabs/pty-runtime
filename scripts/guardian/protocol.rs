//! Fixed, versioned records shared by the fresh helper and infrastructure owner.
//! Each channel has eight bounded outgoing records and one partial incoming record.
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    os::unix::net::UnixStream,
};

pub const FRAME_BYTES: usize = 32;
const QUEUE_RECORDS: usize = 8;
const VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Kind {
    Hello = 1,
    Started = 2,
    Admitted = 3,
    WorkloadExit = 4,
    StartFailed = 5,
    Fault = 6,
    Retiring = 7,
    Terminate = 10,
    Kill = 11,
    Release = 12,
    Finish = 13,
    Abort = 14,
    Successor = 15,
    Execute = 16,
    SignalApplied = 17,
    Escalated = 18,
}
impl Kind {
    fn decode(value: u16) -> io::Result<Self> {
        Ok(match value {
            1 => Self::Hello,
            2 => Self::Started,
            3 => Self::Admitted,
            4 => Self::WorkloadExit,
            5 => Self::StartFailed,
            6 => Self::Fault,
            7 => Self::Retiring,
            10 => Self::Terminate,
            11 => Self::Kill,
            12 => Self::Release,
            13 => Self::Finish,
            14 => Self::Abort,
            15 => Self::Successor,
            16 => Self::Execute,
            17 => Self::SignalApplied,
            18 => Self::Escalated,
            _ => return Err(invalid()),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: Kind,
    pub generation: u64,
    pub values: [i32; 4],
}
impl Frame {
    pub fn new(kind: Kind, generation: u64, values: [i32; 4]) -> Self {
        Self {
            kind,
            generation,
            values,
        }
    }
    fn encode(self) -> [u8; FRAME_BYTES] {
        let mut bytes = [0; FRAME_BYTES];
        bytes[..4].copy_from_slice(b"PTGR");
        bytes[4..6].copy_from_slice(&VERSION.to_le_bytes());
        bytes[6..8].copy_from_slice(&(self.kind as u16).to_le_bytes());
        bytes[8..16].copy_from_slice(&self.generation.to_le_bytes());
        for (index, value) in self.values.iter().enumerate() {
            bytes[16 + index * 4..20 + index * 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    fn decode(bytes: [u8; FRAME_BYTES], generation: u64) -> io::Result<Self> {
        if bytes[..4] != *b"PTGR"
            || bytes[4..6] != VERSION.to_le_bytes()
            || bytes[8..16] != generation.to_le_bytes()
        {
            return Err(invalid());
        }
        let kind = Kind::decode(u16::from_le_bytes([bytes[6], bytes[7]]))?;
        let mut values = [0; 4];
        for (index, value) in values.iter_mut().enumerate() {
            let start = 16 + index * 4;
            *value = i32::from_le_bytes([
                bytes[start],
                bytes[start + 1],
                bytes[start + 2],
                bytes[start + 3],
            ]);
        }
        Ok(Self::new(kind, generation, values))
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "guardian protocol")
}

pub struct Channel {
    stream: UnixStream,
    generation: u64,
    incoming: [u8; FRAME_BYTES],
    received: usize,
    outgoing: VecDeque<[u8; FRAME_BYTES]>,
    sent: usize,
    eof: bool,
}
impl Channel {
    pub fn new(stream: UnixStream, generation: u64) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self {
            stream,
            generation,
            incoming: [0; FRAME_BYTES],
            received: 0,
            outgoing: VecDeque::with_capacity(QUEUE_RECORDS),
            sent: 0,
            eof: false,
        })
    }
    pub fn fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd;
        self.stream.as_raw_fd()
    }
    pub fn wants_write(&self) -> bool {
        !self.outgoing.is_empty()
    }
    pub fn eof(&self) -> bool {
        self.eof
    }
    pub fn enqueue(&mut self, frame: Frame) -> io::Result<()> {
        if frame.generation != self.generation {
            return Err(invalid());
        }
        let bytes = frame.encode();
        if self.outgoing.iter().any(|pending| *pending == bytes) {
            return Ok(());
        }
        if self.outgoing.len() == QUEUE_RECORDS {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.outgoing.push_back(bytes);
        Ok(())
    }
    pub fn flush(&mut self) -> io::Result<()> {
        for _ in 0..QUEUE_RECORDS {
            let Some(front) = self.outgoing.front() else {
                break;
            };
            match self.stream.write(&front[self.sent..]) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    self.sent += count;
                    if self.sent == FRAME_BYTES {
                        self.outgoing.pop_front();
                        self.sent = 0;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    pub fn receive(&mut self) -> io::Result<Option<Frame>> {
        if self.eof {
            return Ok(None);
        }
        match self.stream.read(&mut self.incoming[self.received..]) {
            Ok(0) => return self.finish_receive(),
            Ok(count) => {
                self.received += count;
                if self.received == FRAME_BYTES {
                    self.received = 0;
                    return Frame::decode(self.incoming, self.generation).map(Some);
                }
            }
            // Unix peers can close with a late unread control still queued.
            // Linux then reports reset after buffered status bytes, while Darwin
            // reports EOF. Retirement authorization remains the caller's policy;
            // neither outcome may hide a truncated incoming frame.
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {
                return self.finish_receive();
            }
            Err(error)
                if error.kind() == io::ErrorKind::Interrupted
                    || error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        Ok(None)
    }
    fn finish_receive(&mut self) -> io::Result<Option<Frame>> {
        self.eof = true;
        if self.received == 0 {
            Ok(None)
        } else {
            Err(invalid())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant},
    };

    fn await_eof(channel: &mut Channel) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert_eq!(channel.receive()?, None, "unexpected trailing frame");
            if channel.eof() {
                return Ok(());
            }
            assert!(
                Instant::now() < deadline,
                "peer close did not become observable"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn fragments_reassemble_without_interpreting_partial_records() {
        let (mut writer, reader) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(reader, 7).unwrap();
        let expected = Frame::new(Kind::Started, 7, [1, 2, 3, 4]);
        let bytes = expected.encode();
        writer.write_all(&bytes[..3]).unwrap();
        assert!(channel.receive().unwrap().is_none());
        writer.write_all(&bytes[3..]).unwrap();
        assert_eq!(channel.receive().unwrap(), Some(expected));
    }
    #[test]
    fn stale_generation_and_partial_eof_are_rejected() {
        let bytes = Frame::new(Kind::Kill, 6, [0; 4]).encode();
        assert!(Frame::decode(bytes, 7).is_err());
        let (mut writer, reader) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(reader, 7).unwrap();
        writer.write_all(&bytes[..3]).unwrap();
        drop(writer);
        assert!(channel.receive().unwrap().is_none());
        assert_eq!(
            await_eof(&mut channel).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn status_backpressure_has_a_fixed_record_bound() {
        let (_reader, writer) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(writer, 7).unwrap();
        for index in 0..QUEUE_RECORDS {
            channel
                .enqueue(Frame::new(Kind::Fault, 7, [index as i32; 4]))
                .unwrap();
        }
        assert_eq!(
            channel
                .enqueue(Frame::new(Kind::Fault, 7, [99; 4]))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn peer_close_after_final_record_preserves_eof_with_unread_control() {
        let (parent, mut peer) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(parent, 7).unwrap();
        // A late host control is unread when the helper sends its final status
        // and closes. Linux reports reset after buffered bytes; Darwin reports EOF.
        channel
            .enqueue(Frame::new(Kind::Release, 7, [0; 4]))
            .unwrap();
        channel.flush().unwrap();
        let retired = Frame::new(Kind::Retiring, 7, [123, 0, 0, 0]);
        peer.write_all(&retired.encode()).unwrap();
        drop(peer);
        assert_eq!(channel.receive().unwrap(), Some(retired));
        await_eof(&mut channel).unwrap();
        assert!(channel.eof());
    }
    #[test]
    fn peer_reset_cannot_turn_a_partial_final_record_into_clean_eof() {
        let (parent, mut peer) = UnixStream::pair().unwrap();
        let mut channel = Channel::new(parent, 7).unwrap();
        channel
            .enqueue(Frame::new(Kind::Release, 7, [0; 4]))
            .unwrap();
        channel.flush().unwrap();
        let retired = Frame::new(Kind::Retiring, 7, [123, 0, 0, 0]);
        peer.write_all(&retired.encode()[..3]).unwrap();
        // Model a concurrent fork retaining this endpoint until it execs/exits.
        // Dropping one descriptor must not be mistaken for observable EOF.
        let retained_peer = peer.try_clone().unwrap();
        drop(peer);
        assert!(channel.receive().unwrap().is_none());
        assert!(channel.receive().unwrap().is_none());
        assert!(!channel.eof());
        // Do not shutdown or drain the peer: unread Release must still exercise
        // reset on Linux and EOF on Darwin when its final descriptor closes.
        drop(retained_peer);
        assert_eq!(
            await_eof(&mut channel).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(channel.eof());
    }
}
