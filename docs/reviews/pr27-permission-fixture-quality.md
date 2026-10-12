# PR 27 permission fixture quality review

Reviewer: performance_gate, independent of the test-fix author.
Scope: the test-only change in process_image_materialize.rs, its existing fault
seam, actual assertions, and retained targeted evidence. No reviewer builds.

## Finding and correction

The previous fixture expected chmod(0500) on a directory to force EACCES for
the caller. That assumption does not hold for a sufficiently privileged UID 0
process. The replacement names its contract precisely: materializer handling
of an EACCES result from open. It uses the existing test-only syscall seam to
return -1 with EACCES once, rather than changing production error behavior or
skipping the test under elevated privileges.

Read the seam and call path: fault::write passes a stack-owned fault into the
real write_inner path; the child open syscall consumes it and reports a byte
on the retained descriptor. No high-level result is fabricated by this test.
The exact b"F" receipt assertion prevents an unconsumed injection from passing.
The real materializer still executes error conversion, child lifecycle and
caller-mask restoration. Existing assertions remain: PermissionDenied, no
new destination inode, original neighboring file content, unchanged caller
mask, and a later successful ordinary write with exact new bytes and unchanged
mask. The subsequent write has no injected fault.

The fixture does not prove OS discretionary-access-control enforcement and
explicitly says so. It proves the application response to that OS result.
Other filesystem tests still exercise actual creation, bytes and mode.
There is no production dependency, ownership, allocation, or performance change.
Reusing the narrow existing seam is cohesive; no new abstraction or design
pattern is needed. No P1/P2 quality finding in the corrected test.

## Evidence and limits

`work/pr25-blockers/pr27-eacces-targeted01` and `targeted02` both executed zero
tests because the filter used the source filename instead of the Rust module
path. They provide no affected-test evidence. The initial feature-gating
hypothesis was incorrect: the module is process::image::materialize. Both
empty runs remain retained and are excluded from acceptance.

`work/pr25-blockers/pr27-eacces-targeted03` executes the exact renamed EACCES
test and reports one passed test, with current test-source hash matching its
recorded manifest. Scoped quality review passes. Full mechanical gate and its
scoped performance workloads are pending; strict coverage remains separately
unmet and is not waived by this fixture correction.

Reviewed fixture SHA-256: `36dbdc0ce4a6b8b85ca2b2584ba380d70d6c441a6cd3c0b6727ba280feb3edb0`.

## Completed mechanical and performance evidence

Independently checked `work/pr25-blockers/pr27-eacces-gate01`: exit 0 in
67.511 seconds, with all 361 manifest source hashes still matching current
files. The full log explicitly executes the renamed EACCES test successfully
in its workspace runs. No zero-test filter result is used for this conclusion.

The mandatory runtime and domain performance captures both exit 0, and their
before/after source identities match the current source identity. Runtime
evidence `20261012T185053Z-8ccb2bb3` completes the 1/16-session trials in
53.802/666.632 ms with cleanup 1.086/1.500 ms. Raw and parser byte counts match
at 4,194,326/67,109,222, both shutdown samples have five descriptors and zero
descendants, and post-processing process RSS is 11,550,720/148,897,792 bytes.
RSS is a process point sample, not peak or total-process-family memory.
Domain evidence `20261012T185056Z-4df7f482` retains all 25 ordered positive
trials; ordinary/oversized/query/saturated/one-byte medians are
0.679/8.961/5.537/13.680/10.074 ms.

Scoped quality and mandatory workload review passes for this test-only fix.
No production source was changed, so no extra benchmark rerun was requested
or performed. These scoped workloads do not establish a relative-regression
threshold, full release qualification or soak. The strict coverage gap remains
separate and unresolved.
