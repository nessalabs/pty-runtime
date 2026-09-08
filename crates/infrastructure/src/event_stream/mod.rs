//! Caller-driven forwarding of bounded raw PTY observations to an external event store.
//! No persistence or background forwarding task is created. The caller chooses the
//! sink, stream incarnation, executor and retention policy. Retrying a publisher's
//! pending event preserves its id and exact payload; a new publisher is a new stream
//! of publication attempts and does not promise deduplication across that boundary.
mod codec;
mod errors;
mod publisher;
mod status;
/// Exact pinned dependency used by the adapter, including its distinct store cursor.
pub use ::event_stream as transport;
pub use codec::{DecodedOutput, decode_record};
pub use errors::{PublicationError, SinkFailure};
pub use publisher::{EventStreamPublisher, Publication};

#[cfg(test)]
mod tests;
