//! Unix PTY ownership with dedicated readers and independently supervised helpers.
mod backend;
mod endpoints;
mod guardian;
mod image;
mod image_materialize;
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
fn error(error: std::io::Error) -> ProcessError {
    match error.kind() {
        std::io::ErrorKind::NotFound => ProcessError::NotFound,
        std::io::ErrorKind::PermissionDenied => ProcessError::PermissionDenied,
        _ => ProcessError::Io,
    }
}
#[cfg(test)]
#[path = "../../tests/fixtures/process_spawn_barrier.rs"]
mod spawn_barrier_tests;
