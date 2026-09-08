use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct Quota {
    limit: usize,
    used: AtomicUsize,
}
impl Quota {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
        }
    }
    pub fn acquire(&self, bytes: usize) -> bool {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.limit)
            })
            .is_ok()
    }
    pub fn release(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::AcqRel);
    }
}

pub(crate) struct InputLease {
    bytes: std::sync::Arc<Quota>,
    slots: std::sync::Arc<Quota>,
    count: usize,
}
impl InputLease {
    pub fn acquire(
        bytes: std::sync::Arc<Quota>,
        slots: std::sync::Arc<Quota>,
        count: usize,
    ) -> Result<Self, super::RuntimeError> {
        if !slots.acquire(1) {
            return Err(super::RuntimeError::Capacity);
        }
        if !bytes.acquire(count) {
            slots.release(1);
            return Err(super::RuntimeError::Capacity);
        }
        Ok(Self {
            bytes,
            slots,
            count,
        })
    }
}
impl crate::process::IInputReservation for InputLease {}
impl Drop for InputLease {
    fn drop(&mut self) {
        self.bytes.release(self.count);
        self.slots.release(1);
    }
}
