# Actual reader allocation measurements

Adds optional current-reader count and actual Vec capacity gauges. Infrastructure owns scratch and releases it before its application measurement guard; cumulative-counter reset preserves current allocation measurements. Checkpoint JSON reports explicit null before diagnostics exist. The fixture also exposes the existing per-session staging-slot option without changing production defaults.

Behavioral verification includes an observed pre-hook failure (expected two actual readers/12,284 scratch bytes, observed zero), then passing real-reader normal exit, retained handle, joined shutdown and caught callback-panic tests. Independent correctness re-executed both actual-reader tests and both guard tests. Independent DDD, organization and correctness reviews are linked in docs/archive/reviews/reader-memory-gauges-*.md.

The recorded release build and scoped idle/projected-capacity smoke runs passed with matching, unchanged source inventories. Each smoke used four residents, and recorded four live readers with 16,384 bytes of actual scratch at ready/measurement checkpoints, then zero readers/scratch after shutdown. The baseline reports null. Smoke does not qualify full duration, 64-resident control memory, peak throughput, plateau or release latency.

Independent relaxed snapshots are approximate during allocation/drop. The fixture control handshake is not itself an allocation barrier; memory decomposition must verify settled actual counts/capacity in each record and preserve fixture/shared baseline costs. The full 4 KiB target, coverage target, release matrix, 12-hour soak and deferred native issue remain separate requirements.

The first full gate reached documentation generation after passing its tests, then failed on two unescaped `Vec<u8>` Rustdoc phrases. The retained `macos-gate` log records this documentation failure. Only inline-code backticks were added; all three reviewers independently verified the two-line correction against prior hashes. The final-source release build passed in `reviewed-source-build`; the authoritative complete gate rerun is `macos-gate-reviewed`.

Final reviewed gate passed with unchanged source; its 293-file inventory exactly matches the final-source release build and working source at the checkpoint.
