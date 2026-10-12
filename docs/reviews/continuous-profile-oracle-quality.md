# Independent quality review: continuous profiling across abrupt child death

Latest status: the collected V9 surviving-parent cases pass independent audit.
The additive all-process-death oracle has passed pre-execution review only.
Chronological design findings, fixes and evidence follow.

Pre-execution verdict: **no unresolved P1/P2 found in this fixture/specification/
validator scope**. This review does not report a compiled or executed pass.

Reviewed ignored inputs in `work/counter-integrity/oracles`:

| Input | SHA-256 |
| --- | --- |
| `continuous_fork.rs` | `0d950f4630a391c72f99176378d58d125f17e39ce7fd4fe61b90b9698dae6ec6` |
| `continuous_fork_expected.json` | `404be33ca2d0532e4ff6bfd73cb29d7fbe841976b2b94bb604d0db23eb7a18ca` |
| `validate_continuous_fork.py` | `1a3a7ca3efc07e3f262e4027a000a0af728f2f17ad978089efe9476ef590e592` |

The fixture uses public process, pipe, signal and wait APIs without profiling
flush/reset hooks. Each process executes 20,000 calls to a non-inlined function
and asserts the independently recomputed checksum 3,399,890,000. The expected
40,000/40,000/60,000 aggregate calls follow from parent plus one child in the
`_exit`/self-SIGKILL cases and parent plus two children in the group-SIGKILL case.
They do not derive from measured coverage counts.

Each child signals readiness only after completing its workload and flushing
its observations. The group leader stays alive while the second child joins
its fresh process group, preventing group-ID reuse before the parent sends
SIGKILL. The parent reaps its exact child PIDs and checks raw Darwin exit/signal
statuses. The validator rejects missing/extra observations, incorrect readiness
ordering, unexpected exit status, missing source mappings, incorrect positive
or zero counts, ambiguous function identity and saturated regions. It compares
all declared observations and selected function-entry multiplicities against
the full export, without exclusions or replacement zeros.

The initial draft could leave paused children after parent failure. The revised
fixture sets fatal 30-second child and 45-second parent alarms before setup.
With the normal default signal disposition, an orphan cannot pause indefinitely;
expiry produces an unexpected status and therefore fails the run. This closes
the identified cleanup gap. The external runner must still enforce its own
timeout and record expiry as failure without replacing the sample.

Before execution acceptance, independently verify exact compiler/wrapper/linked
backend/builder identities, unchanged fixture inputs, v0 symbol naming, the
selected binary and flags, mandatory atomic increments, Darwin profile-section
alignment and continuous `%p%c` configuration. Use a fresh directory for each
case, retain all profiles and diagnostics, merge the complete observed profile
inventory, and bind the export to its exact executable. These setup obligations
are outside the validator's scope; a validator pass alone cannot prove them.

The tested persistence boundary is abrupt child death while the parent remains
alive to verify statuses and finish. It does not prove persistence after every
process mapping the profile dies simultaneously. It also does not establish
other platforms, arbitrary signal dispositions or peak/concurrent performance.
No compiler, test binary or benchmark was run by this reviewer; review comprised
source inspection, input hashing and independent checksum arithmetic.

## Surviving-parent V9 collection: independently audited scoped pass

Evidence: `work/counter-integrity/root-continuous-v9-evidence` and
`root_run_continuous_v9.py`. The coordinating task ran each of the three cases
once. The reviewer ran no executable and independently checked the retained
runner, inputs, profiles, binary metadata and exports.

All linked compiler/driver/builder/backend and fixture hashes match; the runner
records unchanged inputs. Each fresh case directory has exactly the one profile
listed in its complete manifest, with matching hash, and the selected executable
hash matches. All three validators pass every named source observation and
function multiplicity: `shared_hit` is exactly 40,000 / 40,000 / 60,000; child
and reap entries are 1 / 1 / 2, and workload entries 2 / 2 / 3. Execution stderr
is empty. The fixture's exact child wait-status assertions remain active.

The runner clears inherited coverage configuration and links a C object that
only defines the `%p%c` profile filename data symbol; it adds no profiling exit
or fork hooks. The reviewer inspected the actual Mach-O section headers:
`__llvm_prf_cnts`, `__llvm_prf_data` and `__llvm_prf_bits` have alignment 2^14,
matching the recorded 16,384-byte host page size. Inspection of the selected
binary's `shared_hit` disassembly confirms atomic `ldadd` counter increments.
This is a pass for the declared surviving-parent `_exit`, self-SIGKILL and
group-SIGKILL persistence cases, not for all-process death or full coverage.

## Additive all-process-death oracle: pre-execution review

The separate `continuous_all_dead` fixture closes the surviving-parent scope
limitation prospectively. It checks that its operator-created session has
PID = PGID = SID before signaling any negative process-group ID. A lone case
self-kills after 20,000 calls. A group case kills parent and two ready children
after 60,000 aggregate calls. Each short identity/observation record is one
atomic pipe write. An uninstrumented operator requires parent return code -9,
records the profile inventory only after all group members disappear, and never
sends a cleanup signal on its success path. Any timeout is marked failure before
cleanup, so operator intervention cannot become a passing sample.

