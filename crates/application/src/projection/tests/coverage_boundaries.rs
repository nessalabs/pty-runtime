//! Independent observer, wake and byte-admission boundary regressions.
use super::support::*;
use crate::{process::OutputAcceptance, projection::ProjectionError};
use pty_runtime_domain::process::DrainOutcome;
use std::{
    sync::{Arc, atomic::Ordering},
    task::Poll,
};

#[test]
fn oversized_or_late_output_is_refused_without_advancing_or_reserving_bytes() {
    let h = Harness::standard();
    assert_eq!(
        h.owner
            .stage_output(&vec![0; options().terminal.feed_bytes + 1]),
        OutputAcceptance::Backpressure
    );
    assert_eq!(h.owner.status().published.offset, 0);
    assert_eq!(h.budgets.resources().staging_bytes.used, 0);
    h.owner.notify_output_drained(DrainOutcome::Eof);
    assert_eq!(h.owner.stage_output(b"late"), OutputAcceptance::Closed);
    let mut closed = h.owner.close().unwrap();
    assert_eq!(h.owner.stage_output(b"closing"), OutputAcceptance::Closed);
    h.pump();
    result(&mut closed).unwrap();
    assert_eq!(h.owner.stage_output(b"closed"), OutputAcceptance::Closed);
    drop(closed);
    assert_budgets_released(&h);
}

#[test]
fn wake_failure_after_output_exit_or_drain_reports_projection_failure_and_cleans() {
    for event in ["output", "exit", "drain"] {
        let h = Harness::standard();
        h.scheduler.reject_wake.store(true, Ordering::Release);
        match event {
            "output" => assert_eq!(
                h.owner.stage_output(b"accepted"),
                OutputAcceptance::Accepted
            ),
            "exit" => h.owner.notify_process_exited(),
            "drain" => h.owner.notify_output_drained(DrainOutcome::Eof),
            _ => unreachable!(),
        }
        assert_eq!(
            h.owner.status().failure,
            Some(ProjectionError::Worker),
            "{event}"
        );
        assert_eq!(h.owner.status().processed.offset, 0);
        if event == "output" {
            assert_eq!(h.owner.status().published.offset, 8);
            assert_eq!(h.budgets.resources().staging_bytes.used, 8);
        }
        h.scheduler.reject_wake.store(false, Ordering::Release);
        h.close();
        assert_budgets_released(&h);
    }
}

#[test]
fn duplicate_process_binding_preserves_original_writer_and_closed_binding_refuses() {
    let h = Harness::standard();
    let replacement = Arc::new(Process::default());
    assert_eq!(
        h.owner.bind_process(replacement.clone()),
        Err(ProjectionError::InvalidConfiguration)
    );
    assert_eq!(h.owner.stage_output(b"?"), OutputAcceptance::Accepted);
    h.pump();
    assert_eq!(*h.process.writes.lock().unwrap(), [b"R".to_vec()]);
    assert!(replacement.writes.lock().unwrap().is_empty());
    h.close();
    assert_eq!(
        h.owner.bind_process(replacement),
        Err(ProjectionError::Closed)
    );
    assert_budgets_released(&h);
}

#[test]
fn invalid_checkpoint_descriptors_and_capacities_are_never_published_to_observers() {
    for wrong_descriptor in [false, true] {
        let h = Harness::standard();
        h.probe
            .wrong_checkpoint_descriptor
            .store(wrong_descriptor, Ordering::Release);
        h.probe.checkpoint_capacity.store(
            if wrong_descriptor {
                0
            } else {
                options().terminal.checkpoint_bytes + 1
            },
            Ordering::Release,
        );
        let expected = if wrong_descriptor {
            ProjectionError::InvalidConfiguration
        } else {
            ProjectionError::Capacity
        };
        let mut checkpoint = h.owner.checkpoint().unwrap();
        let mut transfer = h.owner.begin_transfer().unwrap();
        h.pump();
        assert!(matches!(result(&mut checkpoint), Err(error) if error == expected));
        assert!(matches!(result(&mut transfer), Err(error) if error == expected));
        drop((checkpoint, transfer));
        assert_eq!(h.budgets.resources().checkpoint_buffers.used, 0);
        assert_eq!(h.budgets.resources().transfer_observers.used, 0);
        assert_eq!(
            h.owner.status().failure,
            None,
            "observation failure does not invalidate the live model"
        );
        h.probe
            .wrong_checkpoint_descriptor
            .store(false, Ordering::Release);
        h.probe.checkpoint_capacity.store(0, Ordering::Release);
        let mut valid = h.owner.begin_transfer().unwrap();
        h.pump();
        let valid_result = result(&mut valid).unwrap();
        assert_eq!(
            valid_result.checkpoint().checkpoint().descriptor.processed,
            valid_result.boundary().processed
        );
        drop((valid, valid_result));
        h.close();
        assert_budgets_released(&h);
    }
}

#[test]
fn a_completed_projection_operation_cannot_deliver_a_second_owned_result() {
    let h = Harness::standard();
    let mut operation = h.owner.view().unwrap();
    h.pump();
    let owned = result(&mut operation).unwrap();
    assert!(matches!(
        poll(&mut operation),
        Poll::Ready(Err(ProjectionError::Closed))
    ));
    drop(operation);
    assert!(
        h.budgets.resources().views.used > 0,
        "the first result still owns its memory"
    );
    drop(owned);
    h.close();
    assert_budgets_released(&h);
}
