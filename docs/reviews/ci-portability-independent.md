# Independent behavioral review: native cache portability

Reviewed on 2026-09-08 in the isolated `work/ci-portability` worktree, against
base `06508816021b17f325cb35dce626bef56bc15e20`. Scope: `experiments/run.py`, both
GitHub workflows, and `experiments/tests/test_native_build.py`. No implementation
files were edited for this review.

Disposition: no blocking behavioral findings. The narrow fix is ready for the
planned CI run. A passing repaired Ubuntu run remains required before claiming
that the supplied failed job's observed SIGILL has been resolved.

The actual native compile command now receives `-Dcpu=baseline` from the same
constant included in the cache stamp. I independently inspected pinned Ghostty's
`Config.zig` and Zig's target resolver in the local cache: Linux consumes standard
target options, and the resolver maps explicit baseline to `Target.Cpu.baseline`,
whereas an omitted CPU/architecture query can detect host CPU features. Ghostty's
existing generic macOS target remains a separate upstream path. The change fixes
a real cross-runner cache portability defect without changing optimization level,
test selection, timeout behavior, or runtime terminal code.

Cache review: native build options, the driver content hash, dependencies,
architecture/OS, and libc participate in stamp equality. Existing JSON tuple/list
normalization remains intact. An absent library or legacy/changed stamp triggers
a native rebuild; the stamp is written only after successful compilation and
verification that the library exists. Both workflow keys use new baseline-v2
names and hash the driver plus bootstrap, so old immutable action cache entries
cannot bypass rebuilding. No restore prefix reintroduces old keys. The experiment
comparison path can temporarily use an older driver's cache, but the current
recipe's stamp mismatch forces rebuilding before the current measurements.

Validation independently executed: `python3 -m unittest discover -s experiments/tests -v`
passed all 21 tests, including both new cache-contract cases. They exercise the
actual build orchestration while substituting external compilation/download,
check emitted baseline options, prove warm-cache reuse, and force recompilation
for changed CPU options, changed driver identity, and legacy metadata.
`git diff --check` passed. I also inspected the retained failed Ubuntu log and
root's recorded full macOS gate evidence. The focused tests establish command and
cache behavior; they are not native CPU execution tests. The macOS gate does not
substitute for the next Ubuntu GitHub run.

The investigation correctly preserves uncertainty: the original log reports
SIGILL, but no faulting opcode/core was retained. Cached unsupported ISA is well
supported by the source defect and surrounding evidence; an explicit native trap
cannot be excluded solely from that log. No broader runtime changes are needed
in this narrow fix.
