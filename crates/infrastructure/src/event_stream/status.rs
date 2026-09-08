//! Explicit stable wire codes; domain enums have no serialization dependency.
use super::PublicationError;
use pty_runtime_domain::{
    checkpoint::CheckpointError, process::ProcessError, projection::ProjectionError,
    session::RuntimeError, terminal::TerminalError,
};
macro_rules! codes {
    ($encode:ident, $decode:ident, $ty:ident, {$($code:literal => $variant:ident),+ $(,)?}) => {
        pub(super) fn $encode(value: $ty) -> u16 {
            match value { $($ty::$variant => $code),+ }
        }
        pub(super) fn $decode(code: u16) -> Result<$ty, PublicationError> {
            match code { $($code => Ok($ty::$variant)),+, _ => Err(PublicationError::InvalidRecord) }
        }
    }
}
codes!(process_code, process_from, ProcessError, {
    1 => InvalidCommand, 2 => OutsideRoots, 3 => Capacity, 4 => Closed,
    5 => PermissionDenied, 6 => NotFound, 7 => Timeout, 8 => Unsupported,
    9 => Io, 10 => Internal,
});
codes!(terminal_code, terminal_from, TerminalError, {
    1 => InvalidConfiguration, 2 => BudgetExceeded, 3 => EngineFailure,
    4 => CorruptCheckpoint, 5 => IncompatibleCheckpoint, 6 => StaleControl,
    7 => HistoryIncomplete, 8 => Unsupported,
});
codes!(checkpoint_code, checkpoint_from, CheckpointError, {
    1 => InvalidConfiguration, 2 => CapacityExceeded, 3 => AlreadyExists,
    4 => NotFound, 5 => Unavailable, 6 => AuthenticationFailed,
    7 => EntropyUnavailable, 8 => Cancelled,
});
pub(super) fn runtime_code(value: RuntimeError) -> u16 {
    match value {
        RuntimeError::ExistingSession => 1,
        RuntimeError::MissingSession => 2,
        RuntimeError::Capacity => 3,
        RuntimeError::Closed => 4,
        RuntimeError::NotFinished => 5,
        RuntimeError::InvalidCursor => 6,
        RuntimeError::Internal => 7,
        RuntimeError::Process(error) => 100 + process_code(error),
        RuntimeError::Projection(error) => match error {
            ProjectionError::Capacity => 201,
            ProjectionError::InvalidConfiguration => 202,
            ProjectionError::Closed => 203,
            ProjectionError::Worker => 204,
            ProjectionError::Terminal(error) => 300 + terminal_code(error),
            ProjectionError::Storage(error) => 400 + checkpoint_code(error),
            ProjectionError::Process(error) => 500 + process_code(error),
        },
    }
}
pub(super) fn runtime_from(code: u16) -> Result<RuntimeError, PublicationError> {
    Ok(match code {
        1 => RuntimeError::ExistingSession,
        2 => RuntimeError::MissingSession,
        3 => RuntimeError::Capacity,
        4 => RuntimeError::Closed,
        5 => RuntimeError::NotFinished,
        6 => RuntimeError::InvalidCursor,
        7 => RuntimeError::Internal,
        101..=110 => RuntimeError::Process(process_from(code - 100)?),
        201 => RuntimeError::Projection(ProjectionError::Capacity),
        202 => RuntimeError::Projection(ProjectionError::InvalidConfiguration),
        203 => RuntimeError::Projection(ProjectionError::Closed),
        204 => RuntimeError::Projection(ProjectionError::Worker),
        301..=308 => {
            RuntimeError::Projection(ProjectionError::Terminal(terminal_from(code - 300)?))
        }
        401..=408 => {
            RuntimeError::Projection(ProjectionError::Storage(checkpoint_from(code - 400)?))
        }
        501..=510 => RuntimeError::Projection(ProjectionError::Process(process_from(code - 500)?)),
        _ => return Err(PublicationError::InvalidRecord),
    })
}
