//! Replaceable synchronous storage boundaries; invoke on bounded workers outside model locks.
use pty_runtime_domain::{
    checkpoint::*,
    terminal::{CheckpointDescriptor, TerminalCheckpoint},
};

/// Owner-lifetime authenticated encryption; no storage provider receives plaintext.
pub trait ICheckpointProtector: Send + Sync {
    /// Consume bounded plaintext and authenticate all identity/order metadata.
    fn protect(
        &self,
        key: CheckpointKey,
        checkpoint: TerminalCheckpoint,
    ) -> Result<ProtectedCheckpoint, CheckpointError>;
    /// Authenticate ciphertext against the caller's trusted expected identity and descriptor.
    /// Reject corruption, truncation, wrong keys, and metadata from another generation.
    fn open(
        &self,
        key: CheckpointKey,
        descriptor: &CheckpointDescriptor,
        checkpoint: ProtectedCheckpoint,
    ) -> Result<TerminalCheckpoint, CheckpointError>;
}

/// Immutable all-or-error checkpoint provider. Operations are synchronous with bounded payloads;
/// callers schedule them outside native ownership on bounded workers. Once invoked,
/// a commit runs to completion: cancelling its wait does not cancel or discard its result.
/// The runtime must delete a committed result when its operation becomes stale.
/// Providers may not evict committed sources before explicit deletion.
pub trait ICheckpointStore: Send + Sync {
    /// Atomically reserve temporary plus committed capacity and publish nonempty opaque bytes.
    /// Empty ciphertext is invalid; no provider-specific envelope format is required.
    /// Failure leaves no readable partial checkpoint. Successful cleanup releases the
    /// reservation; failed cleanup retains an abandoned-byte quota charge until reclaimed.
    fn commit(&self, checkpoint: &ProtectedCheckpoint) -> Result<CheckpointRef, CheckpointError>;
    /// Read exactly the committed object, rejecting lengths above the caller's bound
    /// before allocating. Return failure on a short or oversized provider response.
    fn read(
        &self,
        reference: CheckpointRef,
        max_bytes: usize,
    ) -> Result<ProtectedCheckpoint, CheckpointError>;
    /// Release an exact immutable reference; deletion of an absent object is idempotent.
    fn delete(&self, reference: CheckpointRef) -> Result<(), CheckpointError>;
    /// Snapshot finite accounting, including all in-flight storage reservations.
    /// May wait behind provider I/O; never call while holding native/control ownership.
    fn capacity(&self) -> CheckpointCapacity;
}
