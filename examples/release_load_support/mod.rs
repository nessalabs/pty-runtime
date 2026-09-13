pub mod allocator;
pub mod config;
pub mod fixture;
pub mod observe;
pub mod payload;
pub mod population;
pub mod report;
pub mod run;
pub mod wire;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
/// Name a failing path syscall and its operand.
///
/// `Error: Os { code: 2, kind: NotFound }` out of `main` identifies neither the
/// call nor the file it was given, and this fixture binds, connects to and
/// unlinks socket names repeatedly within a trial. Diagnosis needs the site.
pub fn at<T>(site: &str, path: &std::path::Path, result: std::io::Result<T>) -> std::io::Result<T> {
    result.map_err(|error| std::io::Error::other(format!("{site} {}: {error}", path.display())))
}

pub const DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
pub mod cancel;
pub mod phase;
pub mod producer;
pub mod resize;
pub mod resources;
pub mod sink;
