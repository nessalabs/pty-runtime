# Guardian functional coverage instrumentation — 2026-09-08

**No trustworthy guardian functional source-coverage percentage was obtained.**
All five helper unit tests and all 20 existing actual-helper protocol cases pass
under each completed instrumentation variant, but the resulting LLVM source
reports contain saturated inferred region counts. The reported 80.88% line
coverage from the directory named `functional-coverage-verified` is **rejected**;
that name predates the final counter sanity audit. Do not merge that percentage
into the coverage readiness ledger or call the helper 100% covered.

The prior unit-only measurement and its narrow population remain distinct from
this attempt. These runs do prove that the real helper protocol fixtures execute
successfully with the stated instrumentation, including fork, `_exit`, exec,
helper SIGKILL/SIGABRT, owner EOF, and handoff failures. They do not establish
valid coverage counts for those operations.

## Inputs and unchanged production source

The driver copied `helpers/guardian` and `scripts/guardian` from the frozen
`work/coverage-matrix-1` into private work directories and used private Cargo
targets. [The comparison manifest](continuous-probe/source-comparison.json)
records that every helper Rust source file and shared protocol source matched
the current worktree. No production helper source or process cleanup was edited.
The Python protocol fixture was copied unchanged; its helper `env={}` remains.

The expected compiled denominator is all 13 macOS helper source files plus
`../../../scripts/guardian/protocol.rs`. Linux-only `discovery_linux.rs` is
explicitly unmeasured, not covered or excluded to improve a score. The final
inventory is in [denominator.json](functional-coverage-verified/denominator.json).
All objects are exported without source exclusions, then their complete source
inventory is audited. Initial attempts using explicit filename filters omitted
the Rust `#[path]` protocol module because LLVM normalized the requested path
differently; those incomplete reports are also rejected.

The attempted full denominator contained 1,449 executable lines, 106 functions,
and 2,212 regions. Stable Rust emitted no branch mappings, so branch coverage was
unsupported in that run. Experimental branch instrumentation was separately
tried and also rejected for invalid counts.

## Continuous persistence works in small probes

