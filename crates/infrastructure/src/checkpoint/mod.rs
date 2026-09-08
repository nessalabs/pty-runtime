//! Private disk storage and owner-lifetime authenticated encryption adapters.
mod arena;
mod cleanup;
mod file;
mod filesystem;
mod inventory;
mod protector;
pub use file::FileCheckpointStore;
pub use protector::CheckpointProtector;
