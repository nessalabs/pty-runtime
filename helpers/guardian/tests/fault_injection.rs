//! Scoped external-operation failures, compiled only by Cargo test targets.
use std::{cell::Cell, io, marker::PhantomData, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Discovery,
    Anchor,
    Handoff,
}

#[derive(Clone, Copy)]
struct Armed {
    operation: Operation,
    remaining: usize,
    process: i32,
}

thread_local! {
    static ARMED: Cell<Option<Armed>> = const { Cell::new(None) };
}

#[allow(dead_code, reason = "Only the dedicated fault target arms failures")]
pub struct Failure(PhantomData<Rc<()>>);

#[allow(dead_code, reason = "Only the dedicated fault target arms failures")]
impl Failure {
    pub fn arm(operation: Operation, count: usize) -> Self {
        assert!(count > 0);
        ARMED.with(|slot| {
            assert!(
                slot.get().is_none(),
                "nested failure scopes are unsupported"
            );
            slot.set(Some(Armed {
                operation,
                remaining: count,
                // SAFETY: read-only identity; forked children cannot consume the arm.
                process: unsafe { libc::getpid() },
            }));
        });
        Self(PhantomData)
    }

    pub fn remaining(&self) -> usize {
        ARMED.with(|slot| slot.get().expect("live failure scope").remaining)
    }
}

impl Drop for Failure {
    fn drop(&mut self) {
        ARMED.with(|slot| slot.set(None));
    }
}

pub fn check(operation: Operation) -> io::Result<()> {
    ARMED.with(|slot| {
        if let Some(mut armed) = slot.get() {
            // SAFETY: no mutation of process identity or global signal state.
            if armed.process == unsafe { libc::getpid() }
                && armed.operation == operation
                && armed.remaining > 0
            {
                armed.remaining -= 1;
                slot.set(Some(armed));
                return Err(io::Error::from_raw_os_error(match operation {
                    Operation::Discovery => libc::EIO,
                    Operation::Anchor | Operation::Handoff => libc::EAGAIN,
                }));
            }
        }
        Ok(())
    })
}
