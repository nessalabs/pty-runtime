//! Optional bounded runtime measurements. No commands, bytes or session labels are retained.
mod counters;
mod histogram;
mod reader;
mod resources;
pub use reader::ReaderAllocation;
#[cfg(test)]
mod tests;
pub use counters::{AggregateSnapshot, CounterKind};
use histogram::Histogram;
pub use histogram::LatencySnapshot;
pub use resources::{BudgetUsage, ProjectionResources, ResourceSnapshot};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// Distinct internal boundaries; dispatch is never inferred from child echo latency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum LatencyKind {
    /// Public write entry through synchronous admission, including rejected calls.
    InputAdmission,
    /// Accepted adapter input through completion of the host PTY write.
    InputDispatch,
    /// Host read completion through raw replay publication.
    RawOutput,
    /// Host read completion through ordered terminal feed and continuation publication.
    ProjectedOutput,
    /// Public resize entry through synchronous admission, including rejected calls.
    ResizeAdmission,
    /// Admitted resize through the host OS resize operation.
    ResizeDispatch,
    /// Public cancel entry through synchronous admission, including rejected calls.
    CancelAdmission,
    /// Admitted cancellation through acknowledgements of direct workload and
    /// verified root/foreground TERM signals. Includes return IPC latency, so
    /// this is a conservative dispatch bound. Disappeared/unverified targets
    /// yield no success sample; owner teardown records incomplete measurements.
    CancelDispatch,
}
impl LatencyKind {
    /// Stable ordering of all measured boundaries.
    pub const ALL: [Self; 8] = [
        Self::InputAdmission,
        Self::InputDispatch,
        Self::RawOutput,
        Self::ProjectedOutput,
        Self::ResizeAdmission,
        Self::ResizeDispatch,
        Self::CancelAdmission,
        Self::CancelDispatch,
    ];
}

/// Fixed shared histogram storage, enabled explicitly before spawning sessions.
/// Counters use relaxed atomics; concurrent snapshots are approximate. Take the
/// final snapshot after quiescence for consistent counts. Reset only at an
/// externally established quiescent measurement boundary. Histograms have 100us
/// buckets through 102.4ms plus overflow, and retain exact maximum microseconds.
/// Failed operations are counted separately and excluded from success percentiles.
/// An uninstrumented adapter yields no success samples; explicit unavailable
/// measurements are separate from failures and are not zero latency.
pub struct RuntimeDiagnostics {
    histograms: [Histogram; 8],
    counters: counters::Counters,
}
impl RuntimeDiagnostics {
    /// Allocate one fixed shared measurement object; no per-sample allocation.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            histograms: std::array::from_fn(|_| Histogram::new()),
            counters: counters::Counters::new(),
        })
    }
    /// Add a finite aggregate value without allocating or retaining labels.
    pub fn count(&self, kind: CounterKind, count: u64) {
        self.counters.add(kind, count);
    }
    /// Copy aggregate operation counters and current session/replay/reader gauges.
    pub fn aggregate(&self) -> AggregateSnapshot {
        self.counters.snapshot()
    }
    pub(crate) fn session_activity(&self, active: bool) {
        self.counters.session(active);
    }
    pub(crate) fn replay_retention(&self, before: usize, after: usize) {
        self.counters.replay(before, after);
    }
    /// Record an adapter/application boundary without retaining its payload or identity.
    pub fn record(&self, kind: LatencyKind, elapsed: Duration, success: bool) {
        self.histograms[kind as usize].record(elapsed, success);
    }
    /// Copy bounded counters for one boundary. No internal lock is acquired.
    pub fn snapshot(&self, kind: LatencyKind) -> LatencySnapshot {
        self.histograms[kind as usize].snapshot()
    }
    /// Begin a new measurement interval after all measured activity has stopped.
    /// The caller must pause producers, settle input/control operations and parser
    /// staging, and ensure there are no in-flight timing owners. This method does
    /// not pause the runtime. Concurrent recording/reset produces invalid counters
    /// and maxima; use a fresh diagnostics object/runtime when quiescence cannot
    /// be established. Reset never changes session behavior or resource admission.
    pub fn reset_quiescent(&self) {
        self.counters.reset_quiescent();
        for histogram in &self.histograms {
            histogram.reset_quiescent();
        }
    }
    /// Fixed inline counter bytes, excluding Arc header and allocator overhead.
    pub fn storage_bytes() -> usize {
        std::mem::size_of::<Self>()
    }
}

/// Internal timestamp carried by a bounded admitted operation. Drop records a
/// failed/abandoned operation unless completion was reported explicitly.
pub struct Timing {
    diagnostics: Arc<RuntimeDiagnostics>,
    kind: LatencyKind,
    started: Instant,
    finished: bool,
}
impl Timing {
    /// Start at a caller-supplied monotonic boundary (for example actual host read).
    pub fn new(diagnostics: Arc<RuntimeDiagnostics>, kind: LatencyKind, started: Instant) -> Self {
        Self {
            diagnostics,
            kind,
            started,
            finished: false,
        }
    }
    /// Mark an injected boundary uninstrumented without inventing an operation failure.
    pub fn unavailable(mut self) {
        self.diagnostics.histograms[self.kind as usize].unavailable();
        self.finished = true;
    }
    /// Record the outcome once at the actual boundary; later Drop is inert.
    pub fn finish(mut self, success: bool) {
        self.diagnostics
            .record(self.kind, self.started.elapsed(), success);
        self.finished = true;
    }
}
impl Drop for Timing {
    fn drop(&mut self) {
        if !self.finished {
            self.diagnostics
                .record(self.kind, self.started.elapsed(), false);
        }
    }
}
