//! Private disk storage and owner-lifetime authenticated encryption adapters.
mod file;
mod filesystem;
mod protector;
pub use file::FileCheckpointStore;
pub use protector::CheckpointProtector;
