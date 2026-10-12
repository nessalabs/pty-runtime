use super::{
    MAX_SCAN_BYTES, ProgramState, ProgramStatusError, ProgramStatusRecord, report::Mutation,
    scanner::Scanner,
};
use std::{fmt, mem::size_of, sync::Arc};

/// Owned bounded snapshot, ordered from least to most recently updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatusSnapshot {
    /// Changes to records; parser-only continuation does not increment this value.
    pub revision: u64,
    /// Stored reports with ancestor applications resolved at snapshot time.
    pub records: Vec<ProgramStatusRecord>,
}

impl ProgramStatusSnapshot {
    /// Owned allocation bytes, excluding allocator metadata and the struct itself.
    pub fn retained_bytes(&self) -> usize {
        self.records.capacity() * size_of::<ProgramStatusRecord>()
            + self
                .records
                .iter()
                .map(ProgramStatusRecord::heap_bytes)
                .sum::<usize>()
    }
}
/// A scan's bounded continuation, ordered mutations and query endpoints.
/// Dropping this plan cancels it without changing the store. It is intentionally not Clone.
pub struct ProgramStatusPlan {
    identity: Arc<()>,
    generation: u64,
    scanner: Scanner,
    mutations: Vec<Mutation>,
    queries: Vec<usize>,
}
impl fmt::Debug for ProgramStatusPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgramStatusPlan")
            .field("mutations", &self.mutations.len())
            .field("queries", &self.queries.len())
            .finish()
    }
}
impl ProgramStatusPlan {
    /// Byte offsets immediately after completed detection queries in this scan's input.
    pub fn query_offsets(&self) -> &[usize] {
        &self.queries
    }
    /// Owned heap bytes, excluding allocator metadata and the shared store identity.
    pub fn retained_bytes(&self) -> usize {
        self.scanner.retained_bytes()
            + self.queries.capacity() * size_of::<usize>()
            + self.mutations.capacity() * size_of::<Mutation>()
            + self
                .mutations
                .iter()
                .map(|m| match m {
                    Mutation::Report(r) => size_of::<ProgramStatusRecord>() + r.heap_bytes(),
                    Mutation::Clear(s) => s.capacity(),
                    _ => 0,
                })
                .sum::<usize>()
    }
}
/// A session's scanner and record set. Allocations are lazy; records never expire by time.
/// Each committed chunk invalidates previously scanned plans, even without record changes.
pub struct ProgramStatusStore {
    identity: Arc<()>,
    generation: u64,
    scanner: Scanner,
    records: Vec<ProgramStatusRecord>,
    revision: u64,
    limit: usize,
}
impl fmt::Debug for ProgramStatusStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgramStatusStore")
            .field("records", &self.records.len())
            .field("revision", &self.revision)
            .finish()
    }
}
impl Default for ProgramStatusStore {
    fn default() -> Self {
        Self {
            identity: Arc::new(()),
            generation: 0,
            scanner: Scanner::default(),
            records: Vec::new(),
            revision: 0,
            limit: 256,
        }
    }
}
impl ProgramStatusStore {
    /// Configure a record cap in 64..=256; reject other caps without allocating a record array.
    pub fn new(record_limit: usize) -> Result<Self, ProgramStatusError> {
        if !(64..=256).contains(&record_limit) {
            return Err(ProgramStatusError::InvalidRecordLimit);
        }
        Ok(Self {
            limit: record_limit,
            ..Self::default()
        })
    }
    /// Conservative heap reservation for retained records or an owned snapshot at this cap.
    /// Includes vector slack and resolved app copies, excluding allocator metadata.
    pub fn record_reservation(&self) -> usize {
        8192 + self.limit * (2 * size_of::<ProgramStatusRecord>() + 128 + 32 + 32 + 2048 + 192)
    }
    /// Conservative bound to admit before scanning; rejects inputs over MAX_SCAN_BYTES.
    /// Includes continuation and vector growth slack; temporary decoding fits within this bound.
    pub fn scan_reservation(chunk_len: usize) -> Result<usize, ProgramStatusError> {
        if chunk_len > MAX_SCAN_BYTES {
            return Err(ProgramStatusError::ChunkTooLarge);
        }
        Ok(32768 + chunk_len * (2 * size_of::<Mutation>() + 2 * size_of::<usize>() + 4))
    }
    /// Bound a scan against this committed continuation before allocating its plan.
    /// Ordinary text in ground state requires no heap allocation. Other input uses
    /// the conservative static bound; a plan never clones retained records.
    pub fn scan_reservation_for(&self, chunk: &[u8]) -> Result<usize, ProgramStatusError> {
        if chunk.len() > MAX_SCAN_BYTES {
            return Err(ProgramStatusError::ChunkTooLarge);
        }
        if self.scanner.plain_chunk(chunk) {
            Ok(0)
        } else {
            Self::scan_reservation(chunk.len())
        }
    }
    /// Owned heap bytes, excluding allocator metadata, store struct and shared identity.
    pub fn retained_bytes(&self) -> usize {
        self.scanner.retained_bytes()
            + self.records.capacity() * size_of::<ProgramStatusRecord>()
            + self
                .records
                .iter()
                .map(ProgramStatusRecord::heap_bytes)
                .sum::<usize>()
    }
    /// Scan without moving committed parser state or cloning existing records.
    /// Call commit only after all input bytes have been accepted by the terminal model.
    pub fn scan(&self, chunk: &[u8]) -> Result<ProgramStatusPlan, ProgramStatusError> {
        Self::scan_reservation(chunk.len())?;
        if self.generation == u64::MAX || self.revision > u64::MAX - (chunk.len() as u64 + 1) {
            return Err(ProgramStatusError::RevisionExhausted);
        }
        let mut plan = ProgramStatusPlan {
            identity: Arc::clone(&self.identity),
            generation: self.generation,
            scanner: self.scanner.clone(),
            mutations: Vec::new(),
            queries: Vec::new(),
        };
        plan.scanner
            .scan(chunk, &mut plan.mutations, &mut plan.queries);
        Ok(plan)
    }
    /// Apply an accepted chunk atomically. Wrong-store and stale plans change nothing.
    pub fn commit(&mut self, plan: ProgramStatusPlan) -> Result<(), ProgramStatusError> {
        if !Arc::ptr_eq(&self.identity, &plan.identity) || self.generation != plan.generation {
            return Err(ProgramStatusError::StalePlan);
        }
        for mutation in plan.mutations {
            self.apply(mutation)
        }
        self.scanner = plan.scanner;
        self.generation += 1;
        Ok(())
    }
    /// Drop working/blocked records after ordered process exit and again after final drain.
    /// Invalidates pending plans. Counter exhaustion leaves all state untouched.
    pub fn process_exited(&mut self) -> Result<(), ProgramStatusError> {
        if self.generation == u64::MAX || self.revision == u64::MAX {
            return Err(ProgramStatusError::RevisionExhausted);
        }
        self.apply(Mutation::Exit);
        self.generation += 1;
        Ok(())
    }
    /// Bound the next owned snapshot before allocating it, including inherited app copies.
    /// Empty snapshots require no heap reservation; no records are copied here.
    pub fn snapshot_reservation(&self) -> usize {
        self.records.len() * (size_of::<ProgramStatusRecord>() + 32)
            + self
                .records
                .iter()
                .map(ProgramStatusRecord::heap_bytes)
                .sum::<usize>()
    }
    /// Copy at most the configured record cap; callers must admit snapshot memory first.
    pub fn records(&self) -> ProgramStatusSnapshot {
        let mut records = self.records.clone();
        for record in &mut records {
            let mut ancestor = record.id.as_str();
            record.resolved_app = loop {
                if let Some(app) = self
                    .records
                    .iter()
                    .find(|r| r.id == ancestor)
                    .and_then(|r| r.app.as_ref())
                {
                    break Some(app.clone());
                }
                if ancestor.is_empty() {
                    break None;
                }
                ancestor = ancestor.rsplit_once('/').map_or("", |(parent, _)| parent);
            };
        }
        ProgramStatusSnapshot {
            revision: self.revision,
            records,
        }
    }
    fn apply(&mut self, mutation: Mutation) {
        let before = self.records.len();
        match mutation {
            Mutation::Report(record) => {
                if let Some(index) = self.records.iter().position(|r| r.id == record.id) {
                    self.records.remove(index);
                } else if self.records.len() == self.limit {
                    self.records.remove(0);
                }
                self.records.push(*record);
                self.revision += 1;
                return;
            }
            Mutation::Clear(id) => self.records.retain(|r| {
                !id.is_empty()
                    && r.id != id
                    && !r
                        .id
                        .strip_prefix(&id)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }),
            Mutation::Reset => self.records.clear(),
            Mutation::Exit => self
                .records
                .retain(|r| !matches!(r.state, ProgramState::Working | ProgramState::Blocked)),
        }
        if self.records.len() != before {
            self.revision += 1
        }
    }
}

