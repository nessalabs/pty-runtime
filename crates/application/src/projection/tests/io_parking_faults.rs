//! Reject dishonest checkpoint/protection providers before discarding live state.
use super::support::*;
use crate::{
    checkpoint::ICheckpointProtector,
    process::OutputAcceptance,
    projection::{ProjectionError, Residency},
};
use pty_runtime_domain::{checkpoint::*, terminal::*};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, Default)]
enum ProtectFault {
    #[default]
    None,
    Error,
    Key,
    Descriptor,
    Empty,
    Oversized,
}
struct FaultProtector {
    inner: Arc<Protector>,
    fault: Mutex<ProtectFault>,
    calls: AtomicUsize,
}
impl ICheckpointProtector for FaultProtector {
    fn protected_size_limit(&self, bytes: usize) -> Result<usize, CheckpointError> {
        self.inner.protected_size_limit(bytes)
    }
    fn protect(
        &self,
        key: CheckpointKey,
        checkpoint: TerminalCheckpoint,
    ) -> Result<ProtectedCheckpoint, CheckpointError> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let fault = *self.fault.lock().unwrap();
        if matches!(fault, ProtectFault::Error) {
            return Err(CheckpointError::AuthenticationFailed);
        }
        let protected = self.inner.protect(key, checkpoint)?;
        let mut key = protected.key;
        let mut descriptor = protected.descriptor.clone();
        let mut bytes = protected.ciphertext().to_vec();
        match fault {
            ProtectFault::Key => key.generation += 1,
            ProtectFault::Descriptor => descriptor.control_generation += 1,
            ProtectFault::Empty => bytes.clear(),
            // The fixture plaintext limit is 1,024; this exceeds its promised bound.
            ProtectFault::Oversized => bytes.resize(self.protected_size_limit(1024)? + 1, 0),
            _ => {}
        }
        Ok(ProtectedCheckpoint::new(key, descriptor, bytes))
    }
    fn open(
        &self,
        key: CheckpointKey,
        descriptor: &CheckpointDescriptor,
        checkpoint: ProtectedCheckpoint,
    ) -> Result<TerminalCheckpoint, CheckpointError> {
        self.inner.open(key, descriptor, checkpoint)
    }
}
fn observed_protector(h: &Harness, fault: ProtectFault) -> Arc<FaultProtector> {
    let protector = Arc::new(FaultProtector {
        inner: h.protector.clone(),
        fault: Mutex::new(fault),
        calls: AtomicUsize::new(0),
    });
    h.owner.services.lock().unwrap().as_mut().unwrap().protector = protector.clone();
    protector
}
fn retained_after_rejection(h: &Harness, expected: ProjectionError) {
    let status = h.owner.status();
    assert_eq!(status.residency, Residency::Resident);
    assert_eq!(status.failure, None);
    assert_eq!(status.parking_failure, Some(expected));
    assert_eq!(status.processed.offset, 6);
    assert_eq!(status.published.offset, 6);
    assert_eq!(h.probe.alive.load(Ordering::Acquire), 1);
    assert_eq!(h.store.commits.load(Ordering::Acquire), 0);
    assert!(h.store.entries.lock().unwrap().is_empty());
    assert_eq!(h.jobs.len(), 0);
    let usage = h.budgets.resources();
    assert_eq!(usage.native_reservations.used, 4096);
    assert_eq!(usage.checkpoint_buffers.used, 0);
    assert_eq!(usage.stored_bytes.used, 0);
    assert_eq!(usage.stored_slots.used, 0);
    assert_eq!(h.budgets.unreclaimed_sources(), 0);
}
fn retry_preserves_saved_bytes(h: &Harness) {
    h.clock.0.store(65, Ordering::Release);
    h.pump();
    assert_eq!(h.owner.status().residency, Residency::Parked);
    assert_eq!(h.owner.status().parking_failure, None);
    assert_eq!(h.store.commits.load(Ordering::Acquire), 1);
    let mut checkpoint = h.owner.checkpoint().unwrap();
    h.pump();
    let pin = result(&mut checkpoint).unwrap();
    assert_eq!(pin.checkpoint().bytes, b"before");
    assert_eq!(pin.checkpoint().descriptor.processed.offset, 6);
    assert_eq!(pin.checkpoint().descriptor.control_generation, 0);
    drop((pin, checkpoint));
    h.close();
    assert_eq!(h.store.deletes.load(Ordering::Acquire), 1);
    assert!(h.store.entries.lock().unwrap().is_empty());
    let usage = h.budgets.resources();
    for item in [
        usage.journal_bytes,
        usage.journal_slots,
        usage.transfer_observers,
        usage.staging_bytes,
        usage.staging_slots,
        usage.native_reservations,
        usage.checkpoint_buffers,
        usage.stored_bytes,
        usage.stored_slots,
        usage.views,
        usage.requests,
    ] {
        assert_eq!(item.used, 0);
    }
}

#[test]
fn parking_rejects_wrong_descriptor_or_excess_capacity_before_protection() {
    for wrong_descriptor in [true, false] {
        let h = Harness::standard();
        let protector = observed_protector(&h, ProtectFault::None);
        assert_eq!(h.owner.stage_output(b"before"), OutputAcceptance::Accepted);
        h.pump();
        h.probe
            .wrong_checkpoint_descriptor
            .store(wrong_descriptor, Ordering::Release);
        if !wrong_descriptor {
            h.probe.checkpoint_capacity.store(2048, Ordering::Release);
        }
        h.clock.0.store(60, Ordering::Release);
        h.pump();
        retained_after_rejection(
            &h,
            if wrong_descriptor {
                ProjectionError::InvalidConfiguration
            } else {
                ProjectionError::Capacity
            },
        );
        assert_eq!(protector.calls.load(Ordering::Acquire), 0);
        h.probe
            .wrong_checkpoint_descriptor
            .store(false, Ordering::Release);
        h.probe.checkpoint_capacity.store(0, Ordering::Release);
        retry_preserves_saved_bytes(&h);
        assert_eq!(protector.calls.load(Ordering::Acquire), 1);
    }
}

#[test]
fn failed_or_malformed_protection_never_publishes_and_allows_a_valid_retry() {
    for fault in [
        ProtectFault::Error,
        ProtectFault::Key,
        ProtectFault::Descriptor,
        ProtectFault::Empty,
        ProtectFault::Oversized,
    ] {
        let h = Harness::standard();
        let protector = observed_protector(&h, fault);
        assert_eq!(h.owner.stage_output(b"before"), OutputAcceptance::Accepted);
        h.pump();
        h.clock.0.store(60, Ordering::Release);
        h.pump();
        retained_after_rejection(
            &h,
            if matches!(fault, ProtectFault::Error) {
                ProjectionError::Storage(CheckpointError::AuthenticationFailed)
            } else {
                ProjectionError::InvalidConfiguration
            },
        );
        assert_eq!(protector.calls.load(Ordering::Acquire), 1, "{fault:?}");
        h.clock.0.store(64, Ordering::Release);
        h.pump();
        assert_eq!(
            protector.calls.load(Ordering::Acquire),
            1,
            "retry started early"
        );
        *protector.fault.lock().unwrap() = ProtectFault::None;
        retry_preserves_saved_bytes(&h);
        assert_eq!(protector.calls.load(Ordering::Acquire), 2);
    }
}
