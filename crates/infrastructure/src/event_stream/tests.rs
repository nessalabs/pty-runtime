use super::{PublicationError, codec, decode_record, status};
use ::event_stream::{
    Cursor, EventId, IncarnationId, NewEvent, Payload, Record, StreamId, StreamKey,
};
use pty_runtime_domain::{
    ReplayCursor, ReplayPage, SessionLifetime,
    checkpoint::CheckpointError,
    process::{DrainOutcome, ExitStatus, ProcessError},
    projection::ProjectionError,
    session::{OutputEvent, RuntimeError, SessionStatus},
    terminal::TerminalError,
};
fn cursor(offset: u64) -> ReplayCursor {
    ReplayCursor {
        lifetime: SessionLifetime::new(91, 72),
        offset,
    }
}
fn record(payload: Payload) -> Record {
    Record {
        cursor: Cursor::new(
            StreamKey {
                id: StreamId::new("test").unwrap(),
                incarnation: IncarnationId([1; 16]),
            },
            1,
        ),
        event: NewEvent {
            id: EventId::new("test").unwrap(),
            schema: codec::schema().unwrap(),
            payload,
        },
    }
}
#[test]
fn maximum_binary_page_roundtrips_without_payload_diagnostics_and_rejects_corruption() {
    let bytes: Vec<u8> = (0..codec::MAX_PAGE).map(|i| (i % 251) as u8).collect();
    let event = OutputEvent::Replay(ReplayPage::Bytes {
        from: cursor(13),
        next: cursor(13 + bytes.len() as u64),
        bytes,
    });
    let (payload, next, complete) = codec::encode(cursor(13), &event).unwrap();
    assert!(!complete);
    let envelope = record(payload);
    let decoded = decode_record(&envelope).unwrap();
    assert_eq!(decoded.output, event);
    assert_eq!(decoded.byte_cursor, next);
    assert!(format!("{decoded:?}").len() < 400);
    let mut invalid = envelope.clone();
    invalid.event.schema.version += 1;
    assert_eq!(
        decode_record(&invalid),
        Err(PublicationError::InvalidRecord)
    );
    for length in [0, 1, 4, 5, 36, 37, 38, envelope.event.payload.len() - 1] {
        invalid = envelope.clone();
        invalid.event.payload =
            Payload::copy_from_slice(&envelope.event.payload.as_bytes()[..length]);
        assert_eq!(
            decode_record(&invalid),
            Err(PublicationError::InvalidRecord)
        );
    }
    let mut trailing = envelope.event.payload.as_bytes().to_vec();
    trailing.push(0);
    invalid = envelope.clone();
    invalid.event.payload = Payload::copy_from_slice(&trailing);
    assert_eq!(
        decode_record(&invalid),
        Err(PublicationError::InvalidRecord)
    );
    invalid.event.payload = Payload::copy_from_slice(&vec![0; codec::MAX_PAGE + 65]);
    assert_eq!(
        decode_record(&invalid),
        Err(PublicationError::InvalidRecord)
    );
}
#[test]
fn completion_preserves_actual_exit_and_all_portable_failure_categories() {
    let process = [
        ProcessError::InvalidCommand,
        ProcessError::OutsideRoots,
        ProcessError::Capacity,
        ProcessError::Closed,
        ProcessError::PermissionDenied,
        ProcessError::NotFound,
        ProcessError::Timeout,
        ProcessError::Unsupported,
        ProcessError::Io,
        ProcessError::Internal,
    ];
    let terminal = [
        TerminalError::InvalidConfiguration,
        TerminalError::BudgetExceeded,
        TerminalError::EngineFailure,
        TerminalError::CorruptCheckpoint,
        TerminalError::IncompatibleCheckpoint,
        TerminalError::StaleControl,
        TerminalError::HistoryIncomplete,
        TerminalError::Unsupported,
    ];
    let checkpoint = [
        CheckpointError::InvalidConfiguration,
        CheckpointError::CapacityExceeded,
        CheckpointError::AlreadyExists,
        CheckpointError::NotFound,
        CheckpointError::Unavailable,
        CheckpointError::AuthenticationFailed,
        CheckpointError::EntropyUnavailable,
        CheckpointError::Cancelled,
    ];
    let mut errors = vec![
        RuntimeError::ExistingSession,
        RuntimeError::MissingSession,
        RuntimeError::Capacity,
        RuntimeError::Closed,
        RuntimeError::NotFinished,
        RuntimeError::InvalidCursor,
        RuntimeError::Internal,
        RuntimeError::Projection(ProjectionError::Capacity),
        RuntimeError::Projection(ProjectionError::InvalidConfiguration),
        RuntimeError::Projection(ProjectionError::Closed),
        RuntimeError::Projection(ProjectionError::Worker),
    ];
    for error in process {
        assert_eq!(
            status::process_from(status::process_code(error)).unwrap(),
            error
        );
        errors.push(RuntimeError::Process(error));
        errors.push(RuntimeError::Projection(ProjectionError::Process(error)));
    }
    errors.extend(
        terminal
            .into_iter()
            .map(|e| RuntimeError::Projection(ProjectionError::Terminal(e))),
    );
    errors.extend(
        checkpoint
            .into_iter()
            .map(|e| RuntimeError::Projection(ProjectionError::Storage(e))),
    );
    for error in errors {
        let state = SessionStatus {
            admission_error: Some(error),
            drain: Some(DrainOutcome::Failed(ProcessError::Io)),
            cancellation_requested: true,
            ..SessionStatus::default()
        };
        let event = OutputEvent::Complete(state.completion().unwrap());
        let (payload, _, complete) = codec::encode(cursor(13), &event).unwrap();
        assert!(complete);
        assert_eq!(decode_record(&record(payload)).unwrap().output, event);
    }
    for exit in [
        ExitStatus::Code(0),
        ExitStatus::Code(71),
        ExitStatus::Signal(9),
    ] {
        for drain in [
            DrainOutcome::Eof,
            DrainOutcome::Truncated,
            DrainOutcome::Failed(ProcessError::Timeout),
        ] {
            let state = SessionStatus {
                exit: Some(exit),
                drain: Some(drain),
                supervision_error: Some(ProcessError::Internal),
                ..SessionStatus::default()
            };
            let event = OutputEvent::Complete(state.completion().unwrap());
            let (payload, _, _) = codec::encode(cursor(13), &event).unwrap();
            let mut envelope = record(payload);
            assert_eq!(decode_record(&envelope).unwrap().output, event);
            let mut corrupted = envelope.event.payload.as_bytes().to_vec();
            corrupted.push(1);
            envelope.event.payload = Payload::copy_from_slice(&corrupted);
            assert_eq!(
                decode_record(&envelope),
                Err(PublicationError::InvalidRecord)
            );
        }
    }
    assert!(status::runtime_from(999).is_err());
    assert!(status::process_from(0).is_err());
}
#[test]
fn gap_and_range_boundaries_are_checked_in_both_directions() {
    let event = OutputEvent::Replay(ReplayPage::Gap {
        from: cursor(13),
        to: cursor(99),
    });
    let (payload, next, complete) = codec::encode(cursor(13), &event).unwrap();
    assert_eq!(next, cursor(99));
    assert!(!complete);
    assert_eq!(decode_record(&record(payload)).unwrap().output, event);
    assert!(codec::encode(cursor(12), &event).is_err());
    let event = OutputEvent::Replay(ReplayPage::Bytes {
        from: cursor(13),
        next: cursor(15),
        bytes: vec![1],
    });
    assert!(codec::encode(cursor(13), &event).is_err());
    assert!(codec::encode(cursor(13), &OutputEvent::Replay(ReplayPage::Pending)).is_err());
}

