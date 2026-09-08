use pty_runtime_application::terminal::{ITerminal, ITerminalFactory};
use pty_runtime_domain::{ReplayCursor, SessionLifetime, terminal::*};
use pty_runtime_infrastructure::terminal::GhosttyTerminalFactory;
use std::io::Write;
#[path = "inputs.rs"]
mod inputs;
use inputs::Random;

fn config() -> TerminalConfig {
    TerminalConfig {
        size: TerminalSize::new(40, 12).unwrap(),
        history_bytes: 16 * 1024 * 1024,
        continuation_bytes: 64 * 1024,
        reply_bytes: 4096,
        checkpoint_bytes: 8 * 1024 * 1024,
        feed_bytes: 4096,
        native_bytes: 32 * 1024 * 1024,
        view_bytes: 1024 * 1024,
    }
}
fn descriptor(offset: u64, generation: u64) -> CheckpointDescriptor {
    CheckpointDescriptor {
        compatibility: GhosttyTerminalFactory.compatibility().into(),
        processed: ReplayCursor {
            lifetime: SessionLifetime::new(1, 1),
            offset,
        },
        control_generation: generation,
    }
}
fn finish(t: &mut dyn ITerminal) -> Result<(), TerminalError> {
    for _ in 0..4096 {
        if t.restore_history_step()?.is_finished() {
            return Ok(());
        }
    }
    panic!("bounded history-step limit exceeded");
}
unsafe extern "C" {
    fn rt_verify_format(
        bytes: *const u8,
        len: usize,
        out: *mut u8,
        cap: usize,
        written: *mut usize,
    ) -> i32;
}
fn canonical(cp: TerminalCheckpoint) -> Vec<u8> {
    let mut bytes = vec![0; 4 * 1024 * 1024];
    let mut len = 0;
    // SAFETY: Independent synchronous C oracle borrows immutable encoded bytes,
    // writes within initialized output capacity, and retains no Rust pointers.
    assert_eq!(
        unsafe {
            rt_verify_format(
                cp.bytes.as_ptr(),
                cp.bytes.len(),
                bytes.as_mut_ptr(),
                bytes.len(),
                &mut len,
            )
        },
        0
    );
    assert!(len <= bytes.len());
    bytes.truncate(len);
    bytes
}
fn equivalent(a: &mut dyn ITerminal, b: &mut dyn ITerminal, d: CheckpointDescriptor) {
    let av = a.view().unwrap();
    let bv = b.view().unwrap();
    if av != bv {
        eprintln!(
            "active mismatch: cursor {:?} / {:?}; modes {:?} / {:?}; first cell {:?}",
            av.cursor,
            bv.cursor,
            av.modes,
            bv.modes,
            av.cells.iter().zip(&bv.cells).position(|(x, y)| x != y)
        );
    }
    assert!(av == bv, "active state mismatch");
    let a = canonical(a.checkpoint(d.clone()).unwrap());
    let b = canonical(b.checkpoint(d).unwrap());
    if a != b && std::env::var_os("PTY_CORPUS_TRACE").is_some() {
        std::fs::create_dir_all("work/terminal-corpus").unwrap();
        std::fs::write("work/terminal-corpus/reference.vt", &a).unwrap();
        std::fs::write("work/terminal-corpus/restored.vt", &b).unwrap();
    }
    assert!(
        a == b,
        "canonical full history/state mismatch: {} versus {} bytes",
        a.len(),
        b.len()
    );
}
#[derive(Default, Debug)]
struct Counts {
    feeds: u64,
    resizes: u64,
    restores: u64,
    malformed: u64,
    rejected: u64,
    accepted_mutations: u64,
}
fn malformed(cp: &TerminalCheckpoint, rng: &mut Random, counts: &mut Counts, size: TerminalSize) {
    for kind in 0..6 {
        let mut mutated = cp.clone();
        rng.mutate(&mut mutated.bytes, kind);
        counts.malformed += 1;
        let result = GhosttyTerminalFactory
            .restore(mutated, TerminalConfig { size, ..config() })
            .and_then(|mut t| {
                finish(t.as_mut())?;
                // Successfully decoded mutations can be valid; require bounded usable state.
                t.view()?;
                t.feed(b"\x18\x1b[0mprobe")?;
                t.view()?;
                Ok(())
            });
        match result {
            Ok(()) => counts.accepted_mutations += 1,
            Err(
                TerminalError::CorruptCheckpoint
                | TerminalError::InvalidConfiguration
                | TerminalError::BudgetExceeded
                | TerminalError::EngineFailure,
            ) => counts.rejected += 1,
            Err(other) => panic!("unexpected malformed-snapshot outcome {other:?}"),
        }
    }
}
fn case(seed: u64, steps: usize, counts: &mut Counts) {
    let mut rng = Random(seed);
    let mut a = GhosttyTerminalFactory.create(config()).unwrap();
    let mut b = GhosttyTerminalFactory.create(config()).unwrap();
    // Every case exercises actual retained history, independent of random choices.
    for i in 0..160 {
        let line = format!("seed {seed} history {i}\r\n");
        assert_eq!(a.feed(line.as_bytes()), b.feed(line.as_bytes()));
    }
    let mut offset = 0;
    let mut generation = 0;
    for step in 0..steps {
        let operation = rng.bounded(4);
        if std::env::var_os("PTY_CORPUS_TRACE").is_some() {
            eprintln!("step={step} operation={operation}");
        }
        match operation {
            0 | 1 => {
                let bytes = rng.feed();
                if std::env::var_os("PTY_CORPUS_TRACE").is_some() {
                    eprintln!("feed={bytes:02x?}");
                }
                let left = a.feed(&bytes);
                let right = b.feed(&bytes);
                assert_eq!(left, right, "feed outcome step {step}");
                left.unwrap();
                offset += bytes.len() as u64;
                counts.feeds += 1;
            }
            2 => {
                generation += 1;
                let size =
                    TerminalSize::new((20 + rng.bounded(101)) as u16, (4 + rng.bounded(37)) as u16)
                        .unwrap();
                if std::env::var_os("PTY_CORPUS_TRACE").is_some() {
                    eprintln!("resize={size:?}");
                }
                a.resize(size, generation).unwrap();
                b.resize(size, generation).unwrap();
                counts.resizes += 1;
            }
            _ => {
                let d = descriptor(offset, generation);
                if std::env::var_os("PTY_CORPUS_TRACE").is_some() {
                    eprintln!("before-restore");
                }
                equivalent(a.as_mut(), b.as_mut(), d.clone());
                let cp = b.checkpoint(d.clone()).unwrap();
                let size = b.view().unwrap().size;
                malformed(&cp, &mut rng, counts, size);
                b = GhosttyTerminalFactory
                    .restore(cp, TerminalConfig { size, ..config() })
                    .unwrap();
                finish(b.as_mut()).unwrap();
                if std::env::var_os("PTY_CORPUS_TRACE").is_some() {
                    eprintln!("after-restore: {:?}", b.restoration_progress());
                }
                equivalent(a.as_mut(), b.as_mut(), d);
                counts.restores += 1;
            }
        }
    }
    let d = descriptor(offset, generation);
    equivalent(a.as_mut(), b.as_mut(), d.clone());
    let cp = b.checkpoint(d.clone()).unwrap();
    let size = b.view().unwrap().size;
    malformed(&cp, &mut rng, counts, size);
    b = GhosttyTerminalFactory
        .restore(cp, TerminalConfig { size, ..config() })
        .unwrap();
    finish(b.as_mut()).unwrap();
    counts.restores += 1;
    equivalent(a.as_mut(), b.as_mut(), d);
}
pub fn run() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        args.len() <= 3,
        "usage: terminal_corpus [seed] [cases] [steps]"
    );
    let parse = |i, default| {
        args.get(i)
            .map(|s: &String| s.parse::<u64>().expect("unsigned decimal argument"))
            .unwrap_or(default)
    };
    let seed = parse(0, 1);
    let cases = parse(1, 100);
    let steps = parse(2, 64);
    assert!((1..=1_000_000).contains(&cases) && (1..=4096).contains(&steps));
    let started = std::time::Instant::now();
    let mut counts = Counts::default();
    for i in 0..cases {
        let current = seed.wrapping_add(i);
        println!("corpus=v1 case={i} seed={current} steps={steps}");
        std::io::stdout().flush().unwrap();
        case(current, steps as usize, &mut counts);
    }
    println!(
        "PASS corpus=v1 seed={seed} cases={cases} steps={steps} elapsed_ms={} {counts:?}",
        started.elapsed().as_millis()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restored_history_survives_later_resize() {
        let mut a = GhosttyTerminalFactory.create(config()).unwrap();
        for i in 0..160 {
            a.feed(format!("seed 1 history {i}\r\n").as_bytes())
                .unwrap();
        }
        let cp = a.checkpoint(descriptor(0, 0)).unwrap();
        let mut b = GhosttyTerminalFactory.restore(cp, config()).unwrap();
        finish(b.as_mut()).unwrap();
        equivalent(a.as_mut(), b.as_mut(), descriptor(0, 0));
        let size = TerminalSize::new(89, 38).unwrap();
        a.resize(size, 1).unwrap();
        b.resize(size, 1).unwrap();
        equivalent(a.as_mut(), b.as_mut(), descriptor(0, 1));
    }
    #[test]
    fn alternate_wrap_roundtrip_after_width_change() {
        let c = TerminalConfig {
            size: TerminalSize::new(53, 12).unwrap(),
            ..config()
        };
        let mut a = GhosttyTerminalFactory.create(c).unwrap();
        a.feed(b"\x1b[?1049h").unwrap();
        a.feed(&[b'x'; 53]).unwrap();
        assert!(a.view().unwrap().cursor.pending_wrap);
        let size = TerminalSize::new(100, 15).unwrap();
        a.resize(size, 1).unwrap();
        let cp = a.checkpoint(descriptor(0, 1)).unwrap();
        let mut b = GhosttyTerminalFactory
            .restore(cp, TerminalConfig { size, ..c })
            .unwrap();
        finish(b.as_mut()).unwrap();
        let original = a.view().unwrap().cursor;
        let restored = b.view().unwrap().cursor;
        let left = a.feed(b"Z").unwrap();
        let right = b.feed(b"Z").unwrap();
        assert_eq!(left, right);
        let after_a = a.view().unwrap();
        let after_b = b.view().unwrap();
        assert!(
            after_a == after_b,
            "cursor before next byte: {original:?} / {restored:?}; afterward {:?} / {:?}",
            after_a.cursor,
            after_b.cursor
        );
    }
}
