//! OS resize failures must not publish a model generation that did not happen.
use super::{support::*, terminal::Trace};
use crate::projection::{ProjectionError, Residency};
use pty_runtime_domain::terminal::ControlGeneration;
use pty_runtime_domain::{process::ProcessError, terminal::TerminalSize};
use std::sync::atomic::Ordering;

#[test]
fn resize_admission_and_completion_errors_preserve_model_and_allow_same_generation_retry() {
    for admission in [true, false] {
        let h = Harness::standard();
        h.process
            .fail_resize_admission
            .store(admission, Ordering::Release);
        h.process
            .fail_resize_completion
            .store(!admission, Ordering::Release);
        let target = TerminalSize::new(4, 3).unwrap();
        let mut resize = h.owner.resize(target).unwrap();
        h.pump();
        let outcome = result(&mut resize).unwrap();
        assert_eq!(outcome.generation, ControlGeneration::from_raw(1));
        assert_eq!(outcome.os, Err(ProcessError::Io));
        assert_eq!(
            outcome.model,
            Err(ProjectionError::Process(ProcessError::Io))
        );
        drop(resize);
        assert_eq!(h.owner.status().residency, Residency::Resident);
        assert_eq!(h.owner.status().failure, None);
        assert_eq!(
            h.owner.status().control_generation,
            ControlGeneration::from_raw(0)
        );
        assert_eq!(h.budgets.resources().staging_slots.used, 0);
        assert_eq!(h.budgets.resources().requests.used, 0);
        assert!(
            !h.probe
                .trace
                .lock()
                .unwrap()
                .iter()
                .any(|t| matches!(t, Trace::Resize(_)))
        );
        let mut view = h.owner.view().unwrap();
        h.pump();
        assert_eq!(
            result(&mut view).unwrap().view().size,
            options().terminal.size
        );
        drop(view);
        h.process
            .fail_resize_admission
            .store(false, Ordering::Release);
        h.process
            .fail_resize_completion
            .store(false, Ordering::Release);
        let mut retry = h.owner.resize(target).unwrap();
        h.pump();
        let outcome = result(&mut retry).unwrap();
        assert_eq!(outcome.generation, ControlGeneration::from_raw(1));
        assert_eq!(outcome.os, Ok(()));
        assert_eq!(outcome.model, Ok(()));
        assert_eq!(
            h.owner.status().control_generation,
            ControlGeneration::from_raw(1)
        );
        let mut view = h.owner.view().unwrap();
        h.pump();
        assert_eq!(result(&mut view).unwrap().view().size, target);
        drop((retry, view));
        h.close();
    }
}
