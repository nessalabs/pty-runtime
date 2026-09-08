use super::{PublicationError, status};
use ::event_stream::{Payload, Record, SchemaId, SchemaRef};
use pty_runtime_domain::{
    ReplayCursor, ReplayPage, SessionLifetime,
    process::{DrainOutcome, ExitStatus},
    session::{Completion, OutputEvent, SessionStatus},
};
const SCHEMA: &str = "nessalabs.pty-runtime.output";
const VERSION: u32 = 1;
const MAGIC: &[u8; 4] = b"PTYR";
pub(super) const MAX_PAGE: usize = 65536;
const MAX_ENCODED: usize = MAX_PAGE + 64;

/// A decoded typed observation and its independent PTY byte position.
/// The enclosing event record's store cursor is never converted into this cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedOutput {
    /// End of the represented output or gap; unchanged for completion.
    pub byte_cursor: ReplayCursor,
    /// Original portable PTY output, loss or completion facts.
    pub output: OutputEvent,
}
/// Decode only this adapter's versioned schema. Reject oversized, truncated,
/// trailing, contradictory or unknown values before returning a typed observation.
pub fn decode_record(record: &Record) -> Result<DecodedOutput, PublicationError> {
    if record.event.schema.id.as_str() != SCHEMA
        || record.event.schema.version != VERSION
        || record.cursor.version != ::event_stream::CURSOR_VERSION
        || record.cursor.offset == 0
    {
        return Err(PublicationError::InvalidRecord);
    }
    let bytes = record.event.payload.as_bytes();
    if bytes.len() > MAX_ENCODED {
        return Err(PublicationError::InvalidRecord);
    }
    let mut input = Input(bytes);
    if input.take(4)? != MAGIC {
        return Err(PublicationError::InvalidRecord);
    }
    let tag = input.byte()?;
    let lifetime = SessionLifetime::new(input.u64()?, input.u64()?);
    let from = ReplayCursor {
        lifetime,
        offset: input.u64()?,
    };
    let next = ReplayCursor {
        lifetime,
        offset: input.u64()?,
    };
    let output = match tag {
        1 => {
            if input.0.is_empty()
                || input.0.len() > MAX_PAGE
                || next.offset.checked_sub(from.offset) != Some(input.0.len() as u64)
            {
                return Err(PublicationError::InvalidRecord);
            }
            let bytes = input.take(input.0.len())?.to_vec();
            OutputEvent::Replay(ReplayPage::Bytes { from, next, bytes })
        }
        2 if next.offset > from.offset => OutputEvent::Replay(ReplayPage::Gap { from, to: next }),
        3 if next == from => OutputEvent::Complete(decode_completion(&mut input)?),
        _ => return Err(PublicationError::InvalidRecord),
    };
    if !input.0.is_empty() {
        return Err(PublicationError::InvalidRecord);
    }
    Ok(DecodedOutput {
        byte_cursor: next,
        output,
    })
}
pub(super) fn schema() -> Result<SchemaRef, PublicationError> {
    Ok(SchemaRef {
        id: SchemaId::new(SCHEMA).map_err(|_| PublicationError::InvalidRecord)?,
        version: VERSION,
    })
}
pub(super) fn encode(
    from: ReplayCursor,
    event: &OutputEvent,
) -> Result<(Payload, ReplayCursor, bool), PublicationError> {
    let (tag, next, body) = match event {
        OutputEvent::Replay(ReplayPage::Bytes {
            from: start,
            next,
            bytes,
        }) if *start == from
            && next.lifetime == from.lifetime
            && !bytes.is_empty()
            && bytes.len() <= MAX_PAGE
            && next.offset.checked_sub(from.offset) == Some(bytes.len() as u64) =>
        {
            (1, *next, bytes.len())
        }
        OutputEvent::Replay(ReplayPage::Gap { from: start, to })
            if *start == from && to.lifetime == from.lifetime && to.offset > from.offset =>
        {
            (2, *to, 0)
        }
        OutputEvent::Complete(completion) if completion.status.completion().is_some() => {
            (3, from, 16)
        }
        _ => return Err(PublicationError::InvalidRecord),
    };
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(37 + body)
        .map_err(|_| PublicationError::Capacity)?;
    encoded.extend_from_slice(MAGIC);
    encoded.push(tag);
    for value in [
        from.lifetime.owner(),
        from.lifetime.sequence(),
        from.offset,
        next.offset,
    ] {
        encoded.extend_from_slice(&value.to_le_bytes());
    }
    match event {
        OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => encoded.extend_from_slice(bytes),
        OutputEvent::Complete(completion) => encode_completion(&mut encoded, *completion),
        _ => (),
    }
    Ok((Payload::copy_from_slice(&encoded), next, tag == 3))
}
fn encode_completion(bytes: &mut Vec<u8>, completion: Completion) {
    let status = completion.status;
    let (tag, code) = match status.exit {
        None => (0, 0),
        Some(ExitStatus::Code(code)) => (1, code),
        Some(ExitStatus::Signal(code)) => (2, code),
    };
    bytes.push(tag);
    bytes.extend_from_slice(&code.to_le_bytes());
    let (tag, code) = match status.drain {
        None => (0, 0),
        Some(DrainOutcome::Eof) => (1, 0),
        Some(DrainOutcome::Truncated) => (2, 0),
        Some(DrainOutcome::Failed(error)) => (3, status::process_code(error)),
    };
    bytes.push(tag);
    bytes.extend_from_slice(&code.to_le_bytes());
    bytes.extend_from_slice(
        &status
            .supervision_error
            .map_or(0, status::process_code)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(
        &status
            .admission_error
            .map_or(0, status::runtime_code)
            .to_le_bytes(),
    );
    bytes.push(u8::from(status.cancellation_requested));
}
fn decode_completion(input: &mut Input<'_>) -> Result<Completion, PublicationError> {
    let exit_tag = input.byte()?;
    let exit_code = i32::from_le_bytes(
        input
            .take(4)?
            .try_into()
            .map_err(|_| PublicationError::InvalidRecord)?,
    );
    let exit = match exit_tag {
        0 if exit_code == 0 => None,
        1 => Some(ExitStatus::Code(exit_code)),
        2 => Some(ExitStatus::Signal(exit_code)),
        _ => return Err(PublicationError::InvalidRecord),
    };
    let drain_tag = input.byte()?;
    let drain_code = input.u16()?;
    let drain = match (drain_tag, drain_code) {
        (0, 0) => None,
        (1, 0) => Some(DrainOutcome::Eof),
        (2, 0) => Some(DrainOutcome::Truncated),
        (3, code) => Some(DrainOutcome::Failed(status::process_from(code)?)),
        _ => return Err(PublicationError::InvalidRecord),
    };
    let supervision = input.u16()?;
    let admission = input.u16()?;
    let cancellation_requested = match input.byte()? {
        0 => false,
        1 => true,
        _ => return Err(PublicationError::InvalidRecord),
    };
    let status = SessionStatus {
        exit,
        drain,
        cancellation_requested,
        supervision_error: if supervision == 0 {
            None
        } else {
            Some(status::process_from(supervision)?)
        },
        admission_error: if admission == 0 {
            None
        } else {
            Some(status::runtime_from(admission)?)
        },
    };
    status.completion().ok_or(PublicationError::InvalidRecord)
}
struct Input<'a>(&'a [u8]);
impl<'a> Input<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], PublicationError> {
        if self.0.len() < count {
            return Err(PublicationError::InvalidRecord);
        }
        let (value, rest) = self.0.split_at(count);
        self.0 = rest;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8, PublicationError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, PublicationError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| PublicationError::InvalidRecord)?,
        ))
    }
    fn u64(&mut self) -> Result<u64, PublicationError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| PublicationError::InvalidRecord)?,
        ))
    }
}
