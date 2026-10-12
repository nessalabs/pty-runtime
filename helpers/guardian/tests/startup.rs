//! Reject invalid CLI admission in a fresh process, before supervision begins.
use std::{
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Invocation(Child);
impl Drop for Invocation {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn rejected(arguments: &[OsString]) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_pty-runtime-guardian"));
    command
        .args(arguments)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // These invalid-CLI tests do not exercise workload environment policy.
    // Preserve the run-owned profiler destination in instrumented invocations.
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let mut child = Invocation(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(125));
            return;
        }
        assert!(Instant::now() < deadline, "invalid invocation did not exit");
        thread::sleep(Duration::from_millis(1));
    }
}
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}
const MARKER: &str = "--pty-runtime-guardian-v1";

#[test]
fn invalid_admission_is_rejected_without_inherited_endpoints() {
    for values in [
        vec![],
        vec!["wrong-protocol"],
        vec![MARKER],
        vec![MARKER, "not-a-number"],
        vec![MARKER, "1"],
        vec![MARKER, "1", "-1"],
        vec![MARKER, "0", "1", "/bin/true"],
        vec![MARKER, "1", "86400001", "/bin/true"],
        vec![MARKER, "18446744073709551616", "1", "/bin/true"],
        vec![MARKER, "1", "0"],
        // Valid admission still requires the caller-owned descriptors.
        vec![MARKER, "1", "86400000", "/bin/true"],
    ] {
        rejected(&args(&values));
    }
    let mut non_utf8 = args(&[MARKER]);
    non_utf8.push(OsString::from_vec(vec![0xff]));
    rejected(&non_utf8);
}

#[test]
fn argument_count_and_byte_limits_reject_before_endpoint_admission() {
    let mut too_many = args(&[MARKER, "1", "0"]);
    too_many.extend((0..130).map(|_| OsString::from("x")));
    rejected(&too_many);
    let mut too_large = args(&[MARKER, "1", "0"]);
    too_large.push(OsString::from("x".repeat(65537)));
    rejected(&too_large);
}