#[test]
fn independently_constructed_completion_wire_rejects_malformed_fields_and_truncation() {
    // External wire fixture: lifetime 91/72, byte 13, exit 71, EOF, no errors.
    let mut wire = b"PTYR\x03".to_vec();
    for value in [91_u64, 72, 13, 13] {
        wire.extend_from_slice(&value.to_le_bytes());
    }
    wire.extend_from_slice(&[1, 71, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    let expected = SessionStatus {
        exit: Some(ExitStatus::Code(71)),
        drain: Some(DrainOutcome::Eof),
        ..Default::default()
    };
    assert_eq!(
        decode_record(&record(Payload::copy_from_slice(&wire)))
            .unwrap()
            .output,
        OutputEvent::Complete(expected.completion().unwrap())
    );
    for length in 0..wire.len() {
        assert!(decode_record(&record(Payload::copy_from_slice(&wire[..length]))).is_err());
    }
    // Missing exit with a nonzero code, unknown exit/drain tags, EOF with an
    // error code, unknown supervision/admission codes, and a noncanonical bool.
    for (index, value) in [
        (37, 0),
        (37, 3),
        (42, 4),
        (43, 1),
        (45, 255),
        (47, 255),
        (49, 2),
    ] {
        let mut invalid = wire.clone();
        invalid[index] = value;
        assert!(decode_record(&record(Payload::copy_from_slice(&invalid))).is_err());
    }
    // Exhaustive single-byte perturbations: accepted variations must roundtrip
    // canonically; invalid variants must return a typed error without panicking.
    for index in 0..wire.len() {
        for value in 0..=u8::MAX {
            let mut changed = wire.clone();
            changed[index] = value;
            if let Ok(decoded) = decode_record(&record(Payload::copy_from_slice(&changed))) {
                let (encoded, _, _) = codec::encode(decoded.byte_cursor, &decoded.output).unwrap();
                assert_eq!(encoded.as_bytes(), changed);
            }
        }
    }
}
