use super::{
    ProjectedView, ProjectionCoordinator, ProjectionError, Residency, ResizeOutcome,
    budgets::Lease,
    state::{Command, NativeWorkspace, Reply, Resizing},
};
use crate::scheduling::WorkSchedule;
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
    pub(super) fn poll_inflight_operations(
        &self,
        workspace: &mut NativeWorkspace,
    ) -> Option<WorkSchedule> {
        let handle = self.wiring.handle()?;
        let waker = Waker::from(Arc::new(WorkWake(handle)));
        let mut context = Context::from_waker(&waker);
        if let Some(reply) = &mut workspace.reply {
            if reply.operation.is_none() {
                let process = self.wiring.process();
                let Some(process) = process else {
                    return Some(WorkSchedule::Dormant);
                };
                let lease = match self.input.reserve(reply.bytes.len()) {
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
                        workspace.reply = None;
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
                        workspace.reply = None;
                    }
                }
            }
        }
        if let Some(resize) = &mut workspace.resize {
            match resize.operation.as_mut().poll(&mut context) {
                Poll::Pending => return Some(WorkSchedule::Dormant),
                Poll::Ready(os) => {
                    let model = match os {
                        Err(error) => Err(ProjectionError::Process(error)),
                        Ok(()) => match &mut workspace.terminal {
                            Some(terminal) => {
                                self.native_call(|| terminal.resize(resize.size, resize.generation))
                            }
                            None => Err(ProjectionError::Closed),
                        },
                    };
                    if model.is_ok() {
                        workspace.config.size = resize.size;
                        let mut admission =
                            self.admission.lock().unwrap_or_else(|e| e.into_inner());
                        let controlled = admission.policy.record_control_applied(resize.generation);
                        drop(admission);
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
                    workspace.resize = None;
                }
            }
        }
        None
    }
    pub(super) fn apply_command(
        &self,
        workspace: &mut NativeWorkspace,
        event: Command,
    ) -> WorkSchedule {
        match event {
            Command::Output(bytes, mut staging) => {
                let memory = match Lease::shared(
                    self.quotas.shared.views.clone(),
                    workspace.config.reply_bytes,
                ) {
                    Ok(memory) => memory,
                    Err(_) => {
                        self.requeue(Command::Output(bytes, staging));
                        return WorkSchedule::After(Duration::from_millis(5));
                    }
                };
                let result = match &mut workspace.terminal {
                    Some(terminal) => self.native_call(|| terminal.feed(&bytes)),
                    None => Err(ProjectionError::Closed),
                };
                match result {
                    Ok(mut effects) => {
                        if effects.0.len() > workspace.config.reply_bytes {
                            effects.0.fill(0);
                            self.fail(ProjectionError::Capacity);
                        } else {
                            let processed = self
                                .admission
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .policy
                                .record_processed(bytes.len());
                            if let Err(error) = processed {
                                self.fail(error);
                            } else {
                                self.journal.output(bytes, self.status().processed);
                                if let Some(timing) = staging.timing.take() {
                                    timing.finish(true);
                                }
                            }
                            if !effects.0.is_empty() {
                                workspace.reply = Some(Reply {
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
                        self.requeue(Command::Output(bytes, staging));
                        self.fail(error);
                        return WorkSchedule::Dormant;
                    }
                }
            }
            Command::Resize(size, ticket, mut staging) => {
                let process = self.wiring.process();
                let Some(process) = process else {
                    self.requeue(Command::Resize(size, ticket, staging));
                    return WorkSchedule::Dormant;
                };
                let Some(generation) = self.status().control_generation.next() else {
                    ticket.complete(Err(ProjectionError::Capacity));
                    self.fail(ProjectionError::Capacity);
                    return WorkSchedule::Dormant;
                };
                match process.resize_timed(size, staging.timing.take()) {
                    Ok(operation) => {
                        workspace.resize = Some(Resizing {
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
            Command::View(ticket, _staging) => {
                if !ticket.cancelled() {
                    let mut options = self.wiring.options;
                    options.terminal = workspace.config;
                    let result = options
                        .view_reservation()
                        .and_then(|count| Lease::shared(self.quotas.shared.views.clone(), count))
                        .and_then(|lease| {
                            let terminal =
                                workspace.terminal.as_mut().ok_or(ProjectionError::Closed)?;
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
            Command::Checkpoint(request, _) => self.snapshot(workspace, request),
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
    pub(super) fn closing_resize(&self, workspace: &mut NativeWorkspace) -> Option<WorkSchedule> {
        let resize = workspace.resize.as_mut()?;
        let handle = self.wiring.handle()?;
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
                workspace.resize = None;
                None
            }
        }
    }
    pub(super) fn descriptor(&self) -> CheckpointDescriptor {
        let status = self.status();
        CheckpointDescriptor {
            compatibility: self.wiring.compatibility.clone(),
            processed: status.processed,
            control_generation: status.control_generation,
        }
    }
    pub(super) fn requeue(&self, event: Command) {
        let mut admission = self.admission.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(
            admission.policy.status().residency,
            Residency::Closing | Residency::Closed
        ) {
            drop(admission);
            event.fail(ProjectionError::Closed);
        } else {
            admission.queue.push_front(event);
        }
    }
}
