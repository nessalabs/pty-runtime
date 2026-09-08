use pty_runtime_domain::session::RuntimeError;

/// Redacted sink failure category. External diagnostic text and payloads are omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkFailure {
    /// The sink's work or storage limit is full; the publisher retains its pending event.
    Capacity,
    /// The sink may have committed. Retry the same pending identity or reconcile it.
    CommitUnknown,
    /// The supplied identity already names different contents; no cursor advances.
    IdempotencyConflict,
    /// The sink or stream incarnation is no longer available.
    Unavailable,
    /// Another sink operation failed; this category makes no rollback guarantee.
    Other,
}
/// Publication errors never expose bytes, stream names or provider diagnostic strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationError {
    /// Source observation failed before a new pending record was created.
    Source(RuntimeError),
    /// A finite encoding or identity bound was exhausted.
    Capacity,
    /// A record, schema, receipt or portable value violated the wire contract.
    InvalidRecord,
    /// External publication failed. The exact pending event remains retryable.
    Sink(SinkFailure),
}
impl std::fmt::Display for PublicationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PublicationError {}
impl From<RuntimeError> for PublicationError {
    fn from(error: RuntimeError) -> Self {
        Self::Source(error)
    }
}
impl From<::event_stream::Error> for PublicationError {
    fn from(error: ::event_stream::Error) -> Self {
        use ::event_stream::Error;
        Self::Sink(match error {
            Error::Overloaded | Error::CapacityExceeded | Error::AdmissionTimeout => {
                SinkFailure::Capacity
            }
            Error::CommitUnknown { .. } => SinkFailure::CommitUnknown,
            Error::IdempotencyConflict { .. } => SinkFailure::IdempotencyConflict,
            Error::Closed
            | Error::StreamNotFound
            | Error::StaleIncarnation { .. }
            | Error::StreamUnavailable { .. }
            | Error::OwnershipLost => SinkFailure::Unavailable,
            _ => SinkFailure::Other,
        })
    }
}
