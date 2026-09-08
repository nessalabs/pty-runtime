//! Explicit qualification of the default sixty-second real-clock parking threshold.
#![cfg(feature = "ghostty")]
#[allow(dead_code)]
mod support;
use pty_runtime::{terminal::TerminalConfig, *};
use std::time::{Duration, Instant};
use support::*;
#[test]
#[ignore = "explicit real-clock sixty-second parking qualification"]
fn default_policy_parks_at_sixty_seconds_and_keeps_process_alive() {
    let owner = runtime(RuntimeOptions::default());
    let config = TerminalConfig::new(TerminalSize::new(80, 24).unwrap());
    let options = SessionOptions::projected(config);
    assert_eq!(
        options.projection.unwrap().park_after,
        Duration::from_secs(60)
    );
    let session = owner
        .spawn(id("default-park"), &command("echo", &[]), options)
        .unwrap();
    let mut attachment = session.attach(AttachPosition::Oldest).unwrap();
    assert!(matches!(
        block_on(attachment.read_next()).unwrap(),
        OutputEvent::Replay(ReplayPage::Bytes { .. })
    ));
    let started = Instant::now();
    let pid = session.process_id().unwrap();
    drop(block_on(session.projected_view().unwrap()).unwrap());
    drop(attachment);
    std::thread::sleep(Duration::from_secs(55));
    assert_eq!(
        session.projection_status().unwrap().unwrap().residency,
        Residency::Resident
    );
    let deadline = started + Duration::from_secs(90);
    loop {
        let status = session.projection_status().unwrap().unwrap();
        assert_eq!(status.failure, None);
        assert_eq!(status.parking_failure, None);
        if status.residency == Residency::Parked {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "default parking failed: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_secs(59));
    assert_eq!(session.process_id().unwrap(), pid);
    assert_eq!(session.status().unwrap().exit, None);
    println!(
        "{{\"case\":\"default-parking\",\"configured_idle_seconds\":60,\"observed_seconds\":{},\"same_process\":true}}",
        elapsed.as_secs_f64()
    );
    owner.shutdown();
}