#[cfg(test)]
mod counter_boundaries {
    use super::*;

    fn active_store() -> ProgramStatusStore {
        let mut store = ProgramStatusStore::default();
        let plan = store
            .scan(b"\x1b]7501;state=working:id=active\x07\x1b]7501;state=")
            .unwrap();
        store.commit(plan).unwrap();
        store
    }

    #[test]
    fn exhausted_generation_rejects_scan_and_exit_without_mutation() {
        let mut store = active_store();
        // Reach the arithmetic boundary locally; no production constructor or
        // configurable counter is added to expose an impossible setup publicly.
        store.generation = u64::MAX;
        let before = store.records();
        let retained = store.retained_bytes();
        assert!(matches!(
            store.scan(b"done\x07"),
            Err(ProgramStatusError::RevisionExhausted)
        ));
        assert_eq!(
            store.process_exited(),
            Err(ProgramStatusError::RevisionExhausted)
        );
        assert_eq!(store.records(), before);
        assert_eq!(store.retained_bytes(), retained);
        assert_eq!(store.generation, u64::MAX);
    }

    #[test]
    fn exhausted_revision_rejects_mutations_and_preserves_pending_continuation() {
        let mut store = active_store();
        store.revision = u64::MAX;
        let before = store.records();
        let generation = store.generation;
        assert!(matches!(
            store.scan(b"error\x07"),
            Err(ProgramStatusError::RevisionExhausted)
        ));
        assert_eq!(
            store.process_exited(),
            Err(ProgramStatusError::RevisionExhausted)
        );
        assert_eq!(store.records(), before);
        assert_eq!(store.generation, generation);

        // Restore only the test's synthetic counter. Completing the original
        // partial report proves the rejected scan did not consume its suffix.
        store.revision = before.revision - 100;
        let plan = store.scan(b"done\x07").unwrap();
        store.commit(plan).unwrap();
        let snapshot = store.records();
        assert_eq!(snapshot.records.len(), 2);
        assert_eq!(snapshot.records[1].state, ProgramState::Done);
        assert_eq!(snapshot.revision, u64::MAX - 99);
    }
}
