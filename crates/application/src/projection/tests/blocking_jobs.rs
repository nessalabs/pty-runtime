//! What the blocking jobs accept from a provider, and what they refuse.
//!
//! These jobs are the only place an injected store or protector is believed, and
//! the rules they apply were previously buried inside eighty-line orchestration
//! methods with no direct coverage. They need none of the coordinator to run.
//!
//! The distinction that matters most is `Rejected` versus `Uncertain` on commit:
//! the first means nothing reached the store and the disk reservation can be
//! released, the second means ciphertext may exist under a key we cannot
//! confirm, so the reservation stays charged for the runtime's whole life.
//! Confusing them either leaks storage forever or releases a reservation for
//! storage that is still there.
use crate::{
    checkpoint::{ICheckpointProtector, ICheckpointStore},
    projection::{
        ProjectionError,
        blocking::{commit_job, delete_job, read_job},
        state::CommitOutcome,
    },
};
use pty_runtime_domain::{
    ReplayCursor, SessionLifetime,
    checkpoint::{
        CheckpointCapacity, CheckpointError, CheckpointKey, CheckpointRef, ProtectedCheckpoint,
    },
    terminal::{CheckpointDescriptor, CompatibilityId, ControlGeneration, TerminalCheckpoint},
};
use std::sync::Arc;

fn lifetime() -> SessionLifetime {
    SessionLifetime::new(7, 1)
}

fn descriptor() -> CheckpointDescriptor {
    CheckpointDescriptor {
        compatibility: CompatibilityId::new("blocking-job-fixture").unwrap(),
        processed: ReplayCursor {
            lifetime: lifetime(),
            offset: 12,
        },
        control_generation: ControlGeneration::from_raw(3),
    }
}

fn key() -> CheckpointKey {
    CheckpointKey {
        lifetime: lifetime(),
        generation: 5,
    }
}

fn checkpoint() -> TerminalCheckpoint {
    TerminalCheckpoint {
        descriptor: descriptor(),
        bytes: vec![1, 2, 3, 4],
    }
}

/// A store that answers exactly what the test tells it to.
#[derive(Default)]
struct Store {
    read_back: Option<ProtectedCheckpoint>,
    read_error: Option<CheckpointError>,
    commit_ref: Option<CheckpointRef>,
    commit_error: Option<CheckpointError>,
    delete_error: Option<CheckpointError>,
}
impl ICheckpointStore for Store {
    fn commit(&self, _: &ProtectedCheckpoint) -> Result<CheckpointRef, CheckpointError> {
        match (self.commit_error, self.commit_ref) {
            (Some(error), _) => Err(error),
            (None, Some(reference)) => Ok(reference),
            (None, None) => unreachable!("test must supply a commit answer"),
        }
    }
    fn read(&self, _: CheckpointRef, _: usize) -> Result<ProtectedCheckpoint, CheckpointError> {
        match (self.read_error, &self.read_back) {
            (Some(error), _) => Err(error),
            (None, Some(bytes)) => Ok(bytes.clone()),
            (None, None) => unreachable!("test must supply a read answer"),
        }
    }
    fn delete(&self, _: CheckpointRef) -> Result<(), CheckpointError> {
        self.delete_error.map_or(Ok(()), Err)
    }
    fn capacity(&self) -> CheckpointCapacity {
        CheckpointCapacity {
            limit_bytes: 1 << 20,
            committed_bytes: 0,
            abandoned_bytes: 0,
            inflight_bytes: 0,
        }
    }
}

