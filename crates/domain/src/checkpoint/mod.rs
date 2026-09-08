//! Lifetime-bound protected checkpoint storage values.
mod cleanup;
use crate::{identity::SessionLifetime, terminal::CheckpointDescriptor};
pub use cleanup::{CheckpointCleanupLimits, CheckpointCleanupReport};

/// Immutable logical checkpoint identity; generations must never repeat per lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CheckpointKey {
    /// Owning session lifetime.
    pub lifetime: SessionLifetime,
    /// Monotonically issued parking operation generation.
    pub generation: u64,
}

/// Authenticated encrypted payload. Debug omits metadata and contents.
#[derive(Clone)]
pub struct ProtectedCheckpoint {
    /// Identity authenticated by the protector.
    pub key: CheckpointKey,
    /// State ordering authenticated by the protector.
    pub descriptor: CheckpointDescriptor,
    ciphertext: Vec<u8>,
}
impl ProtectedCheckpoint {
    /// Wrap provider bytes; authentication is mandatory before using terminal state.
    pub fn new(key: CheckpointKey, descriptor: CheckpointDescriptor, ciphertext: Vec<u8>) -> Self {
        Self {
            key,
            descriptor,
            ciphertext,
        }
    }
    /// Consume the protected buffer without copying at the protection boundary.
    pub fn into_ciphertext(self) -> Vec<u8> {
        self.ciphertext
    }
    /// Opaque ciphertext for storage adapters only; never terminal plaintext.
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}
impl std::fmt::Debug for ProtectedCheckpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtectedCheckpoint")
            .field("bytes", &self.ciphertext.len())
            .finish()
    }
}

/// Immutable committed identity with a bounded encoded length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointRef {
    /// Provider-issued unique object identity; prevents released reference reuse.
    pub object_id: [u8; 16],
    /// The exact generation stored by this reference.
    pub key: CheckpointKey,
    /// Protected byte length, including authentication overhead.
    pub bytes: usize,
}

/// Storage occupancy including reservations for incomplete writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointCapacity {
    /// Hard maximum for committed and temporary bytes combined.
    pub limit_bytes: usize,
    /// Bytes currently committed.
    pub committed_bytes: usize,
    /// Conservative charges for failed temporary writes whose cleanup also failed.
    /// These bytes have no readable committed reference and remain quota-charged.
    pub abandoned_bytes: usize,
    /// Reserved bytes currently being written.
    pub inflight_bytes: usize,
}

/// Stable storage/protection failures; no paths, native codes, or contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointError {
    /// Invalid size, identity, metadata, or directory policy.
    InvalidConfiguration,
    /// Total storage or requested read limit was exceeded.
    CapacityExceeded,
    /// The requested immutable identity already exists.
    AlreadyExists,
    /// A reference was released or never committed.
    NotFound,
    /// OS/provider operation failed, including short writes and unavailable media.
    Unavailable,
    /// Ciphertext or expected metadata failed authentication.
    AuthenticationFailed,
    /// Randomness required for cryptographic protection was unavailable.
    EntropyUnavailable,
    /// Work was cancelled before the atomic commit point.
    Cancelled,
}
