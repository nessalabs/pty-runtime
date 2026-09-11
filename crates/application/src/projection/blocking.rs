//! What runs on the blocking pool, and what counts as a valid provider answer.
//!
//! These are the only projection operations that may block: reading a committed
//! source back, protecting and committing a new one, and deleting a superseded
//! one. Each is a plain function over injected ports — no workspace, no queue,
//! no coordinator — so the provider contract they enforce can be exercised on
//! its own.
//!
//! Deciding what the provider is allowed to answer is the substance here. A
//! store that returns the wrong number of ciphertext bytes, a protector that
//! hands back a mismatched key, a commit whose reference disagrees with what was
//! written: each has to become a specific outcome, and for commits the
//! difference between "nothing was stored" and "something may have been" decides
//! whether a disk reservation can ever be released.
use super::{ProjectionError, state::CommitOutcome, state::Mailbox};
use crate::{
    checkpoint::{ICheckpointProtector, ICheckpointStore},
    scheduling::{IBlockingExecutor, IWorkHandle},
};
use pty_runtime_domain::{
    checkpoint::{CheckpointError, CheckpointKey, CheckpointRef},
    terminal::{CheckpointDescriptor, TerminalCheckpoint},
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex},
};

/// Hand one job to the blocking pool and return the mailbox it will fill.
///
/// The payload type is the caller's, and it travels with the mailbox into
/// `PendingIo`, so completion cannot misread one job's result as another's.
///
/// A panicking job publishes `Err(Worker)` rather than leaving the mailbox
/// empty: an empty mailbox means "still running", and a crashed worker must not
/// be indistinguishable from a slow one.
pub(super) fn submit<T: Send + 'static>(
    executor: &dyn IBlockingExecutor,
    wake: Option<Arc<dyn IWorkHandle>>,
    work: impl FnOnce() -> Result<T, ProjectionError> + Send + 'static,
) -> Result<Mailbox<T>, ProjectionError> {
    let mailbox: Mailbox<T> = Arc::new(Mutex::new(None));
    let output = mailbox.clone();
    let job = Box::new(move || {
        let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(ProjectionError::Worker));
        *output.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
        if let Some(wake) = wake {
            let _ = wake.wake();
        }
    });
    if executor.submit(job).is_err() {
        return Err(ProjectionError::Capacity);
    }
    Ok(mailbox)
}

/// Read a committed source back and authenticate it.
///
/// The store is not trusted to return what was asked for: a short or long read
/// is reported as unavailable rather than handed to the protector, and a
/// checkpoint whose descriptor or size disagrees with what was requested is
/// refused even though it authenticated.
pub(super) fn read_job(
    store: Arc<dyn ICheckpointStore>,
    protector: Arc<dyn ICheckpointProtector>,
    reference: CheckpointRef,
    descriptor: CheckpointDescriptor,
    protected_max: usize,
    plaintext_max: usize,
) -> impl FnOnce() -> Result<TerminalCheckpoint, ProjectionError> + Send + 'static {
    move || {
        let bytes = store.read(reference, protected_max)?;
        if bytes.ciphertext().len() != reference.bytes {
            return Err(ProjectionError::Storage(CheckpointError::Unavailable));
        }
        let checkpoint = protector.open(reference.key, &descriptor, bytes)?;
        if checkpoint.descriptor != descriptor || checkpoint.bytes.capacity() > plaintext_max {
            return Err(ProjectionError::InvalidConfiguration);
        }
        Ok(checkpoint)
    }
}

/// Protect a checkpoint and commit it.
///
/// Always resolves to a [`CommitOutcome`] rather than an error, because the
/// distinction the caller needs is not success versus failure but whether
/// storage may now hold something:
///
/// - `Rejected` — nothing reached the store, so the reservation can be released.
/// - `Uncertain` — the store acknowledged, but disagreed about what it wrote.
///   Ciphertext may exist under a key we cannot confirm, so the reservation
///   stays charged for the runtime's life.
pub(super) fn commit_job(
    store: Arc<dyn ICheckpointStore>,
    protector: Arc<dyn ICheckpointProtector>,
    key: CheckpointKey,
    checkpoint: TerminalCheckpoint,
    descriptor: CheckpointDescriptor,
    protected_max: usize,
) -> impl FnOnce() -> Result<CommitOutcome, ProjectionError> + Send + 'static {
    move || {
        let protected = match protector.protect(key, checkpoint) {
            Ok(protected) => protected,
            Err(error) => return Ok(CommitOutcome::Rejected(error.into())),
        };
        if protected.key != key
            || protected.descriptor != descriptor
            || protected.ciphertext().is_empty()
            || protected.ciphertext().len() > protected_max
        {
            return Ok(CommitOutcome::Rejected(
                ProjectionError::InvalidConfiguration,
            ));
        }
        let len = protected.ciphertext().len();
        let reference = match store.commit(&protected) {
            Ok(reference) => reference,
            Err(error) => return Ok(CommitOutcome::Rejected(error.into())),
        };
        if reference.key != key || reference.bytes != len {
            return Ok(CommitOutcome::Uncertain(
                ProjectionError::InvalidConfiguration,
            ));
        }
        Ok(CommitOutcome::Published(reference))
    }
}

/// Delete one superseded source. Failure is the reaper's to retry.
pub(super) fn delete_job(
    store: Arc<dyn ICheckpointStore>,
    reference: CheckpointRef,
) -> impl FnOnce() -> Result<(), ProjectionError> + Send + 'static {
    move || store.delete(reference).map_err(ProjectionError::from)
}
