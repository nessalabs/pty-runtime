//! Closed public handles must not retain native resources beyond owner teardown.
#[path = "fixtures/process_support.rs"]
mod support;
use pty_runtime_application::process::IProcessBackend;
use pty_runtime_domain::{
    SessionLifetime,
    process::{ProcessError, ProcessLimits},
};
use std::sync::Arc;

fn descriptors() -> usize {
    std::fs::read_dir("/dev/fd").unwrap().count()
}

#[test]
fn retained_closed_handles_release_all_wake_descriptors() {
    let baseline = descriptors();
    let owner = support::backend(16);
    let mut retained = Vec::new();
    for index in 0..16 {
        let events = Arc::new(support::Events::default());
        let session = owner
            .spawn(
                &support::command("printf ready; sleep 60"),
                support::size(),
                SessionLifetime::new(81, index),
                ProcessLimits::default(),
                events.clone(),
            )
            .unwrap();
        events.wait(|state| state.bytes == b"ready");
        retained.push(session);
    }
    drop(owner);
    assert_eq!(
        descriptors(),
        baseline,
        "closed handles retained OS descriptors"
    );
    for session in &retained {
        assert!(matches!(
            session.write(b"closed"),
            Err(ProcessError::Closed)
        ));
        assert!(matches!(
            session.request_cancel(),
            Err(ProcessError::Closed)
        ));
    }
    drop(retained);
    assert_eq!(descriptors(), baseline);
}
