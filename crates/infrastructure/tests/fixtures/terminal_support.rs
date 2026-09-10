//! Shared pinned-terminal fixtures for the native terminal contract tests.
//!
//! Split out of `terminal_contract.rs` when that file passed the 350-line
//! reviewability limit; `terminal_state_contract.rs` reuses the same fixtures.
#![allow(dead_code)]
use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;

pub fn config() -> TerminalConfig {
    TerminalConfig {
        size: TerminalSize::new(80, 24).unwrap(),
        history_bytes: 1024 * 1024,
        continuation_bytes: 64 * 1024,
        reply_bytes: 4096,
        checkpoint_bytes: 8 * 1024 * 1024,
        feed_bytes: 64 * 1024,
        native_bytes: 32 * 1024 * 1024,
        view_bytes: 1024 * 1024,
    }
}
pub fn descriptor(offset: u64, generation: ControlGeneration) -> CheckpointDescriptor {
    CheckpointDescriptor {
        compatibility: CompatibilityId::new(GhosttyTerminalFactory.compatibility()).unwrap(),
        processed: ReplayCursor {
            lifetime: SessionLifetime::new(1, 1),
            offset,
        },
        control_generation: generation,
    }
}
pub fn complete(terminal: &mut dyn ITerminal) {
    for _ in 0..10_000 {
        if terminal.restore_history_step().unwrap() == RestorationProgress::Complete {
            return;
        }
    }
    panic!("history did not complete within bounded corpus page count");
}
unsafe extern "C" {
    fn rt_verify_format(
        bytes: *const u8,
        len: usize,
        out: *mut u8,
        cap: usize,
        written: *mut usize,
    ) -> i32;
}
pub fn canonical(checkpoint: TerminalCheckpoint) -> Vec<u8> {
    let mut bytes = vec![0u8; 4 * 1024 * 1024];
    let mut len = 0;
    // SAFETY: The independent C oracle borrows complete immutable checkpoint bytes
    // and writes only within this initialized buffer during the synchronous call.
    let result = unsafe {
        rt_verify_format(
            checkpoint.bytes.as_ptr(),
            checkpoint.bytes.len(),
            bytes.as_mut_ptr(),
            bytes.len(),
            &mut len,
        )
    };
    assert_eq!(result, 0);
    bytes.truncate(len);
    bytes
}
pub fn same_semantics(
    a: &mut dyn ITerminal,
    b: &mut dyn ITerminal,
    offset: u64,
    generation: ControlGeneration,
) {
    let a = canonical(a.checkpoint(descriptor(offset, generation)).unwrap());
    let b = canonical(b.checkpoint(descriptor(offset, generation)).unwrap());
    assert!(
        a == b,
        "full formatted terminal/history state differs ({} vs {} bytes)",
        a.len(),
        b.len()
    );
}
