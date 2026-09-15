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

/// Describe an unexpected exec environment without printing any value.
///
/// `coding_standards.md` requires environment values redacted, and this
/// assertion used to honour that by printing nothing at all — which left a
/// truncated read and a real variable leak looking identical. This test has
/// failed six times under CPU contention, once in CI on a documentation-only
/// pull request, and every one of those failures was undiagnosable for exactly
/// that reason.
///
/// Names and lengths separate the two cases and disclose no value: a leak adds
/// a name, while a truncated read shows a short byte count and an unterminated
/// final line.
fn redacted_environment(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    // A Unix environment value may contain a newline, so a line without `=` is
    // a continuation of the previous value, not a variable. Printing it would
    // put part of a multiline secret into a CI log — the exact thing this
    // function exists to prevent. Continuations are counted, never echoed, and
    // a "name" is only reported when it looks like one.
    let mut names: Vec<&str> = Vec::new();
    let mut continuations = 0usize;
    for line in text.split('\n').map(|line| line.trim_end_matches('\r')) {
        if line.is_empty() {
            continue;
        }
        match line.split_once('=') {
            Some((name, _))
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
            {
                names.push(name)
            }
            _ => continuations += 1,
        }
    }
    format!(
        "{} bytes, {} assignment(s), names {:?}, {continuations} unnamed line(s) \
         withheld, final line terminated: {}, \
         valid utf-8: {} (values withheld deliberately; a short byte count with \
         an unterminated line is a truncated read, an extra name is a leak)",
        output.len(),
        names.len(),
        names,
        text.ends_with('\n'),
        std::str::from_utf8(output).is_ok(),
    )
}

#[test]
fn empty_environment_is_exact_at_uninstrumented_exec_boundary() {
    let owner = runtime(RuntimeOptions::default());
    let spec = CommandSpec::new(
        "/usr/bin/env".into(),
        std::env::current_dir().unwrap(),
        vec![],
    )
    .unwrap()
    .with_environment(
        EnvironmentPolicy::Empty,
        vec!["PATH".into(), "PTY_SYNTHETIC_FLAG".into()],
        vec![("PTY_SYNTHETIC_FLAG".into(), "final-override".into())],
    )
    .unwrap();
    let session = owner
        .spawn(id("exact-environment"), &spec, options())
        .unwrap();
    let mut reader = session.attach(AttachPosition::Oldest).unwrap();
    let mut output = Vec::new();
    let done = loop {
        match block_on(reader.read_next()).unwrap() {
            OutputEvent::Replay(ReplayPage::Bytes { bytes, .. }) => output.extend(bytes),
            OutputEvent::Complete(done) => break done,
            other => panic!("unexpected env observation {other:?}"),
        }
    };
    assert_eq!(done.status.exit, Some(ExitStatus::Code(0)));
    assert_eq!(done.status.drain, Some(DrainOutcome::Eof));

    // Four separate claims, asserted separately, because one combined byte
    // comparison could fail for four unrelated reasons and said which only by
    // being absent. Six failures under contention were undiagnosable for that
    // reason, one of them in CI on a documentation-only pull request.
    //
    // 1. The child saw an environment at all. Nothing is *not* a short read:
    //    `env` prints nothing and exits 0 when the environment is empty, so
    //    zero bytes is a complete and correct report of an empty environment -
    //    which is this test's failure, not its instrument's. Reporting it as a
    //    truncated read sent the seventh investigation of this test after a
    //    lost-output theory for hours; the two are told apart here instead.
    let text = String::from_utf8_lossy(&output);
    assert!(
        !output.is_empty(),
        "the child observed an empty environment, so the expected variable never \
         reached it - delivery was complete and there was nothing to deliver: {}",
        redacted_environment(&output)
    );
    // 2. Delivery was complete. `env` terminates every assignment, so output
    //    that does not end in a newline is a truncated read and nothing about
    //    the environment can be concluded from it.
    assert!(
        text.ends_with('\n'),
        "truncated delivery, not an environment result: {}",
        redacted_environment(&output)
    );
    // 3. Exactly one variable reached the child. A leak adds a name here, and
    //    the names are safe to report where the values are not.
    let assignments: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    //    A value may itself contain a newline, so a line without `=` is a
    //    continuation of the previous value rather than a name. Taking the text
    //    before `=` from one of those yielded the value fragment itself, which
    //    `assert_eq!` would then debug-print into a CI log — the leak this
    //    redaction exists to prevent, reintroduced by the assertion. Such lines
    //    are counted under a placeholder, never carried, and the comparison uses
    //    `assert!` so nothing is printed but the redacted report.
    let names: Vec<&str> = assignments
        .iter()
        .map(|line| {
            line.split_once('=')
                .map_or("<continuation of a withheld value>", |(name, _)| name)
        })
        .collect();
    assert!(
        names == ["PTY_SYNTHETIC_FLAG"],
        "environment leak at the exec boundary: {}",
        redacted_environment(&output)
    );
    // 4. The override won. Compared as a parsed assignment rather than as raw
    //    bytes, so a line-ending difference is not mistaken for a leak — and
    //    with `assert!` rather than `assert_eq!`, because the latter debug-
    //    prints both sides on failure and the left side is the value this test
    //    exists to keep out of a CI log. Reaching here means the name was
    //    right, so the only thing left to report is that the value was not.
    assert!(
        assignments == ["PTY_SYNTHETIC_FLAG=final-override"],
        "the expected variable is present but carries a different value, \
         which is withheld: {}",
        redacted_environment(&output)
    );
    owner.shutdown();
}
