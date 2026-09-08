use super::*;
use std::{ffi::OsString, path::PathBuf, time::Duration};
#[test]
fn commands_are_literal_bounded_and_redacted() {
    let args = vec![
        OsString::from("$(do-not-execute)"),
        OsString::from("private-marker"),
    ];
    let command =
        CommandSpec::new(PathBuf::from("/bin/test"), PathBuf::from("/tmp"), args).unwrap();
    assert_eq!(command.arguments().next().unwrap(), "$(do-not-execute)");
    assert_eq!(command.environment(), EnvironmentPolicy::Empty);
    assert!(!format!("{command:?}").contains("private-marker"));
    assert!(CommandSpec::new(PathBuf::from("relative"), PathBuf::from("/tmp"), vec![]).is_err());
    assert!(
        CommandSpec::new(
            PathBuf::from("/bin/test"),
            PathBuf::from("relative"),
            vec![]
        )
        .is_err()
    );
    assert!(
        CommandSpec::new(
            PathBuf::from("/bin/test"),
            PathBuf::from("/tmp"),
            vec!["x".repeat(8193).into()]
        )
        .is_err()
    );
    assert!(
        CommandSpec::new(
            PathBuf::from("/bin/test"),
            PathBuf::from("/tmp"),
            vec!["\0".into()]
        )
        .is_err()
    );
}
#[test]
fn environment_keys_validate_and_overrides_remain_last() {
    let command = CommandSpec::new("/bin/test".into(), "/tmp".into(), vec![]).unwrap();
    assert!(
        command
            .clone()
            .with_environment(
                EnvironmentPolicy::Inherit,
                vec![],
                vec![("BAD=NAME".into(), "value".into())]
            )
            .is_err()
    );
    assert!(
        command
            .clone()
            .with_environment(EnvironmentPolicy::Empty, vec!["".into()], vec![])
            .is_err()
    );
    let command = command
        .with_environment(
            EnvironmentPolicy::Inherit,
            vec!["KEY".into()],
            vec![("KEY".into(), "private-value".into())],
        )
        .unwrap();
    assert_eq!(command.removals().next().unwrap(), "KEY");
    assert_eq!(command.overrides().next().unwrap().1, "private-value");
    assert!(!format!("{command:?}").contains("private-value"));
}
#[test]
fn deadline_and_allocation_extremes_are_rejected() {
    let mut limits = ProcessLimits::default();
    assert!(limits.validate().is_ok());
    limits.terminate_grace = Duration::MAX;
    assert_eq!(limits.validate(), Err(ProcessError::Capacity));
    limits.terminate_grace = Duration::ZERO;
    assert!(limits.validate().is_ok());
    limits.read_chunk = 65537;
    assert_eq!(limits.validate(), Err(ProcessError::Capacity));
}
