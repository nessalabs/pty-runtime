/// Current logical admission reservation, not OS resident memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BudgetUsage {
    /// Units currently reserved, including admitted/consumer-retained work.
    pub used: usize,
    /// Configured ceiling in the same units.
    pub limit: usize,
}
/// Snapshot of shared runtime reservations. Concurrent changes are approximate;
/// take after quiescence for exact cleanup accounting. No registry scan or labels.
#[derive(Debug, Clone, Default)]
pub struct ResourceSnapshot {
    /// Raw attachments and completion waits.
    pub observers: BudgetUsage,
    /// Allocated raw replay capacity, including completed retained contexts.
    pub replay_capacity: BudgetUsage,
    /// Admitted transient input bytes.
    pub input_bytes: BudgetUsage,
    /// Admitted transient input operations.
    pub input_slots: BudgetUsage,
    /// Projection budgets, absent when no projection services are configured.
    pub projection: Option<ProjectionResources>,
}
/// Projection reservations; native requested caps are not actual RSS/PSS/footprint.
#[derive(Debug, Clone, Default)]
pub struct ProjectionResources {
    /// Retained ordered continuation payload bytes.
    pub journal_bytes: BudgetUsage,
    /// Retained ordered continuation records.
    pub journal_slots: BudgetUsage,
    /// Live or provisionally admitted state-transfer observers.
    pub transfer_observers: BudgetUsage,
    /// Lossless staged original output bytes.
    pub staging_bytes: BudgetUsage,
    /// Parser output slots, independent from controls charged to requests.
    pub staging_slots: BudgetUsage,
    /// Native allocation reservations for resident/restoring terminals.
    pub native_reservations: BudgetUsage,
    /// Encoded/decoded checkpoint buffers and retained pins.
    pub checkpoint_buffers: BudgetUsage,
    /// Ciphertext reservations including unreclaimed objects.
    pub stored_bytes: BudgetUsage,
    /// Immutable retained/unreclaimed source identities.
    pub stored_slots: BudgetUsage,
    /// Copied views and pending authoritative reply buffers.
    pub views: BudgetUsage,
    /// Queued controls, pending operations, and consumer-retained operation results.
    pub requests: BudgetUsage,
}
