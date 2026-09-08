//! Public replay completion preserves actual exit separately from OS drain behavior.
mod support;
use pty_runtime::*;
use std::time::Duration;
use support::*;

fn run(owner: &Runtime, key: &str, script: &str) -> (Vec<u8>, Completion) {
    let spec = CommandSpec::new(
        "/bin/sh".into(),
        std::env::current_dir().unwrap(),
        vec!["-c".into(), script.into()],
    )
    .unwrap();
    let mut settings = options();
    settings.process.drain_timeout = Duration::from_millis(40);
    let session = owner.spawn(id(key), &spec, settings).unwrap();
    let mut reader = session.attach(AttachPosition::Oldest).unwrap();
    let mut bytes = Vec::new();
    loop {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes: chunk, .. }) => bytes.extend(chunk),
            OutputEvent::Complete(done) => {
                assert_eq!(block_on(session.wait().unwrap()).unwrap(), done);
                assert_eq!(
                    block_on(reader.read_next()).unwrap(),
                    OutputEvent::Complete(done)
                );
                return (bytes, done);
            }
            other => panic!("unexpected completion stream event {other:?}"),
        }
    }
}

#[test]
fn signal_death_and_descendant_drain_never_become_fabricated_success() {
    let owner = runtime(RuntimeOptions::default());
    let nonzero = owner
        .spawn(id("ordinary-nonzero"), &command("exit", &["19"]), options())
        .unwrap();
    let done = block_on(nonzero.wait().unwrap()).unwrap();
    assert_eq!(done.status.exit, Some(ExitStatus::Code(19)));
    assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
    let (bytes, killed) = run(&owner, "signal", "printf before-signal; kill -TERM $$");
    assert_eq!(bytes, b"before-signal");
    assert_eq!(killed.status.exit, Some(ExitStatus::Signal(15)));
    assert_eq!(killed.status.drain, Some(DrainOutcome::Eof));
    let (bytes, descendant) = run(
        &owner,
        "descendant",
        "trap '' HUP; sleep 1 & printf final; exit 23",
    );
    assert_eq!(bytes, b"final");
    assert_eq!(descendant.status.exit, Some(ExitStatus::Code(23)));
    #[cfg(target_os = "linux")]
    assert_eq!(descendant.status.drain, Some(DrainOutcome::Truncated));
    #[cfg(target_os = "macos")]
    assert_eq!(descendant.status.drain, Some(DrainOutcome::Eof));
    // macOS hangs up this PTY on session-leader exit; Linux waits until the drain
    // deadline. Neither behavior changes the already-reaped nonzero exit status.
    owner.shutdown();
}
