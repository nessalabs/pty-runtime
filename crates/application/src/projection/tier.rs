//! Lock-order instrumentation, compiled only for this crate's tests.
//!
//! `state.rs` documents the projection's locks as three tiers acquired downward
//! and never upward. That was a claim checked by reading. This makes it a claim
//! checked by running: every lock a projection takes goes through one of a few
//! chokepoints, each of which registers its tier here, and taking a lock while
//! already holding one at the same or a deeper tier panics.
//!
//! The tracker is per-thread because that is what lock ordering is about — a
//! deadlock needs one thread holding A wanting B while another holds B wanting
//! A, and it is prevented by every thread acquiring in the same order.
//!
//! A "no inversion detected" result proves nothing on its own: if no nesting
//! ever happened, nothing was tested. The tracker therefore also records which
//! nestings it saw, so tests can assert that the documented ones actually occur.
use std::cell::RefCell;

/// Where a lock sits in the documented order. Lower is acquired first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Tier {
    /// The native workspace: exclusive ownership of the engine.
    Workspace = 1,
    /// The admission queue: accepted work and domain policy.
    Admission = 2,
    /// Leaves — wiring slots, the unreclaimed ledger, the capacity signal.
    /// Nothing may be acquired while one of these is held.
    Leaf = 3,
}

thread_local! {
    static HELD: RefCell<Vec<Tier>> = const { RefCell::new(Vec::new()) };
    static NESTINGS: RefCell<Vec<(Tier, Tier)>> = const { RefCell::new(Vec::new()) };
}

/// Released when the lock it describes is released.
pub(super) struct TierGuard(());

impl Drop for TierGuard {
    fn drop(&mut self) {
        HELD.with(|held| {
            held.borrow_mut().pop();
        });
    }
}

/// Register that this thread is about to acquire a lock at `tier`.
///
/// Panics if it already holds one at the same or a deeper tier, which is
/// exactly the inversion the documented order forbids.
pub(super) fn enter(tier: Tier) -> TierGuard {
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        if let Some(&deepest) = held.last() {
            assert!(
                tier > deepest,
                "lock order inverted: acquiring {tier:?} while holding {deepest:?}. \
                 The documented order is Workspace -> Admission -> Leaf, acquired \
                 downward and never upward; see the chart on `Admission` in state.rs.",
            );
            NESTINGS.with(|seen| seen.borrow_mut().push((deepest, tier)));
        }
        held.push(tier);
    });
    TierGuard(())
}

/// Every (outer, inner) pair this thread has observed since [`reset`].
pub(super) fn observed_nestings() -> Vec<(Tier, Tier)> {
    NESTINGS.with(|seen| seen.borrow().clone())
}

/// Forget observed nestings, so one test's evidence is not another's.
pub(super) fn reset() {
    NESTINGS.with(|seen| seen.borrow_mut().clear());
}

/// Whether this thread currently holds any tracked lock. A test that has
/// returned to the top of its own stack should see `false`.
pub(super) fn holds_none() -> bool {
    HELD.with(|held| held.borrow().is_empty())
}