The initial watchdog ordering had a P2 false-positive path: child alarms at 30
seconds could fire before the parent's 45-second deadline, after readiness, and
the parent could subsequently SIGKILL itself while the validator incorrectly
attributed all deaths to the group kill. Unlike the earlier fixture, no surviving
parent can reap and inspect every child's status here. The fix sets the parent
deadline to 20 seconds, before every child's 30-second alarm. With normal signal
dispositions, a child watchdog expiry cannot now yield an accepted parent -9.
Independent re-review closes this finding; no remaining P1/P2 was found in the
pre-execution fixture/operator/validator scope.

| Frozen additive input | SHA-256 |
| --- | --- |
| `continuous_all_dead.rs` | `343489eced9ea2801ab775da5b71790a9d63b9a0950e323b54205ddce8b91f5a` |
| `continuous_all_dead_expected.json` | `65eafa598ecbe078bc6c75f217a752df4bc258fba86e4317b10d707f5462e84c` |
| `run_continuous_all_dead.py` | `0cf687b51a5ec3b06511542d1f9ef2d103dcb8f146ba87e9210ee48b51897796` |
| `validate_continuous_all_dead.py` | `e9707af6d8b279cae9958ebcde0881447eb98746522fd6e6fa5087245dcb1729` |

This is a design review, not an execution pass. The prior fixture remains
unchanged. Actual linked-input provenance, section alignment, atomic updates,
fresh complete profile inventory and export binding still require independent
audit after collection. No test, build or timing run was performed by this
reviewer.

## All-process-death V9 collection: independent evidence audit

**Scoped PASS.** Independently audited `root_run_all_dead_v9.py` and
`root-all-dead-v9-evidence`, including the raw profiles, full exports, validator
records, process observations, actual Mach-O binary, and linked compiler inputs.
No build, test, timing run, or fixture execution was performed by this reviewer.
The original four frozen additive input hashes above still match; all 22
collection input hashes and the output binary hash independently match. The
wrapper links the recorded V9 MIR builder and V8 LLVM backend. Their manifests,
six native shim object/archive hashes, and the selected LLVM library hash also
match. The compiler remains Rust 1.98.1, commit
`48a229ceaefd4985c50990b14116b6d856af0985`, LLVM 22.1.8.

| Case | Process evidence | Raw `shared_hit` entry / both physical block counters | Workload / child entries |
| --- | --- | --- | --- |
| lone | PID/PGID 38803, parent status -9, empty group after wait | 20,000 / 20,000 / 20,000 | 1 / 0 |
| group | Parent/PGID 38817, children 38818 and 38819, parent status -9, empty group after wait | 60,000 / 60,000 / 60,000 | 3 / 2 |

Both runs have empty stderr and exact public observation counts, including zero
post-kill observations. Each process verifies the checksum 3,399,890,000 before
its completion observation. The group parent consumes both readiness bytes
before its final observation and group SIGKILL. The independent operator uses a
fresh session and observes group disappearance before reading profile files;
there is no success-path cleanup signal or profiling hook. Neither timeout nor
watchdog failure was accepted. This retains the previously reviewed watchdog
ordering and its normal-signal-disposition assumption.

Each fresh profile directory contains exactly its one recorded 49,152-byte raw
profile, with no omitted file. Independent rehashing matches:

- Lone: `c7be8135c482f390b6e8e1a39606294e6b97cc2645a0dce9d109f63e913172de`.
- Group: `e3c2eb3d4f170b809a84fc2cfcf79d0412db4bcd6f3e9fdf241f24fa99f0cb4d`.
- Selected oracle executable: `4606d85ed1242853fe99930ce79482789096444480dfab5a2bf0cc70683a05f9`.

The operator supplies `%p%c` through `LLVM_PROFILE_FILE`. The collection uses
instrumented v0 symbols, optimization level zero, atomic counter updates, and
Darwin section alignment flags. Actual `__llvm_prf_cnts`, `__llvm_prf_data`, and
`__llvm_prf_bits` sections have alignment 2^14, matching the 16,384-byte host
page size. The selected executable's `shared_hit` disassembly uses `ldadd` for
all three counters; `llvm-profdata show` on each raw profile confirms the counts
in the table directly. The complete-profile merge and export commands bind the
same executable, and the source observation/function validator reports no
missing mapping, unexpected hit, or saturated region.

No P1/P2 remains in this collection's declared persistence scope. These results
extend persistence evidence to lone SIGKILL and simultaneous parent/two-child
group SIGKILL after completed work, with every mapped process dead. They do not
establish power-loss durability, arbitrary interruption timing, other platforms,
or general compiler mapping correctness. V9's separate unsupported macro-root
finding and full coverage acceptance remain open.