/// A protector that answers exactly what the test tells it to.
#[derive(Default)]
struct Protector {
    protected: Option<ProtectedCheckpoint>,
    protect_error: Option<CheckpointError>,
    opened: Option<TerminalCheckpoint>,
    open_error: Option<CheckpointError>,
}
impl ICheckpointProtector for Protector {
    fn protected_size_limit(&self, plaintext: usize) -> Result<usize, CheckpointError> {
        Ok(plaintext + 64)
    }
    fn protect(
        &self,
        _: CheckpointKey,
        _: TerminalCheckpoint,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        match (self.protect_error, &self.protected) {
            (Some(error), _) => Err(error),
            (None, Some(protected)) => Ok(protected.clone()),
            (None, None) => unreachable!("test must supply a protect answer"),
        }
    }
    fn open(
        &self,
        _: CheckpointKey,
        _: &CheckpointDescriptor,
        _: ProtectedCheckpoint,
    ) -> Result<TerminalCheckpoint, CheckpointError> {
        match (self.open_error, &self.opened) {
            (Some(error), _) => Err(error),
            (None, Some(opened)) => Ok(opened.clone()),
            (None, None) => unreachable!("test must supply an open answer"),
        }
    }
}

fn protected(
    bytes: Vec<u8>,
    with_key: CheckpointKey,
    d: CheckpointDescriptor,
) -> ProtectedCheckpoint {
    ProtectedCheckpoint::new(with_key, d, bytes)
}

fn reference(bytes: usize) -> CheckpointRef {
    CheckpointRef {
        object_id: [1; 16],
        key: key(),
        bytes,
    }
}

// ---- read ---------------------------------------------------------------

#[test]
fn a_read_whose_length_disagrees_with_the_reference_is_unavailable_not_opened() {
    // The store answered, but with a different number of bytes than the
    // reference records. Handing that to the protector would be trusting a
    // provider that has already contradicted itself.
    let store = Store {
        read_back: Some(protected(vec![9; 10], key(), descriptor())),
        ..Store::default()
    };
    let protector = Protector {
        open_error: Some(CheckpointError::Unavailable),
        ..Protector::default()
    };
    let job = read_job(
        Arc::new(store),
        Arc::new(protector),
        reference(11),
        descriptor(),
        4096,
        4096,
    );
    assert!(matches!(
        job(),
        Err(ProjectionError::Storage(CheckpointError::Unavailable))
    ));
}

#[test]
fn an_authenticated_checkpoint_with_a_different_descriptor_is_still_refused() {
    let mut other = descriptor();
    other.control_generation = ControlGeneration::from_raw(99);
    let store = Store {
        read_back: Some(protected(vec![9; 10], key(), descriptor())),
        ..Store::default()
    };
    let protector = Protector {
        opened: Some(TerminalCheckpoint {
            descriptor: other,
            bytes: vec![1],
        }),
        ..Protector::default()
    };
    let job = read_job(
        Arc::new(store),
        Arc::new(protector),
        reference(10),
        descriptor(),
        4096,
        4096,
    );
    assert!(matches!(job(), Err(ProjectionError::InvalidConfiguration)));
}

#[test]
fn plaintext_larger_than_the_configured_cap_is_refused_after_authentication() {
    let store = Store {
        read_back: Some(protected(vec![9; 10], key(), descriptor())),
        ..Store::default()
    };
    let protector = Protector {
        opened: Some(TerminalCheckpoint {
            descriptor: descriptor(),
            // Real bytes, not `with_capacity`: the double is handed out by
            // clone, and cloning a Vec allocates only `len`, so a reserved-but-
            // empty buffer would arrive at the check with capacity zero.
            bytes: vec![0u8; 64],
        }),
        ..Protector::default()
    };
    let job = read_job(
        Arc::new(store),
        Arc::new(protector),
        reference(10),
        descriptor(),
        4096,
        16,
    );
    assert!(matches!(job(), Err(ProjectionError::InvalidConfiguration)));
}

#[test]
fn a_matching_read_is_returned() {
    let store = Store {
        read_back: Some(protected(vec![9; 10], key(), descriptor())),
        ..Store::default()
    };
    let protector = Protector {
        opened: Some(checkpoint()),
        ..Protector::default()
    };
    let job = read_job(
        Arc::new(store),
        Arc::new(protector),
        reference(10),
        descriptor(),
        4096,
        4096,
    );
    assert_eq!(job().unwrap().descriptor, descriptor());
}

// ---- commit -------------------------------------------------------------

