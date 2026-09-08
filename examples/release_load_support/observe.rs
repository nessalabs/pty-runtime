use super::{Result, payload};
use pty_runtime::*;
pub struct Observer {
    pub attachment: Attachment,
    pub cursor: u64,
    pub bytes: u64,
    pub gaps: u64,
    pub producer: usize,
}
impl Observer {
    pub fn new(session: &Session, producer: usize) -> Result<Self> {
        Ok(Self {
            attachment: session.attach(AttachPosition::Cursor(ReplayCursor {
                lifetime: session.lifetime(),
                offset: 0,
            }))?,
            cursor: 0,
            bytes: 0,
            gaps: 0,
            producer,
        })
    }
    pub fn poll(&mut self) -> Result<()> {
        for _ in 0..8 {
            match self.attachment.try_next()? {
                None | Some(OutputEvent::Complete(_)) => break,
                Some(OutputEvent::Replay(ReplayPage::Bytes { from, next, bytes })) => {
                    assert_eq!(from.offset, self.cursor);
                    assert_eq!(next.offset - from.offset, bytes.len() as u64);
                    for (index, byte) in bytes.iter().enumerate() {
                        assert_eq!(
                            *byte,
                            payload::byte(from.offset + index as u64, self.producer)
                        );
                    }
                    self.bytes += bytes.len() as u64;
                    self.cursor = next.offset;
                }
                Some(OutputEvent::Replay(ReplayPage::Gap { from, to })) => {
                    assert_eq!(from.offset, self.cursor);
                    assert!(to.offset > from.offset);
                    self.gaps += to.offset - from.offset;
                    self.cursor = to.offset;
                }
                _ => return Err(std::io::Error::other("unexpected replay response").into()),
            }
        }
        Ok(())
    }
}
