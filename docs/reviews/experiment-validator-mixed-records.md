# Native experiment mixed-record validator correction

The retained f9da4e1 experiment CI log shows both Ubuntu and macOS failing with
`KeyError: tracked_native_bytes` on the first native lifecycle case. The fixture
now emits a `packed_pool` record immediately before each `memory` record, using
the same stage names. The cleanup loop selected parked/cleanup stages without
checking record kind and read memory-only fields from the pool record.

Added tests before the correction, using an exact retained real JSONL lifecycle
run copied into `experiments/tests/fixtures/native-lifecycle-packed.jsonl` with
provenance. The valid mixed-record summary reproduced the same KeyError. Other
new tests require memory retention to remain rejected, pool-owned requested/
mapped bytes and unreleased maps to be rejected, a missing pool cleanup stage
to fail, and historical memory-only records to retain identical metrics.

The memory cleanup loop now selects only memory records. If pool records are
present, their stage inventory must be complete/nonduplicate; parked and cleanup
records must have nonnegative actual schema counters, zero requested/mapped
bytes and equal maps/unmaps. There is no invented tracked_native_bytes field on
pool records. Memory-only historical runs remain supported, and existing memory
retention assertions and metric denominators are unchanged.

All 26 experiment validator/build-contract unit tests pass after the correction.
Build-contract tests substitute external build operations: their printed build
messages are not fresh native build evidence. Red/green logs, original CI failure,
fixture and reviewed hashes are retained in
`docs/verification/experiment-validator-mixed-records/`.

Only `experiments/gate.py` and experiment tests/fixture were changed as product
work. No native implementation, workload threshold, production allocator or crash
investigation changed. Reading retained fixture records does not qualify the
experimental packing implementation. Root owns independent rereview, final gate,
push and authoritative fresh CI result; the old failed CI run remains failed.