#[test]
fn a_protector_failure_is_rejected_because_nothing_reached_the_store() {
    let job = commit_job(
        Arc::new(Store::default()),
        Arc::new(Protector {
            protect_error: Some(CheckpointError::Unavailable),
            ..Protector::default()
        }),
        key(),
        checkpoint(),
        descriptor(),
        4096,
    );
    assert!(matches!(job(), Ok(CommitOutcome::Rejected(_))));
}

#[test]
fn a_store_failure_is_rejected_because_a_failed_commit_stores_nothing() {
    let job = commit_job(
        Arc::new(Store {
            commit_error: Some(CheckpointError::CapacityExceeded),
            ..Store::default()
        }),
        Arc::new(Protector {
            protected: Some(protected(vec![7; 8], key(), descriptor())),
            ..Protector::default()
        }),
        key(),
        checkpoint(),
        descriptor(),
        4096,
    );
    assert!(matches!(job(), Ok(CommitOutcome::Rejected(_))));
}

#[test]
fn protected_output_that_contradicts_the_request_is_rejected_before_committing() {
    // Each of these is the protector disagreeing with what it was asked for, so
    // the commit never happens and no reservation is at risk.
    let wrong_key = CheckpointKey {
        lifetime: lifetime(),
        generation: 99,
    };
    let mut wrong_descriptor = descriptor();
    wrong_descriptor.control_generation = ControlGeneration::from_raw(42);

    for protected_out in [
        protected(vec![7; 8], wrong_key, descriptor()),
        protected(vec![7; 8], key(), wrong_descriptor),
        protected(Vec::new(), key(), descriptor()),
        protected(vec![7; 9000], key(), descriptor()),
    ] {
        let job = commit_job(
            Arc::new(Store::default()),
            Arc::new(Protector {
                protected: Some(protected_out),
                ..Protector::default()
            }),
            key(),
            checkpoint(),
            descriptor(),
            4096,
        );
        assert!(matches!(job(), Ok(CommitOutcome::Rejected(_))));
    }
}

#[test]
fn a_store_that_acknowledges_but_disagrees_is_uncertain_not_rejected() {
    // The store said yes and then described something else. Ciphertext may
    // exist under a key we cannot confirm, so the reservation must stay
    // charged -- calling this Rejected would release it and leak the storage.
    let wrong_key = CheckpointRef {
        object_id: [1; 16],
        key: CheckpointKey {
            lifetime: lifetime(),
            generation: 999,
        },
        bytes: 8,
    };
    let wrong_len = CheckpointRef {
        object_id: [1; 16],
        key: key(),
        bytes: 4096,
    };
    for answer in [wrong_key, wrong_len] {
        let job = commit_job(
            Arc::new(Store {
                commit_ref: Some(answer),
                ..Store::default()
            }),
            Arc::new(Protector {
                protected: Some(protected(vec![7; 8], key(), descriptor())),
                ..Protector::default()
            }),
            key(),
            checkpoint(),
            descriptor(),
            4096,
        );
        assert!(
            matches!(job(), Ok(CommitOutcome::Uncertain(_))),
            "an acknowledged commit that disagrees must stay charged",
        );
    }
}

#[test]
fn a_consistent_commit_is_published() {
    let job = commit_job(
        Arc::new(Store {
            commit_ref: Some(reference(8)),
            ..Store::default()
        }),
        Arc::new(Protector {
            protected: Some(protected(vec![7; 8], key(), descriptor())),
            ..Protector::default()
        }),
        key(),
        checkpoint(),
        descriptor(),
        4096,
    );
    assert!(matches!(job(), Ok(CommitOutcome::Published(_))));
}

// ---- delete -------------------------------------------------------------

#[test]
fn delete_reports_the_provider_failure_for_the_reaper_to_retry() {
    let job = delete_job(
        Arc::new(Store {
            delete_error: Some(CheckpointError::Unavailable),
            ..Store::default()
        }),
        reference(8),
    );
    assert!(matches!(job(), Err(ProjectionError::Storage(_))));

    let ok = delete_job(Arc::new(Store::default()), reference(8));
    assert!(ok().is_ok());
}
