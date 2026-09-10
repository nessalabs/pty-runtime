# CI native CPU/cache organization review

Independent reviewer: correctness_review, 2026-09-08. Scope is the uncommitted
CI portability diff on the isolated `work/ci-portability` checkout: the shared
native build recipe in `experiments/run.py`, both workflow cache keys, and
`experiments/tests/test_native_build.py`. Read AGENTS.md and coding_standards.md,
reviewed the full surrounding build path and `scripts/native/bootstrap.py`.

Result: no open P1/P2 dependency-direction, DDD, or Clean Code/SOLID findings.

The CPU policy belongs to the native build boundary. It does not add native
engine, platform, or serialization details to domain/application code. The
shared immutable option tuple is the single source for both the invoked Zig
command and its cache identity. The small stamp function remains cohesive with
the build driver; no new service abstraction or runtime policy is introduced.
The native bootstrap delegates to that same driver, avoiding a second compile
recipe. Explicit baseline CPU, recipe contents, target/libc, pinned dependencies,
and build-driver digest are represented in the local stamp. Both workflow keys
invalidate earlier caches and track the shared recipe and bootstrap source.
No fallback restore key can bring the former cache key back into this path.

The focused tests invoke the actual Python orchestration with only the external
compiler/download boundary replaced. They observe first compilation, baseline
command and stamp, reuse of matching metadata, and rebuild after CPU-policy,
driver-digest, or legacy-stamp mismatch on both platform keys. They do not claim
to establish emitted machine-code portability, execute Ghostty, or reproduce the
Ubuntu hardware fault. That evidence remains the responsibility of the native
build and CI verification record. The broader script digest intentionally trades
extra rebuilds for conservative cache invalidation; it is not a blocker.

Independent validation: `python3 -m unittest discover -s experiments/tests -p
test_native_build.py -v` passed both tests (0.014 seconds) in this checkout.
Full gate was not duplicated by this reviewer; root owns its recorded execution.
No implementation changes made.
