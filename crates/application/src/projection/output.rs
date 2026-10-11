use super::{
    ProjectionCoordinator, ProjectionError,
    budgets::StagingLease,
    program_status::{PreparedStatus, StatusAdmissionError},
    state::{Command, NativeWorkspace, Reply},
};
use crate::{scheduling::WorkSchedule, terminal::ITerminal};
use pty_runtime_domain::terminal::{TerminalEffects, program_status::PROGRAM_STATUS_REPLY};
use std::time::Duration;

impl ProjectionCoordinator {
    pub(super) fn apply_output(
        &self,
        workspace: &mut NativeWorkspace,
        bytes: Vec<u8>,
        mut staging: StagingLease,
    ) -> WorkSchedule {
        let prepared = match self.prepare_status(&bytes, workspace.config.reply_bytes) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.requeue(Command::Output(bytes, staging));
                return match error {
                    StatusAdmissionError::Retry => WorkSchedule::After(Duration::from_millis(5)),
                    StatusAdmissionError::Failed(error) => {
                        self.fail(error);
                        WorkSchedule::Dormant
                    }
                };
            }
        };
        let result = match &mut workspace.terminal {
            Some(terminal) => self.feed_status_slices(
                terminal.as_mut(),
                &bytes,
                prepared.plan.query_offsets(),
                workspace.config.reply_bytes,
            ),
            None => Err(ProjectionError::Closed),
        };
        let mut effects = match result {
            Ok(effects) => effects,
            Err(error) => {
                // Feed failures may have accepted any prefix. Retain diagnostic
                // original bytes but fail projection, so that prefix is never retried.
                self.requeue(Command::Output(bytes, staging));
                self.fail(error);
                return WorkSchedule::Dormant;
            }
        };
        let PreparedStatus {
            plan,
            _scan_memory,
            record_memory,
            reply_memory,
        } = prepared;
        let committed = self
            .commit_status(plan, record_memory)
            .and_then(|()| self.queue.record_processed(bytes.len()));
        if let Err(error) = committed {
            effects.0.fill(0);
            self.requeue(Command::Output(bytes, staging));
            self.fail(error);
            return WorkSchedule::Dormant;
        }
        self.journal.output(bytes, self.status().processed);
        if let Some(timing) = staging.timing.take() {
            timing.finish(true);
        }
        if !effects.0.is_empty() {
            workspace.reply = Some(Reply {
                bytes: effects.0,
                _memory: reply_memory,
                operation: None,
            });
        }
        WorkSchedule::After(Duration::ZERO)
    }
    fn feed_status_slices(
        &self,
        terminal: &mut dyn ITerminal,
        bytes: &[u8],
        queries: &[usize],
        limit: usize,
    ) -> Result<TerminalEffects, ProjectionError> {
        if queries.is_empty() {
            let mut effects = self.native_call(|| terminal.feed(bytes))?;
            if effects.0.len() > limit {
                effects.0.fill(0);
                return Err(ProjectionError::Capacity);
            }
            return Ok(effects);
        }
        // This buffer and one native reply buffer coexist. Both full capacities
        // were admitted before the first native call, not merely their final lengths.
        let mut aggregate = ReplyBytes(Vec::with_capacity(limit));
        let mut start = 0;
        for &end in queries {
            self.feed_status_slice(terminal, &bytes[start..end], &mut aggregate, limit)?;
            aggregate.append(PROGRAM_STATUS_REPLY, limit)?;
            start = end;
        }
        if start < bytes.len() {
            self.feed_status_slice(terminal, &bytes[start..], &mut aggregate, limit)?;
        }
        Ok(TerminalEffects(std::mem::take(&mut aggregate.0)))
    }
    fn feed_status_slice(
        &self,
        terminal: &mut dyn ITerminal,
        bytes: &[u8],
        aggregate: &mut ReplyBytes,
        limit: usize,
    ) -> Result<(), ProjectionError> {
        let mut effects = self.native_call(|| terminal.feed(bytes))?;
        let result = aggregate.append(&effects.0, limit);
        effects.0.fill(0);
        result
    }
}
struct ReplyBytes(Vec<u8>);
impl ReplyBytes {
    fn append(&mut self, bytes: &[u8], limit: usize) -> Result<(), ProjectionError> {
        if bytes.len() > limit.saturating_sub(self.0.len()) {
            return Err(ProjectionError::Capacity);
        }
        self.0.extend_from_slice(bytes);
        Ok(())
    }
}
impl Drop for ReplyBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}
