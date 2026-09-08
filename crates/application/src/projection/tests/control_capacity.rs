//! ADR 0002/0003: parser overload must not consume bounded resize admission.
use super::{support::*, terminal::Trace};
use crate::{process::OutputAcceptance, projection::*};
use pty_runtime_domain::terminal::TerminalSize;

#[test]
fn resize_remains_ordered_and_bounded_when_local_parser_slots_are_full() {
    exercise(false);
}

#[test]
fn resize_remains_ordered_and_bounded_when_shared_parser_slots_are_full() {
    exercise(true);
}

fn exercise(shared: bool) {
    let mut options = options();
    options.staging_slots = if shared { 64 } else { 2 };
    options.request_slots = if shared { 64 } else { 2 };
    let limits = ProjectionLimits {
        staging_slots: if shared { 2 } else { 64 },
        request_slots: if shared { 2 } else { 64 },
        ..ProjectionLimits::default()
    };
    let h = Harness::new(options, limits);
    let size = TerminalSize::new(3, 1).unwrap();
    assert_eq!(h.owner.stage_output(b"first"), OutputAcceptance::Accepted);
    assert_eq!(h.owner.stage_output(b"second"), OutputAcceptance::Accepted);
    assert_eq!(
        h.owner.stage_output(b"after"),
        OutputAcceptance::Backpressure
    );
    assert_eq!(h.owner.status().published.offset, 11);
    assert_eq!(h.owner.status().processed.offset, 0);

    let mut first = h
        .owner
        .resize(size)
        .expect("output saturation must leave bounded control admission available");
    let mut second = h
        .owner
        .resize(size)
        .expect("second reserved control slot must remain usable");
    assert!(matches!(
        h.owner.resize(size),
        Err(ProjectionError::Capacity)
    ));
    assert_eq!(h.budgets.resources().requests.used, 2);
    assert!(poll(&mut first).is_pending());
    assert!(poll(&mut second).is_pending());

    h.step(); // One earlier output releases a parser slot, not a control request.
    assert_eq!(h.owner.status().processed.offset, 5);
    assert_eq!(h.owner.stage_output(b"after"), OutputAcceptance::Accepted);
    h.pump();
    let first_result = result(&mut first).unwrap();
    let second_result = result(&mut second).unwrap();
    assert!(first_result.os.is_ok() && first_result.model.is_ok());
    assert!(second_result.os.is_ok() && second_result.model.is_ok());
    assert_eq!((first_result.generation, second_result.generation), (1, 2));
    assert_eq!(h.owner.status().published.offset, 16);
    assert_eq!(h.owner.status().processed.offset, 16);
    assert_eq!(h.owner.status().failure, None);
    assert_eq!(
        *h.probe.trace.lock().unwrap(),
        vec![
            Trace::Feed(b"first".to_vec()),
            Trace::Feed(b"second".to_vec()),
            Trace::Resize(1),
            Trace::Resize(2),
            Trace::Feed(b"after".to_vec()),
        ]
    );
    // Completed retained waits still own their finite request reservations.
    assert!(matches!(
        h.owner.resize(size),
        Err(ProjectionError::Capacity)
    ));
    drop((first, second));
    assert_eq!(h.budgets.resources().requests.used, 0);
    let mut third = h.owner.resize(size).unwrap();
    h.pump();
    let outcome = result(&mut third).unwrap();
    assert_eq!(outcome.generation, 3);
    assert!(outcome.os.is_ok() && outcome.model.is_ok());
    drop(third);
    let resources = h.budgets.resources();
    assert_eq!(resources.staging_slots.used, 0);
    assert_eq!(resources.staging_bytes.used, 0);
    assert_eq!(resources.requests.used, 0);
    h.close();
    assert_eq!(h.budgets.resources().native_reservations.used, 0);
    assert_eq!(h.budgets.resources().requests.used, 0);
}
