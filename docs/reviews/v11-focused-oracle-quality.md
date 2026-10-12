# V11 focused oracle: independent pre-execution review

## Verdict

**Design review PASS in the declared scope; no execution or compiler acceptance
claim.** Independently reviewed the additive ignored control-flow and formatting
fixtures, expected observations/counts, and validator authored by the separate
adversarial reviewer. No P1/P2 was found in the frozen pre-execution design. This
reviewer did not build or execute Rust, run tests, or alter the fixture.

| Frozen input | SHA-256 |
| --- | --- |
| `v11_control_flow.rs` | `60f2dfba245ded740a466f6a527db9fb5ec65b79df54818d2042283405884635` |
| `v11_formatting.rs` | `5dc5defb441688ecdea6c368a9166d062d9d31c83b1a2d589377a3415f2f8dca` |
| `v11_expected.json` | `16726aff021de13b7a16a744225e1c4e886895a99440b6af97989b7373e22638` |
| `validate_v11.py` | `8518afc58d325534d5b504bdb1a70162e9c6e4b1e274786699b38fb9259a4804` |

## Independent observation basis

The control-flow fixture independently asserts return sums 15 and 60 and an
atomic total of 16 ordered observations. Those observations distinguish the
single-variant live arm, three first-guard and two second-guard evaluations,
normal destruction, panic unwinding, and destruction after one pending future
poll. The common destructor observer executes three times. Dead-arm,
post-panic and post-await tokens must remain mapped with count zero; missing
mapping is not treated as zero.

The formatting fixture independently asserts successful rendered strings and
13 ordered atomic observations. It includes nested dynamic and inlined-literal
expressions before a later panic and the mirrored ordering after an earlier
panic. These distinguish one from zero entry at the same kind of nested syntax.
All four interrupted render dispatches remain zero. A named-argument case checks
source argument order even though placeholder order is reversed. Successful
render dispatch/body counts are three; the shared argument-panic observer count
is four. Both `concat!` and nested `stringify!` in the doc attribute require true
metadata provenance rather than a fictitious runtime count.

The validator checks every character of each specified token at its exact source
anchor, including nested macros sharing one source line. It requires exact stdout
order, process status, a unique physical-file view, nonsaturated counters, one
instrumented executable-compilation summary, unique expansion identities and
resolved-to-terminal reconciliation. Every runtime terminal expectation requires an emitted marker and a mapping;
a typed format-consumption receipt cannot satisfy that requirement or waive
the independent expected source count. Attribute metadata has a distinct
required reason and typed target evidence. Unknown/missing mappings and
unsupported terminals fail.

## Review changes and execution requirements

During review, the author added the nested attribute `stringify!` terminal
requirement, retained the public Drop-observation scope limitation, and corrected
a comment so typed consumption must retain the expected count, including zero.
The parent-requested mirror-order and named-argument cases were reviewed in the
final frozen version. No unresolved fixture finding remains.

The eventual runner must use fresh profiles and a complete provenance-directory
inventory from exactly one executable compilation per validator invocation,
bind every source/spec/validator/compiler/binary input and all output hashes,
and preserve failures. The validator cannot authenticate completeness of an
arbitrary externally supplied subset of JSONL files by itself. Actual compilation
and independent collection audit remain pending.

Destructor method observations alone do not prove a source marker is located in
the caller's unwind or coroutine-drop basic block. Acceptance of V11's reachability
repair also requires inspecting the affected marker/CFG evidence, including all
real cleanup/unwind/Yield-drop successors and excluding only imaginary edges.
Passing these fixtures would not establish the entire canonical coverage matrix,
all formatting transformations, or general compiler correctness.

## Final validator delta re-review

The final validator replaces its earlier hash
`6273ced1c2e65581e96312bce45e21fb3ac93e0eb89f18abd49ef54d20bd4ee1`
with the hash in the freeze table. Both Rust sources and the expected-count spec
are unchanged. Genuine attribute metadata now requires the explicit terminal
status `attribute_value_lowered_as_metadata` and the same typed consumption
reason/target evidence. Every runtime terminal expectation now requires
`runtime_entry_mapped` plus its emitted marker and mapping; the prior alternative
accepting a typed runtime-consumption status was removed. Exact per-character
source-count checks remain intact, including zero-count mappings.

Independent source re-review finds no weakened invariant or new P1/P2. The saved
synthetic check report covers complete cases and rejection of absent zero-count
mappings, wrong exits, duplicate terminals, missing runtime receipts and wrong
metadata classification. This report is parser evidence, not Rust execution.
The updated freeze passes pre-execution design review; the collection and
compiler acceptance requirements above remain pending.

## Eager metadata receipt parser re-review

The validator freeze advances from
`115f362c5f2bbb3abe5efd939834177c19e42859560f98b5ba819e2867f94fb9`
to `8518afc58d325534d5b504bdb1a70162e9c6e4b1e274786699b38fb9259a4804`.
The preceding validator/freeze is preserved under `oracles/v11-freeze-v1`.
Both fixture sources and the complete expected-count specification are unchanged.

This narrow schema correction separately reconciles successful eager invocation
receipts with eager terminal receipts. It rejects duplicate/missing identities,
overlap with AST identities, changed parent/callsite/kind, and invented AST roots.
For eager attribute metadata it follows exact recorded parent IDs to a real AST
terminal, rejects cycles, and requires the same typed metadata consumption
receipt. Eager records cannot satisfy any runtime terminal expectation. The
original AST reconciliation and every source-count obligation remain unchanged.

The reviewer inspected the concrete formatting receipt: `expn17` at line 33,
columns 30–55 is the `stringify!` invocation, with explicit eager parent `expn3`.
Its metadata terminal points to that parent and the attribute parser's consumed
root `NodeId(164)`, target `annotated` HirId owner/local zero, and metadata reason.
This matches the pinned compiler's eager-versus-monotonic callback distinction;
it does not manufacture an AST callback that never occurred.

No P1/P2 was found in this parser delta. The updated schema freeze passes review
for the existing two focused fixtures. No Rust build/execution or new case was
added by the reviewer. Fresh collection validation and the compiler's broader
acceptance remain separate.
