use super::{TransferCursor, TransferError, TransferOrder};
use crate::terminal::ControlGeneration;
use crate::{ReplayCursor, SessionLifetime};
#[test]
fn applied_controls_advance_sequence_without_advancing_original_bytes() {
    let lifetime = SessionLifetime::new(4, 2);
    let mut order = TransferOrder::new(lifetime);
    let output = order
        .output(ReplayCursor {
            lifetime,
            offset: 7,
        })
        .unwrap();
    let resize = order.resized(ControlGeneration::from_raw(1)).unwrap();
    let second = order.resized(ControlGeneration::from_raw(2)).unwrap();
    assert_eq!(output.processed, second.processed);
    assert_eq!(resize.cursor.sequence, output.cursor.sequence + 1);
    assert_eq!(second.cursor.sequence, resize.cursor.sequence + 1);
    assert_eq!(second.control_generation, ControlGeneration::from_raw(2));
    assert!(
        order
            .output(ReplayCursor {
                lifetime,
                offset: 7
            })
            .is_err()
    );
    assert!(order.resized(ControlGeneration::from_raw(4)).is_err());
    assert_eq!(order.boundary(), second);
}
#[test]
fn identity_future_and_evicted_ranges_have_distinct_failures() {
    let lifetime = SessionLifetime::new(4, 2);
    let mut order = TransferOrder::new(lifetime);
    let original = order.boundary().cursor;
    let next = order
        .resized(ControlGeneration::from_raw(1))
        .unwrap()
        .cursor;
    order.discard_before(next).unwrap();
    assert_eq!(
        order.validate(original),
        Err(TransferError::ResyncRequired { oldest: next })
    );
    assert_eq!(
        order.validate(TransferCursor {
            lifetime: SessionLifetime::new(4, 3),
            ..original
        }),
        Err(TransferError::ForeignLifetime)
    );
    assert_eq!(
        order.validate(TransferCursor {
            sequence: 2,
            ..original
        }),
        Err(TransferError::FutureCursor)
    );
    assert!(
        order
            .discard_before(TransferCursor {
                sequence: 2,
                ..original
            })
            .is_err()
    );
    assert!(order.validate(next).is_ok());
}

#[test]
fn terminal_prefix_is_immutable_and_domain_forbids_late_mutations() {
    use crate::{process::DrainOutcome, projection::ProjectionError};
    let lifetime = SessionLifetime::new(4, 2);
    let mut order = TransferOrder::new(lifetime);
    assert!(order.seal(None, None).is_err());
    assert!(order.end().is_none());
    order.seal(None, Some(ProjectionError::Worker)).unwrap();
    let sealed = order.end().unwrap();
    assert_eq!(
        order.output(ReplayCursor {
            lifetime,
            offset: 1
        }),
        Err(TransferError::Closed)
    );
    assert_eq!(
        order.resized(ControlGeneration::from_raw(1)),
        Err(TransferError::Closed)
    );
    order.seal(Some(DrainOutcome::Eof), None).unwrap();
    assert_eq!(order.end(), Some(sealed));
    assert_eq!(order.boundary(), sealed.boundary);
    assert!(!order.accepts_events());
    order.close();
    assert_eq!(
        order.validate(sealed.boundary.cursor),
        Err(TransferError::Closed)
    );
    assert_eq!(order.open_boundary(), Err(TransferError::Closed));
    assert_eq!(
        order.seal(Some(DrainOutcome::Eof), None),
        Err(TransferError::Closed)
    );
}
#[test]
fn unavailable_transfer_cannot_be_reopened_or_extended() {
    let lifetime = SessionLifetime::new(4, 2);
    let mut order = TransferOrder::new(lifetime);
    order.invalidate();
    assert!(!order.accepts_events());
    assert_eq!(order.open_boundary(), Err(TransferError::Unavailable));
    assert_eq!(
        order.resized(ControlGeneration::from_raw(1)),
        Err(TransferError::Unavailable)
    );
}
