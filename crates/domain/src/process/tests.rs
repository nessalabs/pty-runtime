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

/// The guardian's deadline must land after the owner's, or it is not a backstop.
///
/// Sharing a deadline is what made the two cancellation triggers untestable:
/// they fired together, so no test could attribute a kill to either and either
/// one could be deleted with the suite still green.
#[test]
fn the_guardian_deadline_is_strictly_later_than_the_owners() {
    for grace in [
        Duration::from_millis(1),
        Duration::from_millis(40),
        Duration::from_millis(250),
        Duration::from_secs(1),
        Duration::from_secs(60),
        Duration::from_secs(3600),
    ] {
        let limits = ProcessLimits {
            terminate_grace: grace,
            ..ProcessLimits::default()
        };
        assert!(
            limits.guardian_grace() > grace,
            "guardian must wait longer than the owner for {grace:?}"
        );
        assert!(
            limits.guardian_grace() <= Duration::from_secs(86400),
            "guardian rejects anything past its own 24-hour ceiling"
        );
        assert!(
            limits.guardian_grace() <= grace + Duration::from_secs(5),
            "a long grace must not push the backstop hours past it"
        );
    }
}

/// A grace at the ceiling cannot be extended, so the two converge there rather
/// than producing a deadline the guardian would refuse to start with.
#[test]
fn a_grace_at_the_ceiling_clamps_instead_of_overflowing() {
    let limits = ProcessLimits {
        terminate_grace: Duration::from_secs(86400),
        ..ProcessLimits::default()
    };
    assert_eq!(limits.guardian_grace(), Duration::from_secs(86400));

    // `validate` rejects anything past the ceiling, so the clamp is only ever
    // reached by a grace that is itself at it.
    let beyond = ProcessLimits {
        terminate_grace: Duration::from_secs(86401),
        ..ProcessLimits::default()
    };
    assert!(beyond.validate().is_err());
}
