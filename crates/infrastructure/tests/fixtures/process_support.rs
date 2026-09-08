//! Integration fixture construction and shared event collection.
#[path = "process_events.rs"]
mod events;
pub use events::*;
pub fn backend(max: usize) -> pty_runtime_infrastructure::process::UnixProcessBackend {
    pty_runtime_infrastructure::process::UnixProcessBackend::new(vec![std::env::temp_dir()], max)
        .unwrap()
}
