//! Failed adapter construction must release native admission before returning.
use super::support::*;
use crate::{
    projection::{
        ProjectionBudgets, ProjectionCoordinator, ProjectionError, ProjectionLimits,
        ProjectionServices,
    },
    runtime::quota::Quota,
    scheduling::{IScheduledWork, IWorkHandle, IWorkScheduler, SchedulingError},
    terminal::{ITerminal, ITerminalFactory},
};
use pty_runtime_domain::{SessionLifetime, terminal::*};
use std::sync::{Arc, Weak, atomic::Ordering};

fn create(
    services: ProjectionServices,
    budgets: Arc<ProjectionBudgets>,
) -> Result<Arc<ProjectionCoordinator>, ProjectionError> {
    ProjectionCoordinator::create(
        SessionLifetime::new(92, 1),
        options(),
        services,
        budgets,
        Arc::new(Quota::new(128)),
        Arc::new(Quota::new(8)),
    )
}
struct FaultFactory {
    inner: Arc<dyn ITerminalFactory>,
    mode: u8,
}
impl ITerminalFactory for FaultFactory {
    fn capabilities(&self) -> TerminalCapabilities {
        let mut caps = self.inner.capabilities();
        if self.mode == 0 {
            caps.checkpoints = false;
        }
        caps
    }
    fn compatibility(&self) -> Result<CompatibilityId, TerminalError> {
        self.inner.compatibility()
    }
    fn create(&self, config: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        match self.mode {
            1 => panic!("synthetic adapter construction panic"),
            2 => Err(TerminalError::EngineFailure),
            _ => self.inner.create(config),
        }
    }
    fn restore(
        &self,
        checkpoint: TerminalCheckpoint,
        config: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        self.inner.restore(checkpoint, config)
    }
}
struct RejectScheduler;
impl IWorkScheduler for RejectScheduler {
    fn register(
        &self,
        _: Weak<dyn IScheduledWork>,
    ) -> Result<Arc<dyn IWorkHandle>, SchedulingError> {
        Err(SchedulingError::Capacity)
    }
    fn shutdown(&self) {}
}

#[test]
fn failed_capability_factory_or_scheduler_admission_leaks_no_native_owner() {
    let h = Harness::standard();
    for mode in 0..4 {
        let mut services = h.services.clone();
        services.terminal = Arc::new(FaultFactory {
            inner: services.terminal.clone(),
            mode,
        });
        if mode == 3 {
            services.scheduler = Arc::new(RejectScheduler);
        }
        let budgets = ProjectionBudgets::new(ProjectionLimits::default()).unwrap();
        let expected = match mode {
            0 => ProjectionError::InvalidConfiguration,
            2 => ProjectionError::Terminal(TerminalError::EngineFailure),
            _ => ProjectionError::Worker,
        };
        assert!(matches!(create(services, budgets.clone()), Err(error) if error == expected));
        assert_eq!(budgets.resources().native_reservations.used, 0);
        assert_eq!(
            h.probe.alive.load(Ordering::Acquire),
            1,
            "only the fixture's original model survives"
        );
    }
    h.close();
    assert_budgets_released(&h);
}

#[test]
fn impossible_shared_budgets_and_protector_bounds_fail_before_native_allocation() {
    let h = Harness::standard();
    let policies = [
        ProjectionLimits {
            view_bytes: 1,
            ..ProjectionLimits::default()
        },
        ProjectionLimits {
            checkpoint_bytes: 1,
            ..ProjectionLimits::default()
        },
        ProjectionLimits {
            stored_bytes: 1,
            ..ProjectionLimits::default()
        },
        ProjectionLimits {
            staging_bytes: 1,
            ..ProjectionLimits::default()
        },
        ProjectionLimits {
            resident_bytes: options().terminal.native_bytes - 1,
            ..ProjectionLimits::default()
        },
    ];
    for (index, policy) in policies.into_iter().enumerate() {
        let budgets = ProjectionBudgets::new(policy).unwrap();
        let expected = if index == 4 {
            ProjectionError::Capacity
        } else {
            ProjectionError::InvalidConfiguration
        };
        assert!(
            matches!(create(h.services.clone(), budgets.clone()), Err(error) if error == expected)
        );
        assert_eq!(budgets.resources().native_reservations.used, 0);
        assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    }
    h.protector
        .compressed_bound
        .store(usize::MAX, Ordering::Release);
    let budgets = ProjectionBudgets::new(ProjectionLimits::default()).unwrap();
    assert!(matches!(
        create(h.services.clone(), budgets.clone()),
        Err(ProjectionError::InvalidConfiguration)
    ));
    assert_eq!(budgets.resources().native_reservations.used, 0);
    h.protector.compressed_bound.store(0, Ordering::Release);
    h.close();
    assert_budgets_released(&h);
}
