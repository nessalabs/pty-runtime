//! Session use cases and bounded observation, independent of OS and engine types.
mod attachment;
mod context;
mod lifecycle;
mod options;
mod owner;
mod projected;
pub(crate) mod quota;
mod repository;
mod session;
mod session_projection;

pub use attachment::{Attachment, NextOutput};
pub use context::SessionContext;
pub use options::{
    AttachPosition, Completion, OutputEvent, RuntimeError, RuntimeOptions, SessionOptions,
    SessionStatus,
};
pub use owner::Runtime;
pub use repository::ISessionRepository;
pub use session::{CompletionWait, Session};

#[cfg(test)]
mod context_tests;
