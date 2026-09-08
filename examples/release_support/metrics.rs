use super::{DEADLINE, Harness, Result};
use pty_runtime::{BudgetUsage, CounterKind, ResourceSnapshot};
use std::time::{Duration, Instant};

impl Harness {
    pub fn checkpoint(&self, phase: &str, completed: usize) -> Result<()> {
        self.metrics(phase, true)?;
        super::checkpoint(phase, completed)
    }
    pub fn metrics(&self, phase: &str, quiescent: bool) -> Result<()> {
        let deadline = Instant::now() + DEADLINE;
        let (resources, aggregate) = loop {
            let resources = entries(self.owner.resources());
            let aggregate = self.diagnostics.aggregate();
            if !quiescent
                || (resources.iter().all(|(_, value)| value.used == 0)
                    && aggregate.active_sessions == 0
                    && aggregate.retained_replay_bytes == 0)
            {
                break (resources, aggregate);
            }
            assert!(
                Instant::now() < deadline,
                "resource cleanup did not settle: {resources:?}, {aggregate:?}"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(resources.iter().all(|(_, value)| value.used <= value.limit));
        assert_eq!(aggregate.count(CounterKind::CleanupFailed), 0);
        let budgets = resources
            .iter()
            .map(|(name, value)| {
                format!(
                    "\"{name}\":{{\"used\":{},\"limit\":{}}}",
                    value.used, value.limit
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let counters = CounterKind::ALL
            .into_iter()
            .map(|kind| format!("\"{kind:?}\":{}", aggregate.count(kind)))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"event\":\"runtime_resources\",\"phase\":\"{phase}\",\"active_sessions\":{},\"retained_replay_bytes\":{},\"budgets\":{{{budgets}}},\"counters\":{{{counters}}}}}",
            aggregate.active_sessions, aggregate.retained_replay_bytes
        );
        Ok(())
    }
}
fn entries(resources: ResourceSnapshot) -> Vec<(&'static str, BudgetUsage)> {
    let mut values = vec![
        ("observers", resources.observers),
        ("replay_capacity", resources.replay_capacity),
        ("input_bytes", resources.input_bytes),
        ("input_slots", resources.input_slots),
    ];
    if let Some(projection) = resources.projection {
        values.extend([
            ("journal_bytes", projection.journal_bytes),
            ("journal_slots", projection.journal_slots),
            ("transfer_observers", projection.transfer_observers),
            ("staging_bytes", projection.staging_bytes),
            ("staging_slots", projection.staging_slots),
            ("native_reservations", projection.native_reservations),
            ("checkpoint_buffers", projection.checkpoint_buffers),
            ("stored_bytes", projection.stored_bytes),
            ("stored_slots", projection.stored_slots),
            ("views", projection.views),
            ("requests", projection.requests),
        ]);
    }
    values
}
