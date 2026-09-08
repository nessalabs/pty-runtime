use super::{Attachment, Child, DEADLINE, Harness, Result, verify};
use pty_runtime::*;
use std::time::{Duration, Instant};

// Preserve the exact error value. Diagnostics are best-effort and inspect public
// state only after failure, before the caller drops its session or harness.
pub(super) fn diagnose<T, E: std::fmt::Debug>(
    result: std::result::Result<T, E>,
    phase: &str,
    turn: Option<u64>,
    child: Option<&Child>,
) -> std::result::Result<T, E> {
    if let (Err(error), Some(turn)) = (&result, turn) {
        use std::io::Write;
        let mut stderr = std::io::stderr().lock();
        if let Some(child) = child {
            let _ = writeln!(
                stderr,
                "soak_error phase={phase} turn={turn} seed={} lifetime={:?} error={error:?} session_status={:?} projection_status={:?}",
                child.pattern_seed,
                child.session.lifetime(),
                child.session.status(),
                child.session.projection_status(),
            );
        } else {
            let _ = writeln!(
                stderr,
                "soak_error phase={phase} turn={turn} error={error:?} session_status=unavailable projection_status=unavailable",
            );
        }
    }
    result
}

async fn observe_projection<T>(
    operation: std::result::Result<ProjectionOperation<T>, RuntimeError>,
    phase: &str,
    turn: u64,
    child: &Child,
) -> Result<T> {
    let result: Result<T> = async {
        Ok(tokio::time::timeout(DEADLINE, operation?)
            .await?
            .map_err(RuntimeError::from)?)
    }
    .await;
    diagnose(result, phase, Some(turn), Some(child))
}

#[derive(Default)]
struct Accounting {
    commanded: u64,
    observed: u64,
    gaps: u64,
    cold_cycles: u64,
    reparks: u64,
    parked_in_cycle: bool,
}
impl Accounting {
    fn observe(&mut self, observer: &mut Attachment, seed: u64) -> Result<()> {
        let (seen, lost) = verify(observer, seed)?;
        self.observed += seen;
        self.gaps += lost;
        assert!(self.observed + self.gaps <= self.commanded);
        Ok(())
    }
}
fn burst(child: &mut Child, accounting: &mut Accounting) -> Result<()> {
    child.burst()?;
    accounting.commanded += 4096;
    Ok(())
}
async fn cold_cycle(
    child: &mut Child,
    observer: &mut Attachment,
    accounting: &mut Accounting,
) -> Result<()> {
    assert_eq!(
        child.session.projection_status()?.unwrap().residency,
        Residency::Parked
    );
    println!(
        "{{\"event\":\"cold_wake\",\"seed\":{}}}",
        child.pattern_seed
    );
    burst(child, accounting)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        let status = child.session.projection_status()?.unwrap();
        assert!(status.failure.is_none(), "{status:?}");
        if status.processed.offset == accounting.commanded {
            assert!(matches!(
                status.residency,
                Residency::Resident | Residency::Usable
            ));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cold output did not restore and catch up: {status:?}"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    drop(
        tokio::time::timeout(DEADLINE, child.session.projected_view()?)
            .await?
            .map_err(RuntimeError::from)?,
    );
    drop(
        tokio::time::timeout(DEADLINE, child.session.terminal_checkpoint()?)
            .await?
            .map_err(RuntimeError::from)?,
    );
    let cursor = observer.cursor();
    *observer = child.session.attach(AttachPosition::Cursor(cursor))?;
    assert_eq!(observer.cursor(), cursor);
    accounting.observe(observer, child.pattern_seed)?;
    accounting.cold_cycles += 1;
    accounting.parked_in_cycle = false;
    println!(
        "{{\"event\":\"cold_restored\",\"seed\":{},\"cycle\":{}}}",
        child.pattern_seed, accounting.cold_cycles
    );
    Ok(())
}

