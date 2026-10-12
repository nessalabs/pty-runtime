//! Real native operations with one malformed response or error at the ABI boundary.
use super::{
    GhosttyTerminalFactory,
    faults::{ColorField, Fault, Guard, Point},
};
use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};

fn config() -> TerminalConfig {
    TerminalConfig::new(TerminalSize::new(4, 2).unwrap())
}
fn descriptor() -> CheckpointDescriptor {
    CheckpointDescriptor {
        compatibility: GhosttyTerminalFactory.compatibility().unwrap(),
        processed: ReplayCursor {
            lifetime: SessionLifetime::new(701, 1),
            offset: 1,
        },
        control_generation: ControlGeneration::INITIAL,
    }
}
fn populated() -> Box<dyn ITerminal> {
    let mut terminal = GhosttyTerminalFactory.create(config()).unwrap();
    terminal.feed(b"A").unwrap();
    assert_eq!(terminal.view().unwrap().cells[0].text, "A");
    terminal
}
fn assert_failed(terminal: &mut dyn ITerminal) {
    assert_eq!(
        terminal.feed(b"untrusted continuation"),
        Err(TerminalError::EngineFailure)
    );
    assert_eq!(terminal.view(), Err(TerminalError::EngineFailure));
    assert_eq!(terminal.history(0, 1), Err(TerminalError::EngineFailure));
    assert_eq!(
        terminal.resize(
            TerminalSize::new(5, 2).unwrap(),
            ControlGeneration::from_raw(1)
        ),
        Err(TerminalError::EngineFailure)
    );
    assert!(matches!(
        terminal.checkpoint(descriptor()),
        Err(TerminalError::EngineFailure)
    ));
    assert_eq!(
        terminal.restore_history_step(),
        Err(TerminalError::EngineFailure)
    );
    assert_eq!(
        terminal.compress_history_step(),
        Err(TerminalError::EngineFailure)
    );
}

#[test]
fn malformed_read_responses_are_rejected_without_poisoning_the_native_owner() {
    for fault in [
        Fault::Return(Point::Info, -1),
        Fault::InvalidDimensions,
        Fault::Return(Point::Rows, -1),
        Fault::Return(Point::Cell, -1),
        Fault::InvalidCodepoint,
        Fault::InvalidColor(ColorField::Foreground),
        Fault::InvalidColor(ColorField::Background),
        Fault::InvalidColor(ColorField::Underline),
        Fault::InvalidUnderline,
    ] {
        let mut terminal = populated();
        let before = terminal.view().unwrap();
        let guard = Guard::arm(fault);
        let expected = if fault == Fault::InvalidDimensions {
            TerminalError::InvalidConfiguration
        } else {
            TerminalError::EngineFailure
        };
        if fault == Fault::Return(Point::Rows, -1) {
            assert_eq!(terminal.history(0, 1), Err(expected));
        } else {
            assert_eq!(terminal.view(), Err(expected));
        }
        guard.assert_consumed();
        // Keep the consumed guard alive: another real read must not reinject it.
        assert_eq!(terminal.view().unwrap(), before);
        terminal.feed(b"B").unwrap();
        let after = terminal.view().unwrap();
        assert_eq!(after.cells[0], before.cells[0]);
        assert_eq!(after.cells[1].text, "B");
    }
}

#[test]
fn a_failed_real_cell_retry_returns_no_partial_view_and_can_be_retried() {
    let mut terminal = GhosttyTerminalFactory.create(config()).unwrap();
    let grapheme = format!("a{}", "\u{0301}".repeat(64));
    terminal.feed(grapheme.as_bytes()).unwrap();
    let before = terminal.view().unwrap();
    assert_eq!(before.cells[0].text, grapheme);
    let guard = Guard::arm(Fault::Return(Point::CellRetry, -1));
    assert_eq!(terminal.view(), Err(TerminalError::EngineFailure));
    guard.assert_consumed();
    assert_eq!(terminal.view().unwrap(), before);
    terminal.feed(b"!").unwrap();
    assert_eq!(terminal.view().unwrap().cells[1].text, "!");
}

#[test]
fn native_mutation_errors_invalidate_the_owner_even_after_the_native_call_ran() {
    for (point, code, expected) in [
        (Point::Feed, -1, TerminalError::EngineFailure),
        (Point::Resize, -1, TerminalError::EngineFailure),
        (Point::Resize, -2, TerminalError::BudgetExceeded),
        (Point::Compress, -1, TerminalError::EngineFailure),
        (Point::Compress, -2, TerminalError::BudgetExceeded),
    ] {
        let mut terminal = populated();
        let guard = Guard::arm(Fault::Return(point, code));
        let result = match point {
            Point::Feed => terminal.feed(b"B").map(|_| ()),
            Point::Resize => terminal.resize(
                TerminalSize::new(5, 2).unwrap(),
                ControlGeneration::from_raw(1),
            ),
            Point::Compress => terminal.compress_history_step().map(|_| ()),
            _ => unreachable!("mutation cases are listed above"),
        };
        assert_eq!(result, Err(expected));
        guard.assert_consumed();
        assert_failed(terminal.as_mut());
        drop(terminal);
        assert_eq!(populated().view().unwrap().cells[0].text, "A");
    }
}

