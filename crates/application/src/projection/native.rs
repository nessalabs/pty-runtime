use super::{
    ProjectedView, ProjectionCoordinator, ProjectionError, Residency, ResizeOutcome,
    budgets::Lease,
    state::{Engine, Event, Reply, Resizing},
};
use crate::{runtime::quota::InputLease, scheduling::WorkSchedule};
use pty_runtime_domain::{process::ProcessError, terminal::CheckpointDescriptor};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::Duration,
};
struct WorkWake(Arc<dyn crate::scheduling::IWorkHandle>);
impl Wake for WorkWake {
    fn wake(self: Arc<Self>) {
        let _ = self.0.wake();
    }
}
impl ProjectionCoordinator {
    pub(super) fn pending_native(&self, engine: &mut Engine) -> Option<WorkSchedule> {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        let waker = Waker::from(Arc::new(WorkWake(handle)));
        let mut context = Context::from_waker(&waker);
        if let Some(reply) = &mut engine.reply {
            if reply.operation.is_none() {
                let process = self
                    .process
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let Some(process) = process else {
                    return Some(WorkSchedule::Dormant);
                };
                let lease = match InputLease::acquire(
                    self.input_bytes.clone(),
                    self.input_slots.clone(),
                    reply.bytes.len(),
                ) {
                    Ok(lease) => lease,
                    Err(_) => return Some(WorkSchedule::After(Duration::from_millis(5))),
                };
                match process.write_reserved(&reply.bytes, Some(Box::new(lease))) {
                    Ok(operation) => reply.operation = Some(operation),
                    Err(ProcessError::Capacity) => {
                        return Some(WorkSchedule::After(Duration::from_millis(5)));
                    }
                    Err(error) => {
                        self.fail(error.into());
                        engine.reply = None;
                        return Some(WorkSchedule::Dormant);
                    }
                }
            }
            if let Some(operation) = &mut reply.operation {
                match operation.as_mut().poll(&mut context) {
                    Poll::Pending => return Some(WorkSchedule::Dormant),
                    Poll::Ready(outcome) => {
                        if outcome.error.is_some() || outcome.written != reply.bytes.len() {
                            self.fail(ProjectionError::Process(
                                outcome.error.unwrap_or(ProcessError::Io),
                            ));
                        }
                        engine.reply = None;
                    }
                }
            }
        }
        if let Some(resize) = &mut engine.resize {
            match resize.operation.as_mut().poll(&mut context) {
                Poll::Pending => return Some(WorkSchedule::Dormant),
                Poll::Ready(os) => {
                    let model = match os {
                        Err(error) => Err(ProjectionError::Process(error)),
                        Ok(()) => match &mut engine.terminal {
                            Some(terminal) => {
                                self.native_call(|| terminal.resize(resize.size, resize.generation))
                            }
                            None => Err(ProjectionError::Closed),
                        },
                    };
                    if model.is_ok() {
                        engine.config.size = resize.size;
                        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                        let controlled = core.policy.controlled(resize.generation);
                        drop(core);
                        if let Err(error) = controlled {
                            self.fail(error);
                        } else {
                            self.journal.resized(resize.size, resize.generation);
                        }
                    } else if os.is_ok() {
                        if let Err(error) = model {
                            self.fail(error);
                        }
                    }
                    resize.ticket.complete(Ok(ResizeOutcome {
                        generation: resize.generation,
                        os,
                        model,
                    }));
                    engine.resize = None;
                }
            }
        }
        None
    }
    pub(super) fn native_event(&self, engine: &mut Engine, event: Event) -> WorkSchedule {
        match event {
            Event::Output(bytes, mut staging) => {
                let memory = match Lease::one(self.budgets.views.clone(), engine.config.reply_bytes)
                {
                    Ok(memory) => memory,
                    Err(_) => {
                        self.requeue(Event::Output(bytes, staging));
                        return WorkSchedule::After(Duration::from_millis(5));
                    }
                };
                let result = match &mut engine.terminal {
                    Some(terminal) => self.native_call(|| terminal.feed(&bytes)),
                    None => Err(ProjectionError::Closed),
                };
                match result {
                    Ok(mut effects) => {
                        if effects.0.len() > engine.config.reply_bytes {
                            effects.0.fill(0);
                            self.fail(ProjectionError::Capacity);
                        } else {
                            let processed = self
                                .core
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .policy
                                .processed(bytes.len());
                            if let Err(error) = processed {
                                self.fail(error);
                            } else {
                                self.journal.output(bytes, self.status().processed);
                                if let Some(timing) = staging.timing.take() {
                                    timing.finish(true);
                                }
                            }
                            if !effects.0.is_empty() {
                                engine.reply = Some(Reply {
                                    bytes: effects.0,
                                    _memory: memory,
                                    operation: None,
                                });
                            }
                        }
                    }
                    Err(error) => {
                        // A native error can occur after mutation. Retain the affected
                        // original bytes and fail projection; never feed them again.
                        self.requeue(Event::Output(bytes, staging));
                        self.fail(error);
                        return WorkSchedule::Dormant;
                    }
                }
            }
            Event::Resize(size, ticket, mut staging) => {
                let process = self
                    .process
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let Some(process) = process else {
                    self.requeue(Event::Resize(size, ticket, staging));
                    return WorkSchedule::Dormant;
                };
                let Some(generation) = self.status().control_generation.checked_add(1) else {
                    ticket.complete(Err(ProjectionError::Capacity));
                    self.fail(ProjectionError::Capacity);
                    return WorkSchedule::Dormant;
                };
                match process.resize_timed(size, staging.timing.take()) {
                    Ok(operation) => {
                        engine.resize = Some(Resizing {
                            size,
                            generation,
                            ticket,
                            _staging: staging,
                            operation,
                        })
                    }
                    Err(error) => ticket.complete(Ok(ResizeOutcome {
                        generation,
                        os: Err(error),
                        model: Err(error.into()),
                    })),
                }
            }
            Event::View(ticket, _staging) => {
                if !ticket.cancelled() {
                    let mut options = self.options;
                    options.terminal = engine.config;
                    let result = options
                        .view_reservation()
                        .and_then(|count| Lease::one(self.budgets.views.clone(), count))
                        .and_then(|lease| {
                            let terminal =
                                engine.terminal.as_mut().ok_or(ProjectionError::Closed)?;
                            let view = self.native_call(|| terminal.view())?;
                            let history =
                                self.native_call(|| Ok(terminal.restoration_progress()))?;
                            let status = self.status();
                            Ok(ProjectedView {
                                view,
                                processed: status.processed,
                                control_generation: status.control_generation,
                                history,
                                _lease: lease,
                            })
                        });
                    ticket.complete(
                        if matches!(
                            self.status().residency,
                            Residency::Closing | Residency::Closed
                        ) {
                            Err(ProjectionError::Closed)
                        } else {
                            result
                        },
                    );
                }
            }
            Event::Checkpoint(request, _) => self.snapshot(engine, request),
        }
        WorkSchedule::After(Duration::ZERO)
    }
    pub(super) fn native_call<T>(
        &self,
        work: impl FnOnce() -> Result<T, pty_runtime_domain::terminal::TerminalError>,
    ) -> Result<T, ProjectionError> {
        match catch_unwind(AssertUnwindSafe(work)) {
            Ok(result) => result.map_err(ProjectionError::from),
            Err(_) => {
                self.fail(ProjectionError::Worker);
                Err(ProjectionError::Worker)
            }
        }
    }
    pub(super) fn closing_resize(&self, engine: &mut Engine) -> Option<WorkSchedule> {
        let resize = engine.resize.as_mut()?;
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()?;
        let waker = Waker::from(Arc::new(WorkWake(handle)));
        let mut context = Context::from_waker(&waker);
        match resize.operation.as_mut().poll(&mut context) {
            Poll::Pending => Some(WorkSchedule::Dormant),
            Poll::Ready(os) => {
                resize.ticket.complete(Ok(ResizeOutcome {
                    generation: resize.generation,
                    os,
                    model: Err(ProjectionError::Closed),
                }));
                engine.resize = None;
                None
            }
        }
    }
    pub(super) fn descriptor(&self) -> CheckpointDescriptor {
        let status = self.status();
        CheckpointDescriptor {
            compatibility: self
                .services()
                .map(|services| services.terminal.compatibility().into())
                .unwrap_or_default(),
            processed: status.processed,
            control_generation: status.control_generation,
        }
    }
    pub(super) fn requeue(&self, event: Event) {
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(
            core.policy.status().residency,
            Residency::Closing | Residency::Closed
        ) {
            drop(core);
            event.fail(ProjectionError::Closed);
        } else {
            core.queue.push_front(event);
        }
    }
}