Rust documents `%c` continuous profiling for Darwin, including preservation on
signals. LLVM documents page padding/alignment as a prerequisite for mapping
counter pages into the output file. [Rust instrumentation guide](https://doc.rust-lang.org/rustc/instrument-coverage.html),
[LLVM coverage guide](https://clang.llvm.org/docs/SourceBasedCodeCoverage.html).

The original Rust probe failed with a counter-section alignment error. A compiler
runtime filename symbol supplied an absolute `helper-%p%c.profraw` path, but that
alone did not align the sections. Apple Clang's driver adds alignment options;
Rust's ordinary coverage link did not. Adding these linker arguments for the
16,384-byte page size fixed that persistence failure:

```text
-C instrument-coverage
-C llvm-args=-instrprof-atomic-counter-update-all
-C link-arg=-Wl,-sectalign,__DATA,__llvm_prf_cnts,0x4000
-C link-arg=-Wl,-sectalign,__DATA,__llvm_prf_bits,0x4000
-C link-arg=-Wl,-sectalign,__DATA,__llvm_prf_data,0x4000
```

The linked C object supplies only the profile filename; it neither injects a
production environment exception nor changes normal process exit. Matching Rust
LLVM tools are required: Rust 1.98.1 uses LLVM 22.1.8, whereas the local Apple tools
use LLVM 21 and rejected the newer indexed profile version.

[Aligned fork/_exit probe](continuous-probe/aligned-execution.json): one parent-only
call, one child-only call, and one main entry were recovered after the child used
`_exit`. [Concurrent SIGKILL probe](continuous-probe/sigkill-execution.json): parent
and child each ran their own function 10,000 times and a shared function 10,000
times, then both died through SIGKILL. The retained counts were exactly 10,000,
10,000, 20,000 and main=1. Neither process used normal exit flushing.

These probes establish persistence and atomic updates for those particular
control-flow shapes. They do not establish valid inferred region counts for the
more complex helper control flow. A [single-process loop/SIGKILL probe](continuous-probe/loop-kill-execution.json)
also produced sensible counts, including zero for its deliberately unused arm.

## Why the full report is rejected

The ordinary continuous run's [export](functional-coverage-verified/export.stdout.txt)
contains inferred counts of **9,223,372,036,854,775,807** in guardian, sentinel, and
successor regions. The [status audit](functional-coverage-verified/measurement-status.json)
identifies every affected location. Its [raw physical counter dump](functional-coverage-verified/raw-counter-values.txt)
has maximum function/internal counts of 316,921/318,819, supporting the conclusion
that these enormous values arise during source-region expression evaluation,
not from billions of actual test iterations.

LLVM has an open report of continuous profiling with fork producing both enormous
counts and false uncovered regions. That report is consistent with this failure
class, but is not proof of the precise cause of every affected helper expression.
[LLVM issue 191788](https://github.com/llvm/llvm-project/issues/191788).

A test-only `pthread_atfork` profiling hook was investigated. Before fork it copies
the current counters. In the child it replaces only the profiling counter mapping
with a private copy, then asks LLVM to map a separate continuous file. It does not
alter helper cleanup or restore/rewrite source coverage counts. Copied execution
prefixes can legitimately be counted more than once; the intended question was
whether covered/uncovered region information became reliable.

The [private-profile probe](continuous-probe/private-execution.json) retained the
expected child/parent/shared counts and main=2 from the copied prefix. The full
helper run removed the successor saturation but still saturated guardian and
sentinel expressions. This experimental hook is preserved in
[private-atfork.c](continuous-probe/private-atfork.c); it is not linked into normal
helper builds and is not an accepted coverage method.

Neither Rust 1.95.0 with matching LLVM 22.1.2 nor Rust 1.98.1 experimental branch
instrumentation resolved the remaining saturations. The latter used
`RUSTC_BOOTSTRAP=1` solely for the scratch `-Z coverage-options=branch` experiment;
it was not a production build configuration.

| Attempt | Behavior fixtures | Coverage disposition |
| --- | --- | --- |
| [Page-aligned continuous](functional-coverage-verified/commands.json) | 5 units + 20 protocol cases pass | Rejected: saturated guardian/sentinel/successor regions |
| [Private child profiles](functional-coverage-private/commands.json) | 5 + 20 pass | Rejected: guardian/sentinel saturation remains |
| [Rust 1.95](functional-coverage-rust195/commands.json) | 5 + 20 pass | Rejected: saturation remains |
| [Experimental branches](functional-coverage-branch/commands.json) | 5 + 20 pass | Rejected: saturation remains |

Each attempt retains commands, exit codes, source manifest, compiler/native page
identity, raw coverage export, annotations and measurement rejection status.
Successful behavioral results do not override the failed measurement audit.

## Reproduction and next step

The [diagnostic driver](../../../scripts/guardian/coverage.py) now rejects saturated
region counters with a nonzero exit and records the offending regions. This is a
necessary sanity check, not a sufficient proof that every nonsaturated expression
is correct. Use fresh work/evidence destinations:

```sh
python3 scripts/guardian/coverage.py \
  --source-root work/coverage-matrix-1 \
  --work work/guardian-continuous-diagnosis-next \
  --evidence docs/verification/guardian/continuous-diagnosis-next
```

To reproduce the rejected private-child-profile experiment, additionally pass
`--fork-profile-hook docs/verification/guardian/continuous-probe/private-atfork.c`.
The driver intentionally remains diagnostic and is not added to the acceptance
gate as a passing coverage requirement.

A trustworthy next measurement needs either a corrected compiler/runtime method
validated against these actual helper paths, or independent direct basic-block
instrumentation that avoids inferred CFG subtraction. Such an alternative would
need its own source mapping/denominator audit; an edge or function-entry count
must not be relabeled as LLVM line/region coverage. Linux execution/instrumentation
remains unmeasured here. The helper functional source-coverage readiness gap stays
open, with no percentage accepted from these runs.
