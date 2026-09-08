//! Real process resource retirement must be independent of old public handles.
#[allow(dead_code)]
mod support;
use pty_runtime::*;
use support::*;

fn descriptors() -> usize {
    let path = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    std::fs::read_dir(path).unwrap().count()
}
#[test]
fn completed_old_handles_do_not_keep_process_wake_descriptors() {
    let before = descriptors();
    let owner = runtime(RuntimeOptions::default());
    let mut retained = Vec::new();
    for index in 0..16 {
        let key = id(&format!("fd-retirement-{index}"));
        let session = owner
            .spawn(key.clone(), &command("exit", &["0"]), options())
            .unwrap();
        let completion = block_on(session.wait().unwrap()).unwrap();
        assert_eq!(completion.status.exit, Some(ExitStatus::Code(0)));
        owner.forget(&key).unwrap();
        retained.push(session);
    }
    owner.shutdown();
    drop(owner);
    assert_eq!(
        descriptors(),
        before,
        "forgotten, completed public handles must retain facts, not closed process wake sockets"
    );
    assert_eq!(retained.len(), 16);
}
