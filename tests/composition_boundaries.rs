//! Default adapter admission and configuration redaction at the public facade.
use pty_runtime::StorageOptions;

#[test]
fn storage_debug_reports_configuration_without_disclosing_the_parent_path() {
    let storage = StorageOptions {
        parent: Some("/private/fixture-secret-parent".into()),
        max_bytes: 1234,
    };
    let text = format!("{storage:?}");
    assert!(text.contains("configured_parent: true"));
    assert!(text.contains("max_bytes: 1234"));
    assert!(!text.contains("fixture-secret-parent"));
    assert!(format!("{:?}", StorageOptions::default()).contains("configured_parent: false"));
}

#[cfg(feature = "ghostty")]
#[test]
fn invalid_default_storage_is_rejected_before_a_runtime_is_created() {
    use pty_runtime::{
        ProjectionError, Runtime, RuntimeError, RuntimeOptions, checkpoint::CheckpointError,
    };
    let error = Runtime::with_storage(
        vec![],
        RuntimeOptions::default(),
        StorageOptions {
            parent: None,
            max_bytes: 0,
        },
    )
    .err()
    .expect("zero storage budget accepted");
    assert_eq!(
        error,
        RuntimeError::Projection(ProjectionError::Storage(
            CheckpointError::InvalidConfiguration
        ))
    );
}
