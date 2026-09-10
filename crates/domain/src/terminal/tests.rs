//! Engine-neutral terminal value bounds.
use super::{CompatibilityId, TerminalError};

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
