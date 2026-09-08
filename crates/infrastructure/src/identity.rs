//! Process-lifetime owner identities without exposing OS randomness to core.
use pty_runtime_domain::process::ProcessError;
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};
static OWNERS: OnceLock<Result<AtomicU64, ProcessError>> = OnceLock::new();

/// Issue a never-reused owner identity within this host process.
/// A randomized initial value also distinguishes restarted owners probabilistically.
pub fn next_owner_identity() -> Result<u64, ProcessError> {
    let counter = OWNERS
        .get_or_init(|| {
            use std::io::Read;
            let mut bytes = [0; 8];
            std::fs::File::open("/dev/urandom")
                .and_then(|mut file| file.read_exact(&mut bytes))
                .map_err(|_| ProcessError::Io)?;
            Ok(AtomicU64::new(u64::from_ne_bytes(bytes) >> 1))
        })
        .as_ref()
        .map_err(|error| *error)?;
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            old.checked_add(1)
        })
        .map_err(|_| ProcessError::Capacity)
}