pub async fn run(seconds: u64) -> Result<()> {
    if !cfg!(feature = "ghostty") {
        return Err(
            std::io::Error::other("mixed release soak requires default Ghostty feature").into(),
        );
    }
    let harness = Harness::new()?;
    let mut children = (0..8)
        .map(|index| harness.spawn(index % 2 == 1))
        .collect::<Result<Vec<_>>>()?;
    let mut observers = children
        .iter()
        .map(|child| {
            child.session.attach(AttachPosition::Cursor(ReplayCursor {
                lifetime: child.session.lifetime(),
                offset: 0,
            }))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let started = Instant::now();
    let mut last_mutation = [started; 8];
    let mut accounting: Vec<_> = (0..8).map(|_| Accounting::default()).collect();
    let mut turns = 0u64;
    let mut parked_observations = 0u64;
    while started.elapsed() < Duration::from_secs(seconds) {
        let index = turns as usize % 4;
        burst(&mut children[index], &mut accounting[index])?;
        for index in [5, 7] {
            if last_mutation[index].elapsed() >= Duration::from_secs(180) {
                cold_cycle(
                    &mut children[index],
                    &mut observers[index],
                    &mut accounting[index],
                )
                .await?;
                last_mutation[index] = Instant::now();
            }
        }
        for (index, child) in children.iter().enumerate() {
            accounting[index].observe(&mut observers[index], child.pattern_seed)?;
            let status = child.session.status()?;
            assert!(
                status.completion().is_none(),
                "unexpected soak completion: {status:?}"
            );
            if let Some(projection) = child.session.projection_status()? {
                assert!(projection.failure.is_none(), "{projection:?}");
                assert!(projection.parking_failure.is_none(), "{projection:?}");
                if index >= 4 && last_mutation[index].elapsed() > Duration::from_secs(75) {
                    assert_eq!(projection.residency, Residency::Parked);
                    parked_observations += 1;
                    if !accounting[index].parked_in_cycle {
                        accounting[index].reparks += u64::from(accounting[index].cold_cycles > 0);
                        accounting[index].parked_in_cycle = true;
                    }
                }
            }
        }
        if turns % 16 == 0 {
            harness.metrics("soak", false)?;
            drop(
                observe_projection(
                    children[1].session.projected_view(),
                    "periodic_view",
                    turns,
                    &children[1],
                )
                .await?,
            );
            drop(
                observe_projection(
                    children[3].session.terminal_checkpoint(),
                    "periodic_checkpoint",
                    turns,
                    &children[3],
                )
                .await?,
            );
            let outcome = observe_projection(
                children[1].session.resize_projected(
                    TerminalSize::new(if turns % 32 == 0 { 90 } else { 80 }, 24).unwrap(),
                ),
                "periodic_resize",
                turns,
                &children[1],
            )
            .await?;
            assert!(outcome.os.is_ok() && outcome.model.is_ok());
            let bytes: u64 = accounting.iter().map(|value| value.observed).sum();
            let gaps: u64 = accounting.iter().map(|value| value.gaps).sum();
            println!(
                "{{\"event\":\"soak_progress\",\"elapsed_seconds\":{},\"turns\":{turns},\"verified_bytes\":{bytes},\"gap_bytes\":{gaps},\"parked_observations\":{parked_observations}}}",
                started.elapsed().as_secs_f64()
            );
        }
        if turns % 32 == 0 {
            let transient = diagnose(
                harness.spawn(turns % 64 == 0),
                if turns % 64 == 0 {
                    "transient_spawn_projected"
                } else {
                    "transient_spawn_raw"
                },
                Some(turns),
                None,
            )?;
            harness
                .finish_observed(transient, turns % 64 != 0, Some(turns))
                .await?;
        }
        turns += 1;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    for (index, child) in children.into_iter().enumerate() {
        let seed = child.pattern_seed;
        harness.finish(child, false).await?;
        accounting[index].observe(&mut observers[index], seed)?;
        let totals = &accounting[index];
        assert_eq!(
            totals.observed + totals.gaps,
            totals.commanded,
            "session {index}"
        );
        if matches!(index, 5 | 7) && seconds >= 270 {
            assert!(totals.cold_cycles > 0 && totals.reparks > 0);
        }
        println!(
            "{{\"event\":\"session_verified\",\"index\":{index},\"seed\":{seed},\"commanded\":{},\"observed\":{},\"gaps\":{},\"cold_cycles\":{},\"reparks\":{}}}",
            totals.commanded, totals.observed, totals.gaps, totals.cold_cycles, totals.reparks
        );
    }
    drop(observers);
    harness.metrics("soak_final", true)?;
    let commanded: u64 = accounting.iter().map(|value| value.commanded).sum();
    let cold_cycles: u64 = accounting.iter().map(|value| value.cold_cycles).sum();
    assert_eq!(commanded, (turns + cold_cycles) * 4096);
    if seconds > 75 {
        assert!(parked_observations > 0);
    }
    Ok(())
}
