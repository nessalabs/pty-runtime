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
    pub fn usage(&self) -> crate::diagnostics::BudgetUsage {
        crate::diagnostics::BudgetUsage {
            used: self.used.load(Ordering::Acquire),
            limit: self.limit,
        }
    }
    pub fn acquire(&self, bytes: usize) -> bool {
        crate::atomic::try_update_usize(&self.used, Ordering::AcqRel, Ordering::Acquire, |used| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_stops_at_the_limit_and_leaves_usage_there() {
        let quota = Quota::new(4);
        assert!(quota.acquire(3));
        assert!(!quota.acquire(2));
        assert_eq!(quota.usage().used, 3);
        assert!(quota.acquire(1));
        assert_eq!(quota.usage().used, 4);
        assert!(!quota.acquire(1));
        quota.release(4);
        assert_eq!(quota.usage().used, 0);
    }
}
