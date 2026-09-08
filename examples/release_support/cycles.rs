use super::{DEADLINE, Harness, Result, verify};
use pty_runtime::*;
use std::{
    io::{Read, Write},
    sync::Barrier,
};

pub async fn run(cycles: usize, attaches: usize) -> Result<()> {
    let harness = Harness::new()?;
    harness.checkpoint("baseline", 0)?;
    for index in 0..cycles {
        let mut child = harness.spawn(false)?;
        let seed = child.pattern_seed;
        let mut observer = child.session.attach(AttachPosition::Cursor(ReplayCursor {
            lifetime: child.session.lifetime(),
            offset: 0,
        }))?;
        child.burst()?;
        harness.finish(child, index % 2 == 1).await?;
        let (bytes, gaps) = verify(&mut observer, seed)?;
        assert_eq!(bytes + gaps, 4096);
        drop(observer);
        if index % 100 == 0 {
            harness.checkpoint("cycles", index + 1)?;
            println!("{{\"event\":\"cycles\",\"completed\":{}}}", index + 1);
        }
    }
    let child = harness.spawn(false)?;
    let identity = child.session.lifetime();
    let pid = child.session.process_id()?;
    let original = ReplayCursor {
        lifetime: identity,
        offset: 0,
    };
    for index in 0..attaches {
        let observer = child.session.attach(AttachPosition::Cursor(original))?;
        assert_eq!(observer.cursor(), original);
        assert_eq!(child.session.lifetime(), identity);
        assert_eq!(child.session.process_id()?, pid);
        drop(observer);
        if index % 10_000 == 0 {
            println!("{{\"event\":\"attaches\",\"completed\":{}}}", index + 1);
        }
    }
    let observers: Vec<_> = (0..4)
        .map(|_| child.session.attach(AttachPosition::Tail))
        .collect::<std::result::Result<_, _>>()?;
    assert!(matches!(
        child.session.attach(AttachPosition::Tail),
        Err(RuntimeError::Capacity)
    ));
    drop(observers);
    harness.finish(child, false).await?;
    harness.checkpoint("final", cycles)?;
    println!("{{\"event\":\"repetition_verified\",\"cycles\":{cycles},\"attaches\":{attaches}}}");
    Ok(())
}

pub async fn races(count: usize, mut seed: u64) -> Result<()> {
    let harness = Harness::new()?;
    for index in 0..count {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut child = harness.spawn(false)?;
        let barrier = Barrier::new(3);
        let session = child.session.clone();
        let control = &mut child.control;
        let resize = seed & 1 == 0;
        let operation = std::thread::scope(|scope| {
            let operation = scope.spawn(|| {
                barrier.wait();
                if resize {
                    session
                        .resize(TerminalSize::new(100, 30).unwrap())
                        .map(Some)
                } else {
                    session.cancel().map(|()| None)
                }
            });
            let exit = scope.spawn(|| {
                barrier.wait();
                control.write_all(b"x")
            });
            barrier.wait();
            let result = operation.join().unwrap();
            let _ = exit.join().unwrap(); // Cancellation may close the separate fixture socket first.
            result
        });
        match operation {
            Ok(Some(wait)) => {
                let outcome = tokio::time::timeout(DEADLINE, wait).await?;
                assert!(outcome.is_ok() || matches!(outcome, Err(ProcessError::Closed)));
            }
            Ok(None)
            | Err(RuntimeError::Closed)
            | Err(RuntimeError::Process(ProcessError::Closed)) => {}
            Err(error) => return Err(error.into()),
        }
        let done = tokio::time::timeout(DEADLINE, child.session.wait()?).await??;
        assert!(done.status.supervision_error.is_none(), "{done:?}");
        assert!(done.status.exit.is_some());
        harness.owner.forget(&child.id)?;
        eviction(&harness)
            .await
            .map_err(|error| std::io::Error::other(format!("eviction: {error}")))?;
        shutdown_spawn()
            .await
            .map_err(|error| std::io::Error::other(format!("shutdown-spawn: {error}")))?;
        println!(
            "{{\"event\":\"race\",\"completed\":{},\"seed\":{seed},\"resize\":{resize}}}",
            index + 1
        );
    }
    Ok(())
}
async fn eviction(harness: &Harness) -> Result<()> {
    let mut child = harness.spawn(false)?;
    let cursor = ReplayCursor {
        lifetime: child.session.lifetime(),
        offset: 0,
    };
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| -> Result<()> {
        let writer = scope.spawn(|| -> Result<()> {
            barrier.wait();
            child.control.write_all(b"b")?;
            let mut ack = [0];
            child.control.read_exact(&mut ack)?;
            assert_eq!(ack, [b'a']);
            Ok(())
        });
        barrier.wait();
        for _ in 0..32 {
            drop(child.session.attach(AttachPosition::Cursor(cursor))?);
        }
        writer.join().unwrap()?;
        Ok(())
    })?;
    let wait = child.session.wait()?;
    child.control.write_all(b"x")?;
    tokio::time::timeout(DEADLINE, wait).await??;
    let mut observer = child.session.attach(AttachPosition::Cursor(cursor))?;
    let (bytes, gaps) = verify(&mut observer, child.pattern_seed)?;
    assert_eq!(bytes + gaps, 4096);
    assert!(gaps > 0);
    harness.owner.forget(&child.id)?;
    Ok(())
}
async fn shutdown_spawn() -> Result<()> {
    let harness = Harness::new()?;
    let barrier = Barrier::new(2);
    let command = CommandSpec::new(
        "/bin/sleep".into(),
        std::env::current_dir()?,
        vec!["60".into()],
    )?;
    let result = std::thread::scope(|scope| {
        let shutdown = scope.spawn(|| {
            barrier.wait();
            harness.owner.shutdown();
        });
        barrier.wait();
        let result = harness.owner.spawn(
            SessionId::new("shutdown-race".into()).unwrap(),
            &command,
            SessionOptions::raw(TerminalSize::new(80, 24).unwrap()),
        );
        shutdown.join().unwrap();
        result
    });
    match result {
        Ok(session) => {
            assert!(session.status()?.completion().is_some());
        }
        Err(RuntimeError::Closed) => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
