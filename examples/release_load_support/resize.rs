//! Failure-only context for the fixture's existing ordered resize operation.
use super::{DEADLINE, Result};
use pty_runtime::{
    ProjectionOperation, ResizeOutcome, Runtime, RuntimeError, Session, TerminalSize,
};
use std::{
    fmt::Debug,
    io::Write,
    time::{Duration, Instant},
};

pub struct Context<'a> {
    pub runtime: &'a Runtime,
    pub session: &'a Session,
    pub producer: usize,
    pub phase: u64,
    pub probe: u64,
    pub size: TerminalSize,
    pub began: Instant,
}

pub async fn run(context: Context<'_>) -> Result<ResizeOutcome> {
    complete(
        context.session.resize_projected(context.size),
        DEADLINE,
        |stage, error| context.failure(stage, error),
    )
    .await
}

impl Context<'_> {
    fn failure(&self, stage: &str, error: &dyn Debug) {
        let elapsed = self.began.elapsed().as_nanos();
        // Capture public facts before propagation drops the population. These
        // concurrent shared reservations cannot identify a rejected local quota.
        let projection = self.session.projection_status();
        let process = self.session.status();
        let resources = self.runtime.resources();
        // Diagnostic I/O must not replace the original operation error. All Debug
        // values below are portable typed runtime facts, never command/output data.
        let _ = writeln!(
            std::io::stdout().lock(),
            "{{\"event\":\"operation_failure\",\"operation\":\"resize_projected\",\"stage\":\"{stage}\",\"phase\":{},\"producer\":{},\"probe\":{},\"cols\":{},\"rows\":{},\"phase_elapsed_ns\":{elapsed},\"clock\":\"monotonic\",\"error\":{:?},\"projection_status\":{:?},\"process_status\":{:?},\"shared_resources\":{:?}}}",
            self.phase,
            self.producer,
            self.probe,
            self.size.cols(),
            self.size.rows(),
            format!("{error:?}"),
            format!("{projection:?}"),
            format!("{process:?}"),
            format!("{resources:?}"),
        );
    }
}

async fn complete(
    admitted: std::result::Result<ProjectionOperation<ResizeOutcome>, RuntimeError>,
    deadline: Duration,
    report: impl FnOnce(&'static str, &dyn Debug),
) -> Result<ResizeOutcome> {
    let wait = match admitted {
        Ok(wait) => wait,
        Err(error) => {
            report("admission", &error);
            return Err(error.into());
        }
    };
    match tokio::time::timeout(deadline, wait).await {
        Err(error) => {
            report("timeout", &error);
            Err(error.into())
        }
        Ok(Err(error)) => {
            let error = RuntimeError::from(error);
            report("completion", &error);
            Err(error.into())
        }
        Ok(Ok(outcome)) => Ok(outcome),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pty_runtime::terminal::ControlGeneration;
    use pty_runtime::{ProcessError, ProjectionError};

    #[tokio::test]
    async fn equal_capacity_errors_keep_admission_and_completion_distinct() {
        let mut stages = Vec::new();
        let admission = complete(
            Err(RuntimeError::Projection(ProjectionError::Capacity)),
            DEADLINE,
            |stage, _| stages.push(stage),
        )
        .await
        .unwrap_err();
        let completion = complete(
            Ok(Box::pin(async { Err(ProjectionError::Capacity) })),
            DEADLINE,
            |stage, _| stages.push(stage),
        )
        .await
        .unwrap_err();
        assert_eq!(stages, ["admission", "completion"]);
        for error in [admission, completion] {
            assert!(matches!(
                error.downcast_ref::<RuntimeError>(),
                Some(RuntimeError::Projection(ProjectionError::Capacity))
            ));
        }
    }

    #[tokio::test]
    async fn pending_operation_times_out_without_reclassifying_the_error() {
        let mut stages = Vec::new();
        let error = complete(
            Ok(Box::pin(std::future::pending())),
            Duration::ZERO,
            |stage, _| stages.push(stage),
        )
        .await
        .unwrap_err();
        assert_eq!(stages, ["timeout"]);
        assert!(error.is::<tokio::time::error::Elapsed>());
    }

    #[tokio::test]
    async fn completed_os_and_model_outcomes_keep_the_existing_caller_policy() {
        let outcome = complete(
            Ok(Box::pin(async {
                Ok(ResizeOutcome {
                    generation: ControlGeneration::from_raw(7),
                    os: Err(ProcessError::Capacity),
                    model: Err(ProjectionError::Process(ProcessError::Capacity)),
                })
            })),
            DEADLINE,
            |_, _| panic!("an inner outcome must still reach the existing caller assertion"),
        )
        .await
        .unwrap();
        assert_eq!(outcome.generation, ControlGeneration::from_raw(7));
        assert_eq!(outcome.os, Err(ProcessError::Capacity));
        assert_eq!(
            outcome.model,
            Err(ProjectionError::Process(ProcessError::Capacity))
        );
    }
}
