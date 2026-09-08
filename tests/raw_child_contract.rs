//! Executed public-facade launch, environment, dimensions and initial-echo contract.
mod support;
use pty_runtime::*;
use std::ffi::OsString;
use support::*;

#[test]
fn explicit_environment_literal_args_canonical_cwd_and_ordered_resize_reach_real_child() {
    let cwd = std::env::current_dir().unwrap();
    let expected_home =
        std::env::var_os("HOME").expect("fixture requires inherited HOME to compare");
    assert!(
        std::env::var_os("PATH").is_some(),
        "fixture requires PATH to demonstrate removal"
    );
    let owner = runtime(RuntimeOptions::default());
    for (name, policy) in [
        ("empty", EnvironmentPolicy::Empty),
        ("inherit", EnvironmentPolicy::Inherit),
    ] {
        let args: Vec<OsString> = vec![
            "inspect-contract".into(),
            cwd.as_os_str().into(),
            "literal $HOME $(printf injected); *".into(),
            name.into(),
            expected_home.clone(),
        ];
        let spec = CommandSpec::new(
            env!("CARGO_BIN_EXE_pty-runtime-fixture").into(),
            cwd.join("."),
            args,
        )
        .unwrap()
        .with_environment(
            policy,
            vec!["PATH".into(), "PTY_SYNTHETIC_FLAG".into()],
            vec![("PTY_SYNTHETIC_FLAG".into(), "final-override".into())],
        )
        .unwrap();
        let mut settings = options();
        settings.size = TerminalSize::new(81, 25).unwrap();
        let session = owner.spawn(id(name), &spec, settings).unwrap();
        let mut reader = session.attach(AttachPosition::Oldest).unwrap();
        let mut output = Vec::new();
        while !output.ends_with(b"\n") {
            match block_on(reader.read_next()).unwrap() {
                OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
                other => panic!("child did not confirm launch contract: {other:?}"),
            }
        }
        assert_eq!(
            String::from_utf8_lossy(&output).replace('\r', ""),
            "contract-ok|size:25 81\n"
        );
        assert_eq!(
            block_on(session.resize(TerminalSize::new(97, 33).unwrap()).unwrap()),
            Ok(())
        );
        assert_eq!(
            block_on(session.write(b"RESIZE-SYNTHETIC\n").unwrap()).written,
            17
        );
        let done = loop {
            match block_on(reader.read_next()).unwrap() {
                OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
                OutputEvent::Complete(done) => break done,
                other => panic!("unexpected observer outcome {other:?}"),
            }
        };
        assert_eq!(done.status.exit, Some(ExitStatus::Code(31)));
        assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
        assert_eq!(
            String::from_utf8_lossy(&output).replace('\r', ""),
            "contract-ok|size:25 81\nresized|size:33 97\n"
        );
        // The exact output proves initial terminal echo did not expose submitted input.
    }
    owner.shutdown();
}

#[test]
fn merged_stdout_stderr_preserves_binary_bytes_through_public_observer() {
    let owner = runtime(RuntimeOptions::default());
    let session = owner
        .spawn(id("binary-merge"), &command("merged-bytes", &[]), options())
        .unwrap();
    let mut reader = session.attach(AttachPosition::Oldest).unwrap();
    let mut output = Vec::new();
    let done = loop {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            OutputEvent::Complete(done) => break done,
            other => panic!("unexpected binary observation {other:?}"),
        }
    };
    assert_eq!(output, [0, 255, b'A', 128, b'B', 0, b'C']);
    assert_eq!(done.status.exit, Some(ExitStatus::Code(0)));
    assert_eq!(done.status.drain, Some(DrainOutcome::Eof));
    owner.shutdown();
}
