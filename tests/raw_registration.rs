//! Public runtime admission races backed by independently counted OS launches.
mod support;
use pty_runtime::*;
use std::sync::{Arc, Barrier};
use support::*;

fn race(
    owner: &Arc<Runtime>,
    key: &SessionId,
    spec: &CommandSpec,
) -> Vec<Result<Session, RuntimeError>> {
    let barrier = Arc::new(Barrier::new(16));
    std::thread::scope(|scope| {
        let joins: Vec<_> = (0..16)
            .map(|_| {
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    owner.spawn(key.clone(), spec, options())
                })
            })
            .collect();
        joins.into_iter().map(|join| join.join().unwrap()).collect()
    })
}
fn ready(session: &Session) {
    let mut reader = session.attach(AttachPosition::Oldest).unwrap();
    let mut received = Vec::new();
    while received.len() < b"registered".len() {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => received.extend(bytes),
            other => panic!("unexpected registration output {other:?}"),
        }
    }
    assert_eq!(received, b"registered");
}
fn finish(session: &Session) {
    assert_eq!(block_on(session.write(b"finish\n").unwrap()).written, 7);
    let completion = block_on(session.wait().unwrap()).unwrap();
    assert_eq!(completion.status.exit, Some(ExitStatus::Code(23)));
    assert_eq!(completion.status.drain, Some(DrainOutcome::Eof));
}

#[test]
fn simultaneous_duplicate_calls_launch_once_even_after_completion_until_forgetting() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter =
        std::env::temp_dir().join(format!("pty-g1-launches-{}-{nonce}", std::process::id()));
    let spec = command("record-launch", &[counter.to_str().unwrap()]);
    let owner = Arc::new(runtime(RuntimeOptions::default()));
    let key = id("contended-lifetime");
    let results = race(&owner, &key, &spec);
    let mut sessions = Vec::new();
    let mut duplicate_count = 0;
    for result in results {
        match result {
            Ok(session) => sessions.push(session),
            Err(RuntimeError::ExistingSession) => duplicate_count += 1,
            Err(error) => panic!("unexpected admission outcome {error:?}"),
        }
    }
    assert_eq!(
        sessions.len(),
        1,
        "more than one caller obtained a new workload"
    );
    assert_eq!(duplicate_count, 15);
    let first = sessions.pop().unwrap();
    ready(&first);
    assert_eq!(std::fs::read(&counter).unwrap(), b"launched\n");
    assert_eq!(
        owner.lookup(&key).unwrap().process_id().unwrap(),
        first.process_id().unwrap()
    );
    finish(&first);
    assert!(
        race(&owner, &key, &spec)
            .into_iter()
            .all(|result| matches!(result, Err(RuntimeError::ExistingSession)))
    );
    assert_eq!(std::fs::read(&counter).unwrap(), b"launched\n");
    owner.forget(&key).unwrap();
    let replacement = owner.spawn(key, &spec, options()).unwrap();
    ready(&replacement);
    assert_ne!(replacement.lifetime(), first.lifetime());
    assert!(matches!(
        replacement.attach(AttachPosition::Cursor(ReplayCursor {
            lifetime: first.lifetime(),
            offset: 0
        })),
        Err(RuntimeError::InvalidCursor)
    ));
    finish(&replacement);
    owner.shutdown();
    let actual = std::fs::read(&counter).unwrap();
    std::fs::remove_file(&counter).unwrap();
    assert_eq!(actual, b"launched\nlaunched\n");
}