#[test]
fn checkpoint_error_discards_the_result_without_invalidating_the_source() {
    let mut terminal = populated();
    let before = terminal.view().unwrap();
    let checkpoint = terminal.checkpoint(descriptor()).unwrap();
    let guard = Guard::arm(Fault::Return(Point::Checkpoint, -1));
    assert!(matches!(
        terminal.checkpoint(descriptor()),
        Err(TerminalError::EngineFailure)
    ));
    guard.assert_consumed();
    assert_eq!(terminal.view().unwrap(), before);
    assert_eq!(
        terminal.checkpoint(descriptor()).unwrap().bytes,
        checkpoint.bytes
    );
    terminal.feed(b"B").unwrap();
    assert_eq!(terminal.view().unwrap().cells[1].text, "B");
}

#[test]
fn failed_restore_metadata_returns_no_owner_and_preserves_a_reusable_checkpoint() {
    let mut source = populated();
    let before = source.view().unwrap();
    let checkpoint = source.checkpoint(descriptor()).unwrap();
    let guard = Guard::arm(Fault::Return(Point::Info, -1));
    assert!(matches!(
        GhosttyTerminalFactory.restore(checkpoint.clone(), config()),
        Err(TerminalError::EngineFailure)
    ));
    guard.assert_consumed();
    assert_eq!(source.view().unwrap(), before);
    let mut restored = GhosttyTerminalFactory
        .restore(checkpoint, config())
        .unwrap();
    assert_eq!(restored.view().unwrap(), before);
    source.feed(b"B").unwrap();
    restored.feed(b"B").unwrap();
    assert_eq!(restored.view().unwrap(), source.view().unwrap());
}

#[test]
fn unsupported_native_compression_is_recoverable() {
    let mut terminal = populated();
    let before = terminal.view().unwrap();
    let guard = Guard::arm(Fault::Return(Point::Compress, -3));
    assert_eq!(
        terminal.compress_history_step(),
        Err(TerminalError::Unsupported)
    );
    guard.assert_consumed();
    assert_eq!(terminal.view().unwrap(), before);
    assert!(
        (0..128).any(|_| !terminal.compress_history_step().unwrap()),
        "compression did not finish within the bounded small-screen corpus"
    );
    assert_eq!(terminal.view().unwrap(), before);
    terminal.feed(b"B").unwrap();
    assert_eq!(terminal.view().unwrap().cells[1].text, "B");
}

#[test]
fn native_history_budget_failure_invalidates_only_the_restored_owner() {
    let mut source = populated();
    for _ in 0..32 {
        source.feed(b"history\r\n").unwrap();
    }
    let checkpoint = source.checkpoint(descriptor()).unwrap();
    let before = source.view().unwrap();
    let mut restored = GhosttyTerminalFactory
        .restore(checkpoint, config())
        .unwrap();
    assert_eq!(restored.view().unwrap(), before);
    let guard = Guard::arm(Fault::Return(Point::History, -2));
    assert_eq!(
        restored.restore_history_step(),
        Err(TerminalError::BudgetExceeded)
    );
    guard.assert_consumed();
    assert_failed(restored.as_mut());
    drop(restored);
    assert_eq!(source.view().unwrap(), before);
    source.feed(b"healthy source").unwrap();
}

#[test]
fn incompatible_checkpoint_request_and_restoring_compression_preserve_valid_owners() {
    let mut source = populated();
    let before = source.view().unwrap();
    let mut foreign = descriptor();
    foreign.compatibility = CompatibilityId::new("foreign-engine").unwrap();
    assert!(matches!(
        source.checkpoint(foreign),
        Err(TerminalError::IncompatibleCheckpoint)
    ));
    assert_eq!(source.view().unwrap(), before);
    let checkpoint = source.checkpoint(descriptor()).unwrap();
    let mut restored = GhosttyTerminalFactory
        .restore(checkpoint, config())
        .unwrap();
    assert_eq!(
        restored.compress_history_step(),
        Err(TerminalError::HistoryIncomplete)
    );
    for _ in 0..128 {
        if restored.restore_history_step().unwrap().is_finished() {
            break;
        }
    }
    assert!(restored.restoration_progress().is_finished());
    assert_eq!(restored.view().unwrap(), before);
    assert_eq!(restored.compress_history_step(), Ok(false));
}

#[test]
fn fault_scope_is_thread_local_and_clears_on_unwind() {
    let mut terminal = populated();
    let before = terminal.view().unwrap();
    let isolated = Guard::arm(Fault::Return(Point::Info, -1));
    std::thread::spawn(|| {
        assert_eq!(populated().view().unwrap().cells[0].text, "A");
    })
    .join()
    .unwrap();
    drop(isolated);
    assert!(
        std::panic::catch_unwind(|| {
            let _guard = Guard::arm(Fault::Return(Point::Info, -1));
            panic!("cancel fault scope before its native call");
        })
        .is_err()
    );
    assert_eq!(terminal.view().unwrap(), before);
    let guard = Guard::arm(Fault::Return(Point::Info, -1));
    assert_eq!(terminal.view(), Err(TerminalError::EngineFailure));
    guard.assert_consumed();
    assert_eq!(terminal.view().unwrap(), before);
}
