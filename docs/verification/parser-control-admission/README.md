# Parser pressure and independent control admission

ADR 0002 requires controls to remain serviceable during continuous output and
reserves lifecycle control independently of input/output admission. ADR 0003
preserves that requirement at parser-backlog capacity. The previous single slot
quota let parser output consume every resize-admission slot.

## Retained failure and correction

The candidate-2 dominant trial failed once; all five frozen candidate-3 full
projected-capacity trials failed in warmup. The failure-only diagnostic then
identified `resize_projected` **admission**, with healthy resident projection and
no recorded session failure. Immediate shared resources showed 4,094 parser
slots across 16 active producers, each limited to 256. That sample alone does
not prove a particular local quota rejection.

Independent deterministic tests filled either the local or shared parser slots.
Both failed at resize admission before the production correction; `red.log` and
`red-source.json` retain that result. Controls now use their existing bounded
request tickets, while nonempty output retains parser byte/slot leases. Queue
backing is reserved using the checked sum of both finite populations. No control
bypasses earlier output; request saturation still returns Capacity.

The same tests now pass and prove exact output/resize FIFO order, no lost bytes,
finite control-request admission, retained-wait charging, re-admission after drop,
and final quota release. Independent all-projection verification passed 52 tests.
All enqueue/requeue ownership paths were reviewed. The pre-existing failure
retention path can discard deque backing; this fix does not claim zero allocations
after failure, only the stated bounded normal admission and ownership contract.

## Integrated verification and limits

`release-build/` records the release image used by `saturation-64-16/`: the same
64-resident/16-active unpaced 1-second warmup and 1-second measurement reproduction
now completes with byte/query/gap checks and zero final logical quotas. This is a
bounded reproduction, not the required five full 60-second capacity repeats.

The final macOS full gate is recorded separately under `macos-gate-reviewed/`.
Independent DDD, strict organization and behavioral reviews remain under
`docs/reviews`. The bundled-file FIFO fix and added domain/application contracts
have separate verification directories. No malformed native input was investigated.

Five-repeat projected-capacity/dominant qualification, final-source Linux checks,
full remaining workloads, a source-relevant 12-hour soak and strict coverage remain
required. Older candidate source/platform evidence is not relabeled as this fix.
