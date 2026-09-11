use super::{
    ProjectionBudgets, ProjectionError,
    budgets::Lease,
    transfer::{TransferEvent, TransferObserver, TransferRead},
};
use crate::runtime::quota::Quota;
use pty_runtime_domain::{
    ReplayCursor, SessionLifetime,
    projection::{
        ProjectionOptions, TransferBoundary, TransferCursor, TransferError, TransferOrder,
    },
    terminal::{ControlGeneration, TerminalSize},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    task::Waker,
};

pub(super) enum RecordKind {
    Output(Vec<u8>),
    Resize {
        size: TerminalSize,
        generation: ControlGeneration,
    },
}
pub(super) struct Record {
    pub after: TransferBoundary,
    pub kind: RecordKind,
    _bytes: Lease,
    _slot: Lease,
}
impl Drop for Record {
    fn drop(&mut self) {
        if let RecordKind::Output(bytes) = &mut self.kind {
            bytes.fill(0);
        }
    }
}
struct State {
    order: TransferOrder,
    records: BTreeMap<u64, Arc<Record>>,
    observers: BTreeMap<u64, Option<Waker>>,
    next_observer: u64,
}
pub(super) struct Journal {
    state: Mutex<State>,
    bytes: Arc<Quota>,
    slots: Arc<Quota>,
    observers: Arc<Quota>,
    global_bytes: Arc<Quota>,
    global_slots: Arc<Quota>,
    global_observers: Arc<Quota>,
}
impl Journal {
    pub fn new(
        lifetime: SessionLifetime,
        options: ProjectionOptions,
        budgets: &ProjectionBudgets,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                order: TransferOrder::new(lifetime),
                records: BTreeMap::new(),
                observers: BTreeMap::new(),
                next_observer: 0,
            }),
            bytes: Arc::new(Quota::new(options.journal_bytes)),
            slots: Arc::new(Quota::new(options.journal_slots)),
            observers: Arc::new(Quota::new(options.transfer_observers)),
            global_bytes: budgets.journal_bytes.clone(),
            global_slots: budgets.journal_slots.clone(),
            global_observers: budgets.transfer_observers.clone(),
        })
    }
    pub fn reserve_observer(&self) -> Result<Lease, ProjectionError> {
        Lease::shared_and_local(self.global_observers.clone(), self.observers.clone(), 1)
    }
    pub fn open(self: &Arc<Self>, permit: Lease) -> Result<TransferObserver, ProjectionError> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let start = state.order.open_boundary().map_err(|error| match error {
            TransferError::Closed => ProjectionError::Closed,
            _ => ProjectionError::Capacity,
        })?;
        let id = state.next_observer;
        state.next_observer = id.checked_add(1).ok_or(ProjectionError::Capacity)?;
        state.observers.insert(id, None);
        Ok(TransferObserver {
            journal: self.clone(),
            id,
            start,
            _permit: permit,
        })
    }
    pub fn read(
        &self,
        id: u64,
        start: TransferCursor,
        cursor: TransferCursor,
        waker: Option<&Waker>,
    ) -> Result<TransferRead, TransferError> {
        // RawWaker clone/drop may execute arbitrary caller code. Neither occurs
        // under the journal mutex, including failed/foreign cursor reads.
        let mut next_waker = waker.cloned();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let old_waker = state.observers.get_mut(&id).and_then(Option::take);
        let result = (|| {
            if !state.observers.contains_key(&id) {
                return Err(TransferError::Closed);
            }
            state.order.validate(cursor)?;
            if cursor.sequence < start.sequence {
                return Err(TransferError::ResyncRequired { oldest: start });
            }
            let result = if let Some(record) = state.records.get(&cursor.sequence) {
                TransferRead::Event(TransferEvent(record.clone()))
            } else if cursor == state.order.boundary().cursor {
                state
                    .order
                    .end()
                    .map_or(TransferRead::Pending, TransferRead::End)
            } else {
                return Err(TransferError::ResyncRequired {
                    oldest: state.order.boundary().cursor,
                });
            };
            if matches!(result, TransferRead::Pending) {
                if let Some(slot) = state.observers.get_mut(&id) {
                    *slot = next_waker.take();
                }
            }
            Ok(result)
        })();
        drop(state);
        drop(old_waker);
        drop(next_waker);
        result
    }

    pub fn cancel_wait(&self, id: u64) {
        let old = self
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .observers
            .get_mut(&id)
            .and_then(Option::take);
        drop(old);
    }
    pub fn remove(&self, id: u64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let removed = state.observers.remove(&id);
        if state.observers.is_empty() {
            state.records.clear();
            let tail = state.order.boundary().cursor;
            let _ = state.order.discard_before(tail);
        }
        drop(state);
        drop(removed);
    }
    pub fn output(&self, bytes: Vec<u8>, processed: ReplayCursor) {
        self.append(RecordKind::Output(bytes), Some(processed));
    }
    pub fn resized(&self, size: TerminalSize, generation: ControlGeneration) {
        self.append(RecordKind::Resize { size, generation }, None);
    }
    fn append(&self, kind: RecordKind, processed: Option<ReplayCursor>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.order.accepts_events() {
            return;
        }
        let after = match &kind {
            RecordKind::Output(_) => processed
                .ok_or(TransferError::Unavailable)
                .and_then(|cursor| state.order.output(cursor)),
            RecordKind::Resize { generation, .. } => state.order.resized(*generation),
        };
        match after {
            Err(_) => {
                state.order.invalidate();
                state.records.clear();
            }
            Ok(after) => {
                if state.observers.is_empty() {
                    let _ = state.order.discard_before(after.cursor);
                } else {
                    let count = match &kind {
                        RecordKind::Output(bytes) => bytes.capacity(),
                        _ => 0,
                    };
                    let leases = loop {
                        let leases = Lease::shared_and_local(
                            self.global_slots.clone(),
                            self.slots.clone(),
                            1,
                        )
                        .and_then(|slot| {
                            Lease::shared_and_local(
                                self.global_bytes.clone(),
                                self.bytes.clone(),
                                count,
                            )
                            .map(|bytes| (slot, bytes))
                        });
                        if leases.is_ok() {
                            break leases;
                        }
                        let Some((_, evicted)) = state.records.pop_first() else {
                            break leases;
                        };
                        let _ = state.order.discard_before(evicted.after.cursor);
                        drop(evicted);
                    };
                    if let Ok((slot, bytes)) = leases {
                        state.records.insert(
                            after.cursor.sequence - 1,
                            Arc::new(Record {
                                after,
                                kind,
                                _bytes: bytes,
                                _slot: slot,
                            }),
                        );
                    } else {
                        // Even a zero-byte resize hole invalidates the previous cursor.
                        state.records.clear();
                        let _ = state.order.discard_before(after.cursor);
                    }
                }
            }
        }
        let wakes = Self::take_wakers(&mut state);
        drop(state);
        Self::wake(wakes);
    }
    pub fn end(
        &self,
        drain: Option<pty_runtime_domain::process::DrainOutcome>,
        failure: Option<ProjectionError>,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let _ = state.order.seal(drain, failure);
        let wakes = Self::take_wakers(&mut state);
        drop(state);
        Self::wake(wakes);
    }
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.order.close();
        state.records.clear();
        let wakes = Self::take_wakers(&mut state);
        drop(state);
        Self::wake(wakes);
    }
    fn take_wakers(state: &mut State) -> Vec<Waker> {
        state
            .observers
            .values_mut()
            .filter_map(Option::take)
            .collect()
    }
    fn wake(wakers: Vec<Waker>) {
        for waker in wakers {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| waker.wake()));
        }
    }
}
