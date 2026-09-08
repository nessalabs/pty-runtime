use super::*;

#[test]
fn identity_enforces_byte_limit_and_control_policy() {
    for invalid in [String::new(), "x".repeat(129), "é".repeat(65)] {
        assert_eq!(SessionId::new(invalid), Err(InvalidSessionId));
    }
    for control in ['\0', '\n', '\r', '\t', '\u{007f}', '\u{0085}'] {
        assert_eq!(
            SessionId::new(format!("before{control}after")),
            Err(InvalidSessionId)
        );
    }
    for valid in ["x".into(), "x".repeat(128), "é".repeat(64), "🦀".repeat(32)] {
        assert!(SessionId::new(valid).is_ok());
    }
}

#[test]
fn accepted_identity_discards_excess_owned_capacity_and_redacts() {
    let mut oversized = String::with_capacity(1024 * 1024);
    oversized.push_str("synthetic-private-identity");
    let id = SessionId::new(oversized).unwrap();
    assert!(!format!("{id:?}").contains("synthetic-private-identity"));
    let owned = id.0.into_string();
    assert_eq!(owned.capacity(), owned.len());
}

#[test]
fn identity_equality_is_literal_and_lifetimes_include_both_parts() {
    let first = SessionId::new("a".into()).unwrap();
    assert_eq!(first, first.clone());
    assert_ne!(first, SessionId::new("A".into()).unwrap());
    assert_ne!(SessionLifetime::new(1, 1), SessionLifetime::new(1, 2));
    assert_ne!(SessionLifetime::new(1, 1), SessionLifetime::new(2, 1));
}
