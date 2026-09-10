//! Real pinned Ghostty terminal adapter; enable the `ghostty` feature to build it.
mod ffi;
mod projection;
mod state;
use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::terminal::*;
use std::{ffi::c_void, ptr::NonNull};

/// Pinned terminal factory. All native callbacks stay within its exclusive owner.
#[derive(Debug, Default)]
pub struct GhosttyTerminalFactory;
/// Binary compatibility includes the exact upstream revision and adapter version.
pub const COMPATIBILITY: &str = "ghostty-vt:82232ecde55405559dec29c5466cb9e39938cb41:snapshot-wrap-ad2d4709c7bb53d3060f03390c8f169066768af9a705219c1b7331be82f44776:runtime-1";

impl ITerminalFactory for GhosttyTerminalFactory {
    fn capabilities(&self) -> TerminalCapabilities {
        TerminalCapabilities {
            checkpoints: true,
            incremental_restore: true,
            mutation_during_restore: true,
            history_compression: true,
        }
    }
    fn compatibility(&self) -> &'static str {
        COMPATIBILITY
    }
    fn create(&self, config: TerminalConfig) -> Result<Box<dyn ITerminal>, TerminalError> {
        let config = config.validate()?;
        // SAFETY: Scalar limits are validated. The C wrapper creates one heap-stable
        // exclusive owner; allocation/reply callbacks contain no Rust and cannot unwind.
        let raw = unsafe {
            ffi::rt_new(
                config.size.cols(),
                config.size.rows(),
                config.history_bytes,
                config.continuation_bytes,
                config.native_bytes,
            )
        };
        let raw = NonNull::new(raw).ok_or(TerminalError::EngineFailure)?;
        Ok(Box::new(GhosttyTerminal {
            raw,
            config,
            generation: 0,
            restoring: None,
            skipped_pages: 0,
            failed: false,
        }))
    }
    fn restore(
        &self,
        checkpoint: TerminalCheckpoint,
        config: TerminalConfig,
    ) -> Result<Box<dyn ITerminal>, TerminalError> {
        let config = config.validate()?;
        if checkpoint.descriptor.compatibility.as_str() != COMPATIBILITY {
            return Err(TerminalError::IncompatibleCheckpoint);
        }
        if checkpoint.bytes.len() > config.checkpoint_bytes {
            return Err(TerminalError::BudgetExceeded);
        }
        let mut error = 0;
        // SAFETY: The C decoder borrows immutable checkpoint bytes, retained in the
        // returned owner until decoder completion/destruction. No Rust callbacks run.
        let raw = unsafe {
            ffi::rt_restore(
                checkpoint.bytes.as_ptr(),
                checkpoint.bytes.len(),
                config.continuation_bytes,
                config.history_bytes,
                config.native_bytes,
                &mut error,
            )
        };
        let raw = NonNull::new(raw).ok_or(if error == -2 {
            TerminalError::BudgetExceeded
        } else {
            TerminalError::CorruptCheckpoint
        })?;
        let mut terminal = GhosttyTerminal {
            raw,
            config,
            generation: checkpoint.descriptor.control_generation,
            restoring: Some(checkpoint),
            skipped_pages: 0,
            failed: false,
        };
        let info = terminal.info()?;
        if TerminalSize::new(info.cols, info.rows)? != config.size {
            return Err(TerminalError::InvalidConfiguration);
        }
        Ok(Box::new(terminal))
    }
}

/// A single terminal owner; its native resources are never shared concurrently.
pub struct GhosttyTerminal {
    raw: NonNull<c_void>,
    config: TerminalConfig,
    generation: u64,
    restoring: Option<TerminalCheckpoint>,
    skipped_pages: u64,
    failed: bool,
}
// SAFETY: No thread-local Ghostty state is used. Moving transfers the sole native
// owner and allocator context together; all operations require &mut self. Not Sync.
unsafe impl Send for GhosttyTerminal {}
impl Drop for GhosttyTerminal {
    fn drop(&mut self) {
        // SAFETY: Exactly one owner frees decoder first, then terminal; borrowed
        // checkpoint bytes remain alive until after this Drop body completes.
        unsafe { ffi::rt_free(self.raw.as_ptr()) };
    }
}
impl GhosttyTerminal {
    fn healthy(&self) -> Result<(), TerminalError> {
        if self.failed {
            Err(TerminalError::EngineFailure)
        } else {
            Ok(())
        }
    }
    fn mutation(&mut self, result: i32) -> Result<(), TerminalError> {
        if result == 0 {
            return Ok(());
        }
        self.failed = true;
        Err(if result == -2 {
            TerminalError::BudgetExceeded
        } else {
            TerminalError::EngineFailure
        })
    }
}
fn buffer(len: usize) -> Result<Vec<u8>, TerminalError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| TerminalError::BudgetExceeded)?;
    bytes.resize(len, 0);
    Ok(bytes)
}
