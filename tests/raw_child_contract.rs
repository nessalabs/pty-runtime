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
/// failed seven times under CPU contention, once in CI on a documentation-only
/// pull request, and the first six were undiagnosable for exactly that reason.
///
/// **The child is run as `env -0`, and that is load-bearing.** Newline-delimited
/// output cannot support this inference at all: an environment value may contain
/// a newline, so a fragment of one value can be shaped exactly like an
/// assignment — `SENSITIVE_FRAGMENT=rest` — and no character filter can tell it
/// from a real name. This function used to report such a fragment's prefix as a
/// variable name, printing part of the value it exists to withhold; a base64
/// value ending in `=` does the same. NUL-delimited records are unambiguous, so
/// a name is a name.
fn redacted_environment(output: &[u8]) -> String {
    // Each record is one whole assignment, terminated by NUL, so the text before
    // the first `=` is the variable's name and nothing else can be mistaken for
    // it. A trailing partial record - the signature of a truncated read - is
    // counted by length alone and never split.
    let terminated = output.last() == Some(&0);
    let mut records: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
    let trailing = if terminated {
        records.pop();
        None
    } else {
        records.pop().map(<[u8]>::len)
    };
    let mut names: Vec<String> = Vec::new();
    let mut unnamed = 0usize;
    for record in records {
        match record.iter().position(|byte| *byte == b'=') {
            Some(0) | None => unnamed += 1,
            Some(split) => names.push(String::from_utf8_lossy(&record[..split]).into_owned()),
        }
    }
    format!(
        "{} bytes, {} assignment(s), names {:?}, {unnamed} record(s) carrying no \
         name withheld, final record NUL-terminated: {terminated}, unterminated \
         trailing bytes: {}, valid utf-8: {} (values withheld deliberately; a \
         short byte count with an unterminated final record is a truncated read, \
         an extra name is a leak)",
        output.len(),
        names.len(),
        names,
        trailing.map_or_else(|| "none".to_owned(), |count| count.to_string()),
        std::str::from_utf8(output).is_ok(),
    )
}

/// A value carrying an embedded newline must not surface as a second name.
///
/// With newline-delimited output this helper reported the prefix of such a
/// fragment as a variable, printing part of the value it promises to withhold.
/// A base64 value ending in `=` did the same. NUL-delimited records remove the
/// ambiguity rather than filtering for it.
#[test]
fn a_multiline_value_never_surfaces_as_a_variable_name() {
    let leaky = b"LEAKED_SECRET=prefix\nSENSITIVE_VALUE_FRAGMENT=rest\0";
    let report = redacted_environment(leaky);
    assert!(
        report.contains("LEAKED_SECRET"),
        "the real name is still reported: {report}"
    );
    assert!(
        !report.contains("SENSITIVE_VALUE_FRAGMENT"),
        "a fragment of the value was reported as a name: {report}"
    );
    assert!(
        !report.contains("prefix") && !report.contains("rest"),
        "no part of the value may appear: {report}"
    );
    // A base64 value ending in `=`, which the old character filter also
    // mistook for an assignment.
    let padded = b"TOKEN=aGVsbG8=\0";
    let report = redacted_environment(padded);
    assert!(report.contains("TOKEN"), "{report}");
    assert!(
        !report.contains("aGVsbG8"),
        "the value must not appear: {report}"
    );
    // A truncated read reports its trailing bytes by length, never by content.
    let cut = b"TOKEN=abc\0PARTIAL=sec";
    let report = redacted_environment(cut);
    assert!(
        report.contains("final record NUL-terminated: false"),
        "{report}"
    );
    assert!(
        !report.contains("sec"),
        "the partial value must not appear: {report}"
    );
}

#[test]
fn empty_environment_is_exact_at_uninstrumented_exec_boundary() {
    let owner = runtime(RuntimeOptions::default());
    // `-0`: NUL-delimited records. Newline-delimited output cannot be parsed
    // back into names safely, because a value may contain a newline and a
    // fragment of one can be shaped exactly like an assignment.
    let spec = CommandSpec::new(
        "/usr/bin/env".into(),
        std::env::current_dir().unwrap(),
        vec!["-0".into()],
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
    assert!(
        !output.is_empty(),
        "the child observed an empty environment, so the expected variable never \
         reached it - delivery was complete and there was nothing to deliver: {}",
        redacted_environment(&output)
    );
    // 2. Delivery was complete. `env -0` terminates every record with NUL, so
    //    output that does not end in one is a truncated read and nothing about
    //    the environment can be concluded from it.
    assert!(
        output.last() == Some(&0),
        "truncated delivery, not an environment result: {}",
        redacted_environment(&output)
    );
    // 3. Exactly one variable reached the child, and it is the expected one.
    //    Each record is a whole assignment, so the bytes before the first `=`
    //    are a name and cannot be a fragment of some other variable's value.
    //    Compared with `assert!` rather than `assert_eq!`, because the latter
    //    debug-prints both sides and one of them is the value this test exists
    //    to keep out of a CI log.
    let records: Vec<&[u8]> = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect();
    let names: Vec<&[u8]> = records
        .iter()
        .map(
            |record| match record.iter().position(|byte| *byte == b'=') {
                Some(split) => &record[..split],
                None => b"<record carrying no name>",
            },
        )
        .collect();
    assert!(
        names == [b"PTY_SYNTHETIC_FLAG".as_slice()],
        "environment leak at the exec boundary: {}",
        redacted_environment(&output)
    );
    // 4. The override won. Reaching here means the name was right, so the only
    //    thing left to report is that the value was not - and it is withheld.
    assert!(
        records == [b"PTY_SYNTHETIC_FLAG=final-override".as_slice()],
        "the expected variable is present but carries a different value, \
         which is withheld: {}",
        redacted_environment(&output)
    );
    owner.shutdown();
}
