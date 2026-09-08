use super::{Attachment, Child, DEADLINE, Harness, Result, verify};
use pty_runtime::*;
use std::time::{Duration, Instant};

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
                tokio::time::timeout(DEADLINE, children[1].session.projected_view()?)
                    .await?
                    .map_err(RuntimeError::from)?,
            );
            drop(
                tokio::time::timeout(DEADLINE, children[3].session.terminal_checkpoint()?)
                    .await?
                    .map_err(RuntimeError::from)?,
            );
            let outcome = tokio::time::timeout(
                DEADLINE,
                children[1].session.resize_projected(
                    TerminalSize::new(if turns % 32 == 0 { 90 } else { 80 }, 24).unwrap(),
                )?,
            )
            .await?
            .map_err(RuntimeError::from)?;
            assert!(outcome.os.is_ok() && outcome.model.is_ok());
            let bytes: u64 = accounting.iter().map(|value| value.observed).sum();
            let gaps: u64 = accounting.iter().map(|value| value.gaps).sum();
            println!(
                "{{\"event\":\"soak_progress\",\"elapsed_seconds\":{},\"turns\":{turns},\"verified_bytes\":{bytes},\"gap_bytes\":{gaps},\"parked_observations\":{parked_observations}}}",
                started.elapsed().as_secs_f64()
            );
        }
        if turns % 32 == 0 {
            let transient = harness.spawn(turns % 64 == 0)?;
            harness.finish(transient, turns % 64 != 0).await?;
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
