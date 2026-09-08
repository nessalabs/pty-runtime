# Reviewed harness and validator checkpoint

This checkpoint adds bounded unpaced raw/projected load modes, distinct producer
windows/backpressure/RTT measurements, explicit correctness versus performance
target summaries, partial CPU-census scope, successful-driver pipe cleanup and
the mixed native-memory/pool experiment-validator correction.

The final macOS full mechanical gate and release build pass with identical,
unchanged source inventories. See `macos-gate/metadata.json`, its complete
`command.log`, and `final-source-build/metadata.json`. Focused raw/projected smoke
and cap-censor tests retain their own identities and limitations.

Independent reviews cover capacity correctness (including cap-boundary fix),
DDD/organization, Python reporting contracts (including tri-state red/green
correction), and experiment validation. No P1/P2 remains in this harness/validator
scope. This is a checkpoint, not release qualification.

The candidate-2 dominant-load failure remains open. Full capacity repeats,
remaining release matrix, running 12-hour soak, strict 100% coverage and known
user-deferred native crash remain unresolved. Historical failures and source
identities are preserved. Prior checkpoint CI passed the runtime workflow but
failed the experiment validator; new CI must verify the validator correction.
