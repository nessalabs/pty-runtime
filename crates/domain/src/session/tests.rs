use super::*;

#[test]
fn repeated_completion_facts_are_idempotent_and_contradictions_preserve_first_facts() {
    for drain_first in [false, true] {
        let mut status = SessionStatus::default();
        if drain_first {
            status.record_drain(DrainOutcome::Eof).unwrap();
        } else {
            status.record_exit(ExitStatus::Code(7)).unwrap();
        }
        assert!(status.completion().is_none());
        status.record_exit(ExitStatus::Code(7)).unwrap();
        status.record_drain(DrainOutcome::Eof).unwrap();
        let completed = status;
        status.record_exit(ExitStatus::Code(7)).unwrap();
        status.record_drain(DrainOutcome::Eof).unwrap();
        assert_eq!(status, completed);
        assert_eq!(
            status.record_exit(ExitStatus::Code(0)),
            Err(RuntimeError::Internal)
        );
        assert_eq!(
            status.record_drain(DrainOutcome::Failed(ProcessError::Io)),
            Err(RuntimeError::Internal)
        );
        assert_eq!(status, completed);
        assert_eq!(status.admit_cancel(), Err(RuntimeError::Closed));
        assert_eq!(status, completed);
        assert_eq!(status.completion().unwrap().status, completed);
    }
}

#[test]
fn admission_and_supervision_failure_require_drain_without_inventing_exit() {
    for admission in [false, true] {
        let mut status = SessionStatus::default();
        if admission {
            status.record_admission_failure(RuntimeError::Capacity);
            status.record_admission_failure(RuntimeError::Internal);
            assert_eq!(status.admission_error, Some(RuntimeError::Capacity));
            assert_eq!(status.supervision_error, None);
        } else {
            status.record_failure(ProcessError::PermissionDenied);
            status.record_failure(ProcessError::Internal);
            assert_eq!(
                status.supervision_error,
                Some(ProcessError::PermissionDenied)
            );
            assert_eq!(status.admission_error, None);
        }
        assert_eq!(status.exit, None);
        assert!(status.completion().is_none());
        status.record_drain(DrainOutcome::Eof).unwrap();
        let completed = status.completion().unwrap();
        assert_eq!(completed.status.exit, None);
        assert_eq!(completed.status, status);
    }
}
