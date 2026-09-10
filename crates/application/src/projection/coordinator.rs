use super::{
    ProjectionBudgets, ProjectionError, ProjectionOptions, ProjectionStatus,
    budgets::Lease,
    state::{Admission, NativeWorkspace},
};
use crate::{
    checkpoint::{ICheckpointProtector, ICheckpointStore},
    process::IProcessSession,
    runtime::quota::Quota,
    scheduling::{
        IBlockingExecutor, ICapacitySignal, IClock, IScheduledWork, IWorkHandle, IWorkScheduler,
    },
    terminal::ITerminalFactory,
};
use pty_runtime_domain::{SessionLifetime, projection::ProjectionPolicy};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

/// Explicit external boundaries. Native work and slow provider work use separate pools.
#[derive(Clone)]
pub struct ProjectionServices {
    /// Creates exclusive engine owners and interprets compatibility identities.
    pub terminal: Arc<dyn ITerminalFactory>,
    /// Monotonic policy time, injectable for deterministic idle tests.
    pub clock: Arc<dyn IClock>,
    /// Coalesced serialized model work; weak registrations avoid ownership cycles.
    pub scheduler: Arc<dyn IWorkScheduler>,
    /// Bounded workers for all protection and provider operations.
    pub blocking: Arc<dyn IBlockingExecutor>,
    /// Runtime-shared capacity notifications retained before a reader waits.
    pub capacity: Arc<dyn ICapacitySignal>,
    /// Immutable bounded opaque provider; never receives plaintext.
    pub store: Arc<dyn ICheckpointStore>,
    /// Owner-lifetime authenticated protection, separate from storage.
    pub protector: Arc<dyn ICheckpointProtector>,
}
/// One authoritative model and independently bounded parser backlog for a process lifetime.
/// The runtime retains this owner until explicit close cleanup completes. Raw process
/// state and cancellation never require the native ownership mutex.
pub struct ProjectionCoordinator {
    pub(super) journal: Arc<super::journal::Journal>,
    pub(super) options: ProjectionOptions,
    pub(super) protected_bytes: usize,
    pub(super) services: Mutex<Option<ProjectionServices>>,
    pub(super) budgets: Arc<ProjectionBudgets>,
    pub(super) admission: Mutex<Admission>,
    pub(super) workspace: Mutex<NativeWorkspace>,
    pub(super) handle: Mutex<Option<Arc<dyn IWorkHandle>>>,
    pub(super) local_bytes: Arc<Quota>,
    pub(super) local_slots: Arc<Quota>,
    pub(super) local_requests: Arc<Quota>,
    pub(super) input_bytes: Arc<Quota>,
    pub(super) input_slots: Arc<Quota>,
    pub(super) process: Mutex<Option<Arc<dyn IProcessSession>>>,
    pub(super) stall_generation: AtomicU64,
}
impl ProjectionCoordinator {
    /// Reserve model admission before creating native state or launching the child.
    /// The containing application runtime supplies its existing input quotas.
    pub(crate) fn create(
        lifetime: SessionLifetime,
        options: ProjectionOptions,
        services: ProjectionServices,
        budgets: Arc<ProjectionBudgets>,
        input_bytes: Arc<Quota>,
        input_slots: Arc<Quota>,
    ) -> Result<Arc<Self>, ProjectionError> {
        let options = options.validate()?;
        // Every queued control owns a request ticket, while output owns a parser
        // slot. Reserve both finite populations before accepting either kind.
        let queue_slots = options
            .staging_slots
            .checked_add(options.request_slots)
            .ok_or(ProjectionError::InvalidConfiguration)?;
        let protected_bytes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            services
                .protector
                .protected_size_limit(options.terminal.checkpoint_bytes)
        }))
        .map_err(|_| ProjectionError::Worker)??;
        let peak = protected_bytes
            .checked_add(options.terminal.checkpoint_bytes)
            .ok_or(ProjectionError::InvalidConfiguration)?;
        if protected_bytes == 0
            || protected_bytes > isize::MAX as usize
            || budgets.limits.checkpoint_bytes < peak
            || budgets.limits.stored_bytes < protected_bytes
            || budgets.limits.staging_bytes < options.terminal.feed_bytes
            || budgets.limits.view_bytes
                < options
                    .view_reservation()?
                    .max(options.terminal.reply_bytes)
        {
            return Err(ProjectionError::InvalidConfiguration);
        }
        if !services.terminal.capabilities().checkpoints {
            return Err(ProjectionError::InvalidConfiguration);
        }
        let resident = Lease::shared(budgets.resident.clone(), options.terminal.native_bytes)?;
        if services.terminal.compatibility().is_empty()
            || services.terminal.compatibility().len() > 4096
        {
            return Err(ProjectionError::InvalidConfiguration);
        }
        let terminal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            services.terminal.create(options.terminal)
        }))
        .map_err(|_| ProjectionError::Worker)??;
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(queue_slots)
            .map_err(|_| ProjectionError::Capacity)?;
        let owner = Arc::new(Self {
            journal: super::journal::Journal::new(lifetime, options, &budgets),
            admission: Mutex::new(Admission {
                policy: ProjectionPolicy::new(lifetime, options, services.clock.now())?,
                queue,
                output_drain: None,
                close_waiters: Vec::new(),
                unreclaimed_failure: None,
                cleanup_failure: None,
                retry_cleanup: false,
            }),
            workspace: Mutex::new(NativeWorkspace {
                terminal: Some(terminal),
                resident: Some(resident),
                source: None,
                restore_memory: None,
                io: None,
                reply: None,
                resize: None,
                pending_deletes: VecDeque::new(),
                config: options.terminal,
                history_step_owed: false,
            }),
            local_bytes: Arc::new(Quota::new(options.staging_bytes)),
            local_slots: Arc::new(Quota::new(options.staging_slots)),
            local_requests: Arc::new(Quota::new(options.request_slots)),
            options,
            protected_bytes,
            services: Mutex::new(Some(services)),
            budgets,
            handle: Mutex::new(None),
            input_bytes,
            input_slots,
            process: Mutex::new(None),
            stall_generation: AtomicU64::new(0),
        });
        let work: Arc<dyn IScheduledWork> = owner.clone();
        let handle = owner
            .services()?
            .scheduler
            .register(Arc::downgrade(&work))
            .map_err(|_| ProjectionError::Worker)?;
        *owner.handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
        owner.wake()?;
        Ok(owner)
    }
    /// Bind exactly one admitted process, then wake any replies waiting for startup.
    pub fn bind_process(&self, process: Arc<dyn IProcessSession>) -> Result<(), ProjectionError> {
        let mut slot = self.process.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err(ProjectionError::InvalidConfiguration);
        }
        if matches!(
            self.status().residency,
            super::Residency::Closing | super::Residency::Closed
        ) {
            return Err(ProjectionError::Closed);
        }
        *slot = Some(process);
        drop(slot);
        let result = self.wake();
        if let Err(error) = result {
            self.fail(error);
        }
        result
    }
    /// Independent exact parser positions, residency and failure facts.
    pub fn status(&self) -> ProjectionStatus {
        self.admission
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy
            .status()
    }
    /// Sleep using the generation observed before the preceding rejected admission.
    /// A notification between rejection and this call is never lost. One PTY reader
    /// owns this admission/wait pair; cancellation is bounded by the caller deadline.
    pub fn wait_for_capacity(&self, deadline: Instant) {
        let generation = self.stall_generation.load(Ordering::Acquire);
        if !matches!(
            self.status().residency,
            super::Residency::Closing | super::Residency::Closed
        ) {
            if let Ok(services) = self.services() {
                services.capacity.wait_after(generation, deadline);
            }
        }
    }
    pub(super) fn services(&self) -> Result<ProjectionServices, ProjectionError> {
        self.services
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(ProjectionError::Closed)
    }
    pub(super) fn wake(&self) -> Result<(), ProjectionError> {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or(ProjectionError::Worker)?;
        handle.wake().map_err(|_| ProjectionError::Worker)
    }
}
