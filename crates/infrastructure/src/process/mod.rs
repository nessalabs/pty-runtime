//! Unix PTY ownership with bounded dedicated readers and shared supervision.
mod backend;
mod endpoints;
mod io;
mod lifecycle;
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
