use super::*;

fn buffer(limit: usize) -> ReplayBuffer {
    ReplayBuffer::new(SessionLifetime::new(7, 1), limit)
}

#[test]
fn eviction_reports_gap_before_exact_suffix_and_reads_do_not_advance() {
    let mut replay = buffer(5);
    let start = replay.end();
    replay.append(b"abcd").unwrap();
    replay.append(b"efgh").unwrap();
    assert_eq!(replay.len(), 5);
    assert_eq!(
        replay.read(start, 2),
        Ok(ReplayPage::Gap {
            from: start,
            to: replay.floor()
        })
    );
    let page = replay.read(replay.floor(), 2).unwrap();
    assert_eq!(page, replay.read(replay.floor(), 2).unwrap());
    assert!(
        matches!(page, ReplayPage::Bytes { bytes, next, .. } if bytes == b"de" && next.offset == 5)
    );
}

#[test]
fn identities_and_future_positions_are_validated_even_when_empty() {
    let replay = buffer(0);
    let foreign = ReplayCursor {
        lifetime: SessionLifetime::new(8, 1),
        offset: 0,
    };
    assert_eq!(replay.read(foreign, 1), Err(ReplayError::ForeignLifetime));
    let future = ReplayCursor {
        offset: 1,
        ..replay.end()
    };
    assert_eq!(replay.read(future, 1), Err(ReplayError::FutureCursor));
    assert_eq!(
        replay.read(replay.end(), 0),
        Err(ReplayError::EmptyPageLimit)
    );
}

#[test]
fn zero_retention_and_global_eviction_keep_stream_position() {
    let mut replay = buffer(0);
    replay.append(b"lost").unwrap();
    assert!(replay.is_empty());
    assert_eq!(replay.floor().offset, 4);
    let mut replay = buffer(5);
    replay.append(b"123456789").unwrap();
    replay.retain_at_most(2);
    assert_eq!(replay.floor().offset, 7);
    assert!(
        matches!(replay.read(replay.floor(), 100), Ok(ReplayPage::Bytes { bytes, .. }) if bytes == b"89")
    );
}

#[test]
fn offset_overflow_is_atomic_and_debug_redacts_content() {
    let mut replay = buffer(10);
    replay.end = u64::MAX;
    assert_eq!(replay.append(b"x"), Err(ReplayError::OffsetExhausted));
    assert!(replay.is_empty());
    let mut replay = buffer(10);
    replay.append(b"secret").unwrap();
    assert!(!format!("{:?}", replay.read(replay.floor(), 10)).contains("secret"));
}

#[test]
fn chunking_matches_reference_suffix_across_capacities() {
    for limit in 0..32 {
        let mut replay = buffer(limit);
        let mut reference = Vec::new();
        for step in 0..200usize {
            let chunk: Vec<_> = (0..step % 17).map(|n| (step + n) as u8).collect();
            reference.extend_from_slice(&chunk);
            replay.append(&chunk).unwrap();
            let expected = &reference[reference.len().saturating_sub(limit)..];
            assert_eq!(replay.len(), expected.len());
            assert_eq!(replay.end().offset as usize, reference.len());
            let page = replay.read(replay.floor(), 100).unwrap();
            match page {
                ReplayPage::Pending => assert!(expected.is_empty()),
                ReplayPage::Bytes { bytes, .. } => assert_eq!(bytes, expected),
                ReplayPage::Gap { .. } => panic!("floor must never produce a gap"),
            }
        }
    }
}

#[test]
fn releasing_storage_preserves_exact_loss_and_continuation_positions() {
    let mut replay = buffer(8);
    let initial = replay.end();
    assert!(replay.reserve_capacity(8).unwrap() >= 8);
    replay.append(b"abcdef").unwrap();
    let end = replay.end();
    for _ in 0..2 {
        replay.release_storage();
        assert_eq!(replay.len(), 0);
        assert_eq!(replay.allocated_bytes(), 0);
        assert_eq!(replay.end(), end);
        assert_eq!(replay.floor(), end);
        for _ in 0..2 {
            assert_eq!(
                replay.read(initial, 8),
                Ok(ReplayPage::Gap {
                    from: initial,
                    to: end
                })
            );
        }
        assert_eq!(replay.read(end, 8), Ok(ReplayPage::Pending));
    }
    replay.append(b"gh").unwrap();
    assert_eq!(
        replay.end(),
        ReplayCursor {
            offset: 8,
            ..initial
        }
    );
    assert_eq!(
        replay.read(initial, 8),
        Ok(ReplayPage::Gap {
            from: initial,
            to: end
        })
    );
    assert_eq!(
        replay.read(end, 8),
        Ok(ReplayPage::Bytes {
            from: end,
            bytes: b"gh".to_vec(),
            next: ReplayCursor {
                offset: 8,
                ..initial
            },
        })
    );
}
