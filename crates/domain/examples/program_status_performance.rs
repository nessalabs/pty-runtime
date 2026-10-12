//! Deterministic bounded workloads; timings are observations, not release qualification.
use pty_runtime_domain::terminal::program_status::ProgramStatusStore;
use std::{hint::black_box, time::Instant};

fn exercise(name: &str, chunks: &[Vec<u8>], rounds: usize) {
    // Account for allocations outside the timed loop; this does not measure OS RSS.
    let mut admission = ProgramStatusStore::default();
    let mut max_plan_heap = 0;
    for chunk in chunks {
        let plan = admission.scan(chunk).unwrap();
        max_plan_heap = max_plan_heap.max(plan.retained_bytes());
        assert!(
            plan.retained_bytes() <= ProgramStatusStore::scan_reservation(chunk.len()).unwrap()
        );
        admission.commit(plan).unwrap();
        assert!(admission.retained_bytes() <= admission.record_reservation());
    }
    for trial in 0..5 {
        let mut store = ProgramStatusStore::default();
        let started = Instant::now();
        let mut queries = 0;
        for _ in 0..rounds {
            for chunk in chunks {
                let plan = store
                    .scan(black_box(chunk))
                    .expect("bounded benchmark input");
                queries += plan.query_offsets().len();
                store.commit(plan).expect("fresh plan");
            }
        }
        let elapsed = started.elapsed().as_nanos();
        let snapshot = store.records();
        let count = snapshot.records.len();
        let store_heap = store.retained_bytes();
        let snapshot_heap = snapshot.retained_bytes();
        assert!(store_heap <= store.record_reservation());
        assert!(snapshot_heap <= store.record_reservation());
        let bytes = chunks.iter().map(Vec::len).sum::<usize>() * rounds;
        match name {
            "ordinary" | "oversized" => assert_eq!(count, 0),
            "queries" => {
                assert_eq!(count, 0);
                assert_eq!(queries, rounds * 512);
            }
            "saturated" => assert_eq!(count, 256),
            "one_byte" => assert_eq!(count, 1),
            _ => unreachable!(),
        }
        println!(
            "{{\"workload\":\"{name}\",\"trial\":{trial},\"bytes\":{bytes},\"elapsed_ns\":{elapsed},\"records\":{count},\"queries\":{queries},\"preflight_max_plan_heap\":{max_plan_heap},\"store_heap\":{store_heap},\"snapshot_heap\":{snapshot_heap}}}"
        );
        black_box(store);
    }
}
fn main() {
    exercise("ordinary", &[vec![b'x'; 4096]], 2048);
    let mut oversized = b"\x1b]7501;state=working:msg=".to_vec();
    oversized.extend(vec![b'A'; 8192]);
    oversized.push(7);
    exercise("oversized", &[oversized], 1024);
    exercise("queries", &[b"\x1b]7501;?\x07".repeat(512)], 1024);
    let reports: Vec<Vec<u8>> = (0..257)
        .map(|i| format!("\x1b]7501;state=working:id=job{i}:msg=eA==\x07").into_bytes())
        .collect();
    exercise("saturated", &reports, 64);
    let split: Vec<Vec<u8>> = b"\x1b]7501;state=blocked:kind=auth:msg=TG9naW4=\x1b\\"
        .iter()
        .map(|b| vec![*b])
        .collect();
    exercise("one_byte", &split, 4096);
}
