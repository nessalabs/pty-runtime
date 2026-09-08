use pty_runtime::{BudgetUsage, ResourceSnapshot};
pub fn report(phase: &str, snapshot: &ResourceSnapshot, empty: bool) {
    let check = |name: &str, usage: BudgetUsage| {
        assert!(usage.used <= usage.limit, "{name}: {usage:?}");
        if empty {
            assert_eq!(usage.used, 0, "unreleased {name}");
        }
        println!(
            "{{\"event\":\"budget\",\"phase\":\"{phase}\",\"name\":\"{name}\",\"used\":{},\"limit\":{}}}",
            usage.used, usage.limit
        );
    };
    check("observers", snapshot.observers);
    check("replay_capacity", snapshot.replay_capacity);
    check("input_bytes", snapshot.input_bytes);
    check("input_slots", snapshot.input_slots);
    if let Some(p) = &snapshot.projection {
        for (name, usage) in [
            ("journal_bytes", p.journal_bytes),
            ("journal_slots", p.journal_slots),
            ("transfer_observers", p.transfer_observers),
            ("staging_bytes", p.staging_bytes),
            ("staging_slots", p.staging_slots),
            ("native_reservations", p.native_reservations),
            ("checkpoint_buffers", p.checkpoint_buffers),
            ("stored_bytes", p.stored_bytes),
            ("stored_slots", p.stored_slots),
            ("views", p.views),
            ("requests", p.requests),
        ] {
            check(name, usage);
        }
    }
}
