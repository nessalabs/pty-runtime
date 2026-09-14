//! Unix PTY ownership with dedicated readers and independently supervised helpers.
mod backend;
mod endpoints;
mod guardian;
mod image;
mod io;
mod lifecycle;
#[path = "../../../../scripts/guardian/protocol.rs"]
mod protocol;
mod registration;
mod session;
mod signals;
mod spawn;
mod spawner;
mod supervisor;
mod watch;
pub use backend::UnixProcessBackend;
use pty_runtime_domain::process::ProcessError;
/// Map an OS error onto the process boundary's stable vocabulary.
///
/// Running out of descriptors is admission being full, not an unclassified I/O
/// fault, and saying so is the difference between a diagnosable failure and an
/// afternoon. `Io` is the residue: it means "the OS refused and the reason does
/// not correspond to anything this boundary distinguishes". The errno itself
/// stays out of the domain by design — see `ProcessError`'s own note — so the
/// classification has to carry the meaning instead.
fn error(error: std::io::Error) -> ProcessError {
    // Checked before `kind`, because Rust leaves every one of these
    // uncategorised and they would otherwise land in `Io` unexamined.
    if matches!(
        error.raw_os_error(),
        Some(libc::EMFILE | libc::ENFILE | libc::ENOMEM | libc::ENOSPC)
    ) {
        return ProcessError::Capacity;
    }
    match error.kind() {
        std::io::ErrorKind::NotFound => ProcessError::NotFound,
        std::io::ErrorKind::PermissionDenied => ProcessError::PermissionDenied,
        _ => ProcessError::Io,
    }
}

#[cfg(test)]
mod error_tests {
    use super::error;
    use pty_runtime_domain::process::ProcessError;
    use std::io::Error;

    /// The 128-session load case failed five of five on a host whose descriptor
    /// limit was 1024, reporting only `Io`. Naming it cost a full matrix run.
    #[test]
    fn descriptor_and_memory_exhaustion_report_capacity_not_unclassified_io() {
        for code in [libc::EMFILE, libc::ENFILE, libc::ENOMEM, libc::ENOSPC] {
            assert_eq!(
                error(Error::from_raw_os_error(code)),
                ProcessError::Capacity,
                "errno {code} is admission being full"
            );
        }
    }

    #[test]
    fn already_distinguished_failures_keep_their_own_meaning() {
        assert_eq!(
            error(Error::from_raw_os_error(libc::ENOENT)),
            ProcessError::NotFound
        );
        assert_eq!(
            error(Error::from_raw_os_error(libc::EACCES)),
            ProcessError::PermissionDenied
        );
    }

    /// `EAGAIN` is exhaustion from `fork` and "not ready" from a non-blocking
    /// read, and this boundary cannot tell which. It stays unclassified rather
    /// than being guessed at.
    #[test]
    fn ambiguous_and_unknown_failures_remain_io() {
        assert_eq!(
            error(Error::from_raw_os_error(libc::EAGAIN)),
            ProcessError::Io
        );
        assert_eq!(
            error(Error::from_raw_os_error(libc::EPIPE)),
            ProcessError::Io
        );
        assert_eq!(error(Error::other("no errno at all")), ProcessError::Io);
    }
}
#[cfg(test)]
#[path = "../../tests/fixtures/process_spawn_barrier.rs"]
mod spawn_barrier_tests;
