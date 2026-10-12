# Passing assertions and the retained region denominator

Independent reviewer: performance_gate. This is a read-only investigation of
the reported zero region at `crates/domain/src/process/tests.rs:13`, using
actual exports and the actual compiled matrix binary. No tests, assertions,
compiler, denominator, or thresholds were changed; no builds were run.

Both `full-current-continuous02/workspace.json` and
`full-test-fault-matrix01/workspace.json` report the test
`commands_are_literal_bounded_and_redacted` with function count 3. Its line 13
condition subregions have count 3. A separate code region
`[13, 5, 13, 64, 0, 7, 0, 0]` uses filename alias 7, which resolves to the same
test source. Thus the zero is not evidence that the assertion was uncalled.
The same test also has zero full-call aliases for its other `assert!` calls,
and zero regions in the installed `assert_eq!` macro's `assert_failed` arm.

To challenge the panic-arm interpretation, disassembled the existing matrix01
domain test binary, SHA-256
`c45708e0cf7384954eb96d41f3341e029b6aeafd6b707651fea3bac853a8fe77`.
In that exact function, `contains` returns at address `0x100025c38`; a true
return branches at `0x100025c48` through `0x100025c5c` to `0x100026660`.
That block atomically increments a separate physical counter (offset `0xa0`
from this function's counter base) and calls `core::panicking::panic` at
`0x10002668c`. A false `contains` result follows the successful assertion path
to the next assertion. The installed `core/src/macros/mod.rs:51` separately
shows the `assert_eq!` failure branches explicitly.

Evidence is retained in ignored files
`work/coverage-closure/assertion-command-disassembly.txt` and
`work/coverage-closure/assertion-counter-audit.json`. The latter captures the
actual export region records and observed hashes of source, export, metadata,
binary and disassembly. This is existing native code evidence rather than a
new MIR build; it directly verifies the executed branch and physical counter.

Conclusion: the current all-selected-code region denominator includes real
assertion-failure arms. This example cannot receive a hit while its unchanged
original test passes: it requires the prohibited private marker to be exposed
and the assertion to panic. Calling another assertion or testing another
function's panic does not hit this function's counter. Catching/forcing this
original test failure merely for a counter would weaken its acceptance meaning.
Do not exclude these regions, change the assertion, manufacture a failing run,
or change the compiler to call the current 100% gate passed.

This establishes a concrete compatibility limitation between the retained
all-code region target and an all-passing unchanged test suite. It does not
classify every zero region as unreachable and does not excuse the remaining
production error paths. Continue to distinguish those useful coverage gaps
from these demonstrated assertion-failure counters, while reporting the
unchanged strict coverage gate as unmet.
