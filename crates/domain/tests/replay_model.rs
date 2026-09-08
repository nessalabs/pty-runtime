//! Independent stream-ledger tests across append, pressure, and observer reads.
use pty_runtime_domain::{ReplayBuffer, ReplayCursor, ReplayError, ReplayPage, SessionLifetime};

fn random(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}

#[test]
fn seeded_interleavings_match_an_append_only_ledger() {
    for limit in [0usize, 1, 3, 16, 127] {
        for seed in 1..=32 {
            let lifetime = SessionLifetime::new(99, seed);
            let mut actual = ReplayBuffer::new(lifetime, limit);
            let mut all_output = Vec::<u8>::new();
            let mut earliest = 0usize;
            let mut rng = seed;
            for step in 0..400 {
                if random(&mut rng) % 4 == 0 {
                    let pressure = random(&mut rng) % 150;
                    actual.retain_at_most(pressure);
                    earliest = earliest.max(all_output.len().saturating_sub(pressure));
                } else {
                    let length = random(&mut rng) % 200;
                    let chunk: Vec<u8> = (0..length).map(|_| random(&mut rng) as u8).collect();
                    all_output.extend_from_slice(&chunk);
                    actual.append(&chunk).unwrap();
                    earliest = earliest.max(all_output.len().saturating_sub(limit));
                }
                assert_eq!(actual.len(), all_output.len() - earliest);
                assert_eq!(actual.floor().offset, earliest as u64);
                assert_eq!(actual.end().offset, all_output.len() as u64);
                let random_offset = random(&mut rng) % (all_output.len() + 2);
                for offset in [
                    0,
                    earliest,
                    all_output.len(),
                    all_output.len() + 1,
                    random_offset,
                ] {
                    let cursor = ReplayCursor {
                        lifetime,
                        offset: offset as u64,
                    };
                    let page_size = random(&mut rng) % 31 + 1;
                    let expected = if offset > all_output.len() {
                        Err(ReplayError::FutureCursor)
                    } else if offset < earliest {
                        Ok(ReplayPage::Gap {
                            from: cursor,
                            to: ReplayCursor {
                                lifetime,
                                offset: earliest as u64,
                            },
                        })
                    } else if offset == all_output.len() {
                        Ok(ReplayPage::Pending)
                    } else {
                        let end = (offset + page_size).min(all_output.len());
                        Ok(ReplayPage::Bytes {
                            from: cursor,
                            next: ReplayCursor {
                                lifetime,
                                offset: end as u64,
                            },
                            bytes: all_output[offset..end].to_vec(),
                        })
                    };
                    assert_eq!(
                        actual.read(cursor, page_size),
                        expected,
                        "limit={limit} seed={seed} step={step}"
                    );
                    assert_eq!(
                        actual.read(cursor, page_size),
                        expected,
                        "read mutated observer position"
                    );
                }
                assert_eq!(
                    actual.read(
                        ReplayCursor {
                            lifetime: SessionLifetime::new(98, seed),
                            offset: 0
                        },
                        1
                    ),
                    Err(ReplayError::ForeignLifetime)
                );
                assert_eq!(
                    actual.read(actual.end(), 0),
                    Err(ReplayError::EmptyPageLimit)
                );
            }
        }
    }
}
