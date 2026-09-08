//! End-to-end raw session lifecycle and observation contracts.
mod support;
use pty_runtime::*;
use support::*;

#[test]
fn raw_suffix_gap_completion_and_completed_id_are_stable() {
    let runtime = runtime(RuntimeOptions::default());
    let mut session_options = options();
    session_options.replay_bytes = 31;
    let key = id("output");
    let session = runtime
        .spawn(key.clone(), &command("bytes", &["10000"]), session_options)
        .unwrap();
    let start = ReplayCursor {
        lifetime: session.lifetime(),
        offset: 0,
    };
    let done = block_on(session.wait().unwrap()).unwrap();
    assert_eq!(done.status.exit, Some(ExitStatus::Code(0)));
    assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
    let mut observer = session.attach(AttachPosition::Cursor(start)).unwrap();
    assert!(
        matches!(block_on(observer.read_next()).unwrap(),OutputEvent::Replay(ReplayPage::Gap {from,to}) if from.offset==0 && to.offset==9969)
    );
    assert!(
        matches!(block_on(observer.read_next()).unwrap(),OutputEvent::Replay(ReplayPage::Bytes {bytes,..}) if bytes==(9969..10000).map(|n|(n%251) as u8).collect::<Vec<_>>())
    );
    assert_eq!(
        block_on(observer.read_next()).unwrap(),
        OutputEvent::Complete(done)
    );
    assert!(matches!(
        runtime.spawn(key.clone(), &command("exit", &["0"]), options()),
        Err(RuntimeError::ExistingSession)
    ));
    runtime.forget(&key).unwrap();
    let replacement = runtime
        .spawn(key, &command("exit", &["3"]), options())
        .unwrap();
    assert_ne!(session.lifetime(), replacement.lifetime());
    assert!(matches!(
        replacement.attach(AttachPosition::Cursor(start)),
        Err(RuntimeError::InvalidCursor)
    ));
    assert_eq!(
        block_on(replacement.wait().unwrap()).unwrap().status.exit,
        Some(ExitStatus::Code(3))
    );
}

#[test]
fn detach_and_cancelled_observation_preserve_live_process_and_cursor() {
    let runtime = runtime(RuntimeOptions::default());
    let key = id("interactive");
    let session = runtime
        .spawn(key.clone(), &command("echo", &[]), options())
        .unwrap();
    let pid = session.process_id().unwrap();
    let mut observer = session.attach(AttachPosition::Oldest).unwrap();
    let ready = block_on(observer.read_next()).unwrap();
    assert!(matches!(ready,OutputEvent::Replay(ReplayPage::Bytes{bytes,..}) if bytes==b"ready"));
    let cursor = observer.cursor();
    {
        let pending = observer.read_next();
        drop(pending);
    }
    assert_eq!(cursor, observer.cursor());
    drop(observer);
    drop(session);
    let session = runtime.lookup(&key).unwrap();
    assert_eq!(session.process_id().unwrap(), pid);
    let mut observer = session.attach(AttachPosition::Cursor(cursor)).unwrap();
    let input = b"\0binary\xff\n";
    assert_eq!(block_on(session.write(input).unwrap()).written, input.len());
    let mut received = Vec::new();
    while received.len() < input.len() {
        if let OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) =
            block_on(observer.read_next()).unwrap()
        {
            received.extend(bytes);
        }
    }
    assert_eq!(received, input);
    session.cancel().unwrap();
    let wait = session.wait().unwrap();
    drop(wait);
    assert!(
        block_on(session.wait().unwrap())
            .unwrap()
            .status
            .exit
            .is_some()
    );
}

#[test]
fn synthetic_input_is_not_echoed_or_exposed_in_debug() {
    let runtime = runtime(RuntimeOptions::default());
    let spec = command("prompt", &[]);
    assert!(!format!("{spec:?}").contains("prompt"));
    let session = runtime.spawn(id("prompt"), &spec, options()).unwrap();
    let mut observer = session.attach(AttachPosition::Oldest).unwrap();
    let mut output = Vec::new();
    while !output.ends_with(b"synthetic-code: ") {
        if let OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) =
            block_on(observer.read_next()).unwrap()
        {
            output.extend(bytes);
        }
    }
    let secret = b"SYNTHETIC-PRIVATE-CODE\n";
    assert_eq!(
        block_on(session.write(secret).unwrap()).written,
        secret.len()
    );
    loop {
        match block_on(observer.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            OutputEvent::Complete(_) => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(
        !output
            .windows(secret.len() - 1)
            .any(|part| part == &secret[..secret.len() - 1])
    );
    assert!(String::from_utf8_lossy(&output).contains("accepted:22"));
}

#[test]
fn admission_bounds_and_owner_drop_preserve_explicit_completion() {
    let runtime = runtime(RuntimeOptions {
        max_sessions: 1,
        max_observers: 1,
        ..RuntimeOptions::default()
    });
    let key = id("one");
    let session = runtime
        .spawn(key.clone(), &command("hold", &[]), options())
        .unwrap();
    assert!(matches!(
        runtime.spawn(id("two"), &command("hold", &[]), options()),
        Err(RuntimeError::Capacity)
    ));
    assert_eq!(runtime.forget(&key), Err(RuntimeError::NotFinished));
    let attachment = session.attach(AttachPosition::Oldest).unwrap();
    assert!(matches!(session.wait(), Err(RuntimeError::Capacity)));
    drop(attachment);
    drop(runtime);
    assert!(
        block_on(session.wait().unwrap())
            .unwrap()
            .status
            .exit
            .is_some()
    );
}
