//! Experiment-only OS implementations. No production session API lives here.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("These experiments require Linux or macOS");

pub fn descriptor_count() -> usize {
    let path = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    std::fs::read_dir(path)
        .expect("descriptor inventory")
        .count()
}
