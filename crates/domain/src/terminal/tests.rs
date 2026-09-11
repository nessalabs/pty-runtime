//! Engine-neutral terminal value bounds.
use super::{CompatibilityId, ControlGeneration, TerminalError};

#[test]
fn control_generation_advances_by_exactly_one_and_reports_exhaustion() {
    let start = ControlGeneration::INITIAL;
    assert_eq!(start.get(), 0);

    let first = start.next().expect("the first control always exists");
    assert_eq!(first.get(), 1);
    assert_eq!(first.next().map(ControlGeneration::get), Some(2));

    // Ordering is meaningful, so a stale control can be recognised as stale.
    assert!(first > start);

    // Exhaustion is reported rather than wrapping back to a generation that has
    // already been applied, which would silently accept a stale resize.
    let last = ControlGeneration::from_raw(u64::MAX);
    assert_eq!(last.next(), None);
}

#[test]
fn compatibility_identity_rejects_empty_and_oversized_values_and_redacts_debug() {
    assert_eq!(
        CompatibilityId::new("").unwrap_err(),
        TerminalError::InvalidConfiguration
    );

    let largest = "x".repeat(CompatibilityId::MAX_BYTES);
    let accepted = CompatibilityId::new(&largest).expect("the exact bound is accepted");
    assert_eq!(accepted.as_str(), largest);

    let oversized = "x".repeat(CompatibilityId::MAX_BYTES + 1);
    assert_eq!(
        CompatibilityId::new(&oversized).unwrap_err(),
        TerminalError::InvalidConfiguration
    );

    // Multi-byte content is bounded by bytes, not characters, so an identity
    // that fits by character count can still exceed the allocation bound.
    let multibyte = "é".repeat(CompatibilityId::MAX_BYTES);
    assert!(multibyte.chars().count() <= CompatibilityId::MAX_BYTES);
    assert_eq!(
        CompatibilityId::new(&multibyte).unwrap_err(),
        TerminalError::InvalidConfiguration
    );

    // The identity is adapter metadata; ordinary diagnostics expose only its size.
    let identity = CompatibilityId::new("private-engine-build-marker").unwrap();
    let rendered = format!("{identity:?}");
    assert!(!rendered.contains("private-engine-build-marker"));
    assert!(rendered.contains(&"private-engine-build-marker".len().to_string()));
}

#[test]
fn compatibility_identity_equality_is_literal() {
    let a = CompatibilityId::new("engine-v1").unwrap();
    let b = CompatibilityId::new("engine-v1").unwrap();
    let c = CompatibilityId::new("engine-v2").unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
}
