# Native snapshot corrections: domain-driven-design review

Independent DDD review of exactly two commits on
`native-page-capacity-admission`: `95710a9` "Reject snapshot page capacities a
native page cannot address" and `81960a2` "Begin continuation export at the
sequence still unfinished". Not a self-review. Organization/design patterns and
behavioral correctness are reviewed separately; the organization reviewer
explicitly handed the compatibility-marker question to this specialist, and
section 3 answers it.

What I verified by reading: both diffs in full; the 613-line
`scripts/native/patches/snapshot-pending-wrap.patch`;
`scripts/native/verify_source.py`; `scripts/native/patches/README.md`;
`scripts/native/checkpoint.c` and `owner.c`; `crates/infrastructure/src/terminal/`
(`mod.rs`, `state.rs`, `ffi.rs`); `crates/domain/src/terminal/mod.rs` and
`checkpoint.rs`; `crates/application/src/projection/native.rs:269-278`;
`crates/infrastructure/examples/terminal_corpus_support/runner.rs`; and the
corrected vendored sources under
`work/experiment-cache/ghostty-82232ecde55405559dec29c5466cb9e39938cb41/src/terminal/`
(`snapshot/page.zig`, `snapshot/continuation.zig`, `stream_continuation.zig`,
`stream.zig`, `PageList.zig`, `page.zig`, `size.zig`, `Parser.zig`,
`parse_table.zig`, `c/snapshot.zig`, `c/result.zig`). I traced the parse table
by hand for every byte the new `c1Introducer` names, and traced
`error.CapacityTooLarge` and `error.ContinuationLimitExceeded` from the Zig
error set through `c/snapshot.zig` and the C bridge into `TerminalError`.

What I did not do: no build, no `zig build test-lib-vt`, no `scripts/gate.py`,
no corpus run, no coverage run. I did not recompute any recorded SHA-256 and did
not re-derive the seed 201 or seed 509 failures. I read the preserved logs but
re-ran nothing. Nothing here converts the author's scoped evidence into
acceptance.

## Summary position

The dependency direction is correct in both corrections and neither commit adds
an edge that violates the layering in `coding_standards.md`. The problems are
not direction, they are **ownership and vocabulary**: one invariant (`a page
layout must fit `size.max_page_size``) now has two names and six independent
statements; one concept ("replay start") now has two incompatible definitions
under near-identical names; and the runtime compatibility marker is modelling
build provenance while the domain contract it satisfies says binary-format
identity. None of these is a crash today. All three are the kind of defect that
produces the *next* seed-509.

Separately, the review ledger for `81960a2` records a causal claim about the
Rust wrapper that the source does not support (finding D1). That one must be
corrected before the scope is called qualified, because the ledger is the
evidence.

## 1. Is the page-capacity check at the right boundary?

The check lives at
`work/.../src/terminal/snapshot/page.zig:593-597`, inside
`Header.pageCapacity` (`:570`), which is the snapshot decoder's factory for a
`terminal_page.Capacity`. The invariant it enforces is owned by the page/PageList
side, which states the same rule five other times:

- `PageList.zig:544` comptime `assert(layout.total_size <= size.max_page_size)`
- `PageList.zig:733` runtime assert, described as "redundant here for safety"
- `PageList.zig:4250-4252` `if (layout.total_size > size.max_page_size) return
  error.OutOfSpace;` — the growth path, which *does* return a typed error
- `PageList.zig:4307-4308` the same comparison again, as a projection guard
- `PageList.zig:4563` runtime assert in `createPage`

**The case for the codec.** `Header.pageCapacity` is already, by its own
doc comment, "Validate native allocation requirements and produce the page
capacity", and it already owns `CapacityError.InvalidDimensions`. It is the
anti-corruption layer: the one place where untrusted wire bytes become a domain
value object. Validating there is textbook — reject at the boundary, before any
allocation, so the domain never sees an impossible value. The alternative,
making `Page.init`/`createPage` return a typed error, is exactly what upstream
declined to do, and said so in the source: `PageList.zig:4560-4562`, "It would
be better to encode this into the Zig error handling system but that is a big
undertaking and we only have a few centralized call sites". Doing it in a
vendored patch would enlarge a hash-pinned six-file delta into an error-set
change rippling through `PageList`, for a repo whose stated policy is a fixed,
minimal, reviewable patch. And the check is placed correctly *within* the codec:
`Decoder.init` (`snapshot/page.zig:200`) runs it before `screen.zig:384` and
`history.zig:218` allocate, so both production PAGE paths reject before
allocation. I confirmed those are the only production consumers of
`decoder.capacity()`.

**The case against.** An ACL's job is to reject values the domain cannot
represent. It is not the ACL's job to know *how* the domain computes its own
address space. The patch makes the codec import `../size.zig` and evaluate
`terminal_page.Page.layout(capacity).total_size` itself — it now encodes the
page's internal addressing strategy. If `size.OffsetInt` ever widens, or `Page`
moves to per-member offsets, the codec's admission rule becomes silently wrong
(too strict or too lax) with no compile error anywhere. That is a domain
implementation detail leaking upward, and it is the classic symptom of an
invariant with no owner. The `Capacity` value object has no validating
constructor at all — it is a bare struct that anyone may fill in — which is why
the rule has to be restated at each of the six sites, with three different
behaviors (`assert`, `error.OutOfSpace`, `error.CapacityTooLarge`).

**Position.** The *placement* is acceptable and I would not ask for it to move.
The *duplication* is the defect, and it is a real one: this commit took an
invariant that was already stated five times and made it six, in a second file,
under a second name. The proportionate fix is small and stays inside the patch's
existing surface: put the predicate on the domain type once — `Page.Capacity`
gaining a `fits()` (or `Page.layoutChecked`) — and have both
`Header.pageCapacity` and `PageList.adjustCapacity` call it. The codec then asks
the page a yes/no question instead of re-deriving the page's addressing rule.
Recorded as D2.

Note also that `Decoder` does not keep the value it validated:
`snapshot/page.zig:200` evaluates `pageCapacity()` and discards the result with
`_ =`, and `:205` and `:224` re-derive it with `catch unreachable`. This is
"validate, then re-parse" rather than "parse, don't validate". It is sound today
only because `Header` is immutable between `init` and `capacity` and
`pageCapacity` is pure. It is worth flagging in this specific file because the
defect being fixed here *was* an assertion evaporating under `ReleaseFast`, and
`catch unreachable` is the same construct. Storing the validated `Capacity` as a
`Decoder` field removes the re-derivation and the `unreachable` together.
Recorded as N1.

## 2. Vocabulary, and error fidelity through the C boundary

**`CapacityTooLarge` is a second word for an existing concept.** The identical
condition already has a name in this codebase: `PageList.zig:4251` returns
`error.OutOfSpace` when `Page.layout(cap).total_size > size.max_page_size`. The
new `CapacityError.CapacityTooLarge` (`snapshot/page.zig:561-567`) names the
same predicate differently in a different layer. Two words for one concept is a
ubiquitous-language failure, and here it has a practical edge: a reader who
greps for `OutOfSpace` will not find the decoder's admission rule, and a reader
who greps for `CapacityTooLarge` will not find the growth path that already
enforces it. `CapacityTooLarge` is the better name of the two — it says what is
wrong rather than what ran out — so the resolution is to promote it, not to
adopt `OutOfSpace`.

**The continuation trimming does use the module's own vocabulary — and that is
the problem.** `ReplayStart`, `Tracker.replayStart`, "introducer", "committed",
"omittable" are all the module's established terms, which is right. But
`Tracker.replayStart` (`stream_continuation.zig:213`) and `findVTReplayStart`
(`:304`) now denote different things under names that differ by a prefix. See
section 4.

One smaller vocabulary point: `ReplayStart.c1: ?u8` (`:194-197`) mixes two
concerns in one value object — *where* replay begins (`index`) and *how the
introducer must be spelled on the wire* (`c1`). The spelling is an encoding
decision belonging to `write`; the location is the model. Two fields in one
struct is not a crisis, but the doc comment has to explain the encoding rule
inside a type named for a position. Recorded as N3.

**Error fidelity through the C boundary is lossy in a way the domain already has
vocabulary for.** The Ghostty result set (`src/terminal/c/result.zig:1-10`)
distinguishes `invalid_value` (-2), `limit_exceeded` (-6), `out_of_memory` (-1)
and the rest. The bridge discards most of it:

- `scripts/native/checkpoint.c:14-15`: `if (o->denied || w.overflow) return -2;
  return r ? -1 : 0;` — every non-success Ghostty code except a locally observed
  allocator denial or writer overflow becomes `-1`.
- `crates/infrastructure/src/terminal/state.rs:68-75` turns `-1` into
  `TerminalError::EngineFailure`.
- `scripts/native/checkpoint.c:25-32`: on the restore path everything except a
  local denial leaves `*error = -1`, and
  `crates/infrastructure/src/terminal/mod.rs:75-79` turns that into
  `TerminalError::CorruptCheckpoint`.

Two of these collapses are defensible and one is not.

`invalid_value -> EngineFailure` on the encode path is *correct* modelling. The
domain documents `EngineFailure` as "Native allocation or processing failed;
projection must be treated as failed" (`crates/domain/src/terminal/mod.rs:95-96`),
and an engine that cannot encode its own state is precisely that. The seed-509
defect presenting as `EngineFailure` was the right domain answer to a genuine
engine defect. Ghostty's `out_of_memory` is also handled correctly, though by
accident of layering rather than by mapping: every decoder allocation runs
through the bridge's bounded allocator (`checkpoint.c:25`, `owner.c:15-22`), so
an OOM sets `o->denied` and surfaces as `BudgetExceeded` before the result code
is consulted.

`limit_exceeded -> CorruptCheckpoint` on the restore path **is** a modelling
defect, and it is reachable by configuration alone rather than by corruption.
`mod.rs:69` passes `config.continuation_bytes` as
`GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES` (`checkpoint.c:26`).
`snapshot/continuation.zig:95` raises `error.ContinuationLimitExceeded` when the
record's payload exceeds it, and `c/snapshot.zig:500` deliberately maps that to
`.limit_exceeded` — the C API takes care to distinguish it. The bridge then
throws that distinction away, and a perfectly valid checkpoint restored under a
smaller continuation budget is reported to the operator as
`TerminalError::CorruptCheckpoint`. The domain already has the right word
(`BudgetExceeded`, "A configured buffer bound was exceeded",
`domain/src/terminal/mod.rs:93-94`), and the adapter already uses it correctly
for the analogous encode-side bound at `mod.rs:59-61`. This is a one-line
mapping gap, not a design problem — but it currently tells the truth backwards.
Recorded as D4.

The collapse has a second, evidence-level cost that matters more than the
message text. The corpus harness at
`crates/infrastructure/examples/terminal_corpus_support/runner.rs:121-130`
counts `CorruptCheckpoint | InvalidConfiguration | BudgetExceeded |
EngineFailure` as "rejected". Because the bridge maps clean rejection of
malformed bytes and genuine engine misbehavior to the same two variants, the
"101771 malformed mutations rejected" figure quoted in `81960a2` and in
`terminal-continuation-c1-introducer.md:98-103` is an oracle for "did not crash
and did not decode", not for "was rejected cleanly". The two defects found in
this scope were found because one segfaulted and the other aborted a corpus
seed; a third defect that merely returned `invalid_value` from a state it should
have handled would be counted as a success by this harness. That is worth
stating plainly next to the corpus counts. It does not invalidate the recorded
evidence, and it is not caused by these commits, but it bounds what the corpus
proves.

## 3. Does the COMPATIBILITY marker express the compatibility boundary?

No. It expresses build provenance, and the contract it fills asks for format
identity.

The domain field is documented as "Opaque engine and binary-format identity
interpreted only by its adapter" (`crates/domain/src/terminal/checkpoint.rs:42-44`).
The adapter's doc comment says "Binary compatibility includes the exact upstream
revision and adapter version" (`crates/infrastructure/src/terminal/mod.rs:12`),
which is notable for not mentioning the component that actually changes. The
value at `mod.rs:13` is
`ghostty-vt:<archive rev>:snapshot-wrap-<patch file SHA-256>:runtime-1`, and the
only field that moved in either commit is the digest of
`scripts/native/patches/snapshot-pending-wrap.patch` — a build input, not a
format.

The consequence is visible in this very scope, twice in one day:

- `95710a9` is **decode-side only**. `Header.pageCapacity` is reached from
  `Decoder.init` and `decodePayload`; the encoder path builds its `Header` from
  a live page at `snapshot/page.zig:548-558` and never calls it. No snapshot
  this build writes differs by one byte from what the previous build wrote, and
  no snapshot the previous build wrote becomes undecodable. Rotating the marker
  discarded every existing checkpoint for zero contract change.
- `81960a2` **does** change emitted bytes, but only in the case where the
  previous build *failed to emit anything at all*. Any checkpoint the previous
  build actually produced carries a continuation that the new decoder validates
  identically, because `stream_continuation.validate` is unchanged. So this
  rotation also discards checkpoints that would decode correctly.

`scripts/native/patches/README.md:90-95` justifies the marker as ensuring
"checkpoints from the uncorrected codec do not silently cross this compatibility
boundary". For `95710a9` that claim is not true in the direction it implies:
there is no old checkpoint the new decoder would mis-handle, and the new
decoder's own validation is what protects the boundary in `81960a2`. The marker
is doing defence-in-depth against a risk that is already covered, and paying for
it with every live session's state on every native correction.

I am not asking for the rotations to be reverted — pre-release, the cost is
bounded, and "when in doubt, invalidate" is a defensible operating posture. The
defect is that the model has one field doing two jobs and the documentation
asserts the job it is not doing. The correct shape is the one the organization
reviewer sketched and I concur with: a hand-assigned codec generation that moves
only when the encoded or replayed contract actually moves, with the patch digest
carried alongside as provenance for the build stamp and CI cache keys, where it
already belongs (`verify_source.py:38`, `:105`). Until that exists, `mod.rs:12`
and `README.md:90-95` must at least stop describing a provenance stamp as a
format identity. Recorded as N5. I also concur with the organization reviewer's
P2 that nothing mechanically checks `mod.rs:13` against
`verify_source.py:38`; from a DDD standpoint that is the same defect seen from
the other side — the identity has no single owner, so it is replicated by hand.

## 4. Is "replay start" coherently modelled?

Separation of *concerns* here is legitimate. Duplication of the *invariant* is
not, and it is already partial.

The feed path must be cheap: `findVTReplayStart` (`stream_continuation.zig:304`)
is a backward SIMD scan for the last ESC, called once per feed from
`Tracker.append` (`:154`). The export path may be expensive: it runs once per
snapshot and can afford a forward parse. Keeping them apart is right, and the
patch is explicit that the feed path and its SIMD scan are untouched. That part
of the design is sound.

The problem is that the two now answer *different questions* under names that
differ only by a prefix:

- `findVTReplayStart(input) -> ?usize` = index of the last ESC.
- `Tracker.replayStart() -> ReplayStart` (`:213-244`) = index of the last byte
  that actually introduced the sequence the parser is still building — ESC, or a
  C1 byte whose processing genuinely moved the parser into that introducer's
  entry state (`:231-236`) — but only when the retained bytes would otherwise
  replay committed work (`:242`), and carrying an instruction to re-spell the
  byte.

And a third party depends on both: `validate` (`:62-66`) decides canonicality
with the ESC-only `findVTReplayStart` and rejects anything that does not begin
at index 0. So `write` produces under one rule what `validate` accepts under
another. Today they agree, and I traced why: `replayStart` scans forward and
overwrites `found` at every introducer, so it returns the *last* one; therefore
no ESC survives after the trim point, and `write` (`:271-280`) always emits ESC
at byte 0, either the original or the synthesized `ESC` + `c - 0x40`. That
agreement rests on an unstated invariant — "`replayStart` returns the last
introducer, so the exported suffix contains no earlier canonical start" — that
is written down nowhere and asserted by no test. A future change that returns,
say, the *first* introducer after the last committed byte (a plausible move, to
retain more context) would keep every existing unit test passing and reintroduce
seed 509's exact failure mode: `write` emits, `validate` rejects, the snapshot
fails.

This is the same class of mismatch the commit message itself names as the root
cause — "the model mismatch that caused this" — and the correction reproduced
the shape rather than removing it. `terminal-continuation-c1-introducer.md:59-60`
claims "export, validation, and the feed path share one model". They share the
`BoundaryScanner`; they do not share a definition of the canonical start.

The minimum acceptable resolution is cheap and does not require restructuring:
state the invariant in the source at `:213` and at `:62`, and turn the
agreement into a checked property. `stream.zig` already asserts
`continuationpkg.validate(writer.buffered())` for the four new C1 cases and for
the inert-prefix case, and the pre-existing "continuation reconstructs every
unfinished VT state" table (`stream.zig:5026-5084`) covers all thirteen parser
states but does *not* call `validate` on the exported bytes. Extending that
existing table with `try continuationpkg.validate(writer.buffered())` converts
the implicit coupling into an asserted invariant across every state, for one
line. The better long-term answer is that `validate` and `replayStart` call one
canonicality function, so the definition cannot fork at all.

I also confirmed, by reading `parse_table.zig:56-86` and `:111-120` together
with `Parser.next` (`Parser.zig:257-315`), that the `before != state and entry ==
state` guard at `:232-234` is correct rather than merely plausible: `0x98`,
`0x9e`, `0x9f` inside `sos_pm_apc_string` are self-transitions, and
`Parser.next` suppresses both the exit and the entry action when
`self.state == next_state`, so those bytes emit no `apc_end` and no `apc_start`
and genuinely introduce nothing. The same holds for `0x9b` inside `csi_entry`.
Excluding them is right. But `c1Introducer` (`:246-255`) is a hand-copied
duplicate of `parse_table.zig:68-85`, in a different file, with nothing
asserting the two agree — a fourth restatement of knowledge the table owns. A
single test iterating `0x80..0x100` and comparing `c1Introducer(c)` against
`table[c][@intFromEnum(State.escape)]` would pin it. Recorded as N6.

## Findings

Must be resolved before this scope is called qualified:

**D1 — P2 — the review ledger records a mechanism the source does not have.**
`docs/archive/reviews/terminal-continuation-c1-introducer.md:16-18` states that
`state.rs` converts the encode failure to `TerminalError::EngineFailure` "while
setting `failed = true`, so the terminal can never be checkpointed again", and
`81960a2`'s commit message repeats it ("The Rust wrapper latches the terminal as
failed, so it never checkpoints again").
`crates/infrastructure/src/terminal/state.rs:47-78` does not set `self.failed`
on the checkpoint path; only `mod.rs:123-133` (`mutation`, reached from `feed`
and `resize`) and `state.rs:104-121` (`restore_history_step`) latch it, and
`state.rs` is unchanged since `f9da4e1`. The real blast radius is that
checkpointing fails deterministically while the parser sits in that state; the
terminal keeps feeding, resizing, projecting, and can checkpoint again once the
sequence completes. That is a materially smaller failure than the one recorded.
`coding_standards.md` requires findings with file/line evidence; an incorrect
causal claim in the ledger is a defect in the evidence itself. Correct the doc.

**D2 — P2 — the `max_page_size` invariant now has six statements, two names, and
no owner.** `snapshot/page.zig:593-597` (`CapacityTooLarge`) versus
`PageList.zig:4250-4252` (`OutOfSpace`), plus asserts at `PageList.zig:544`,
`:733`, `:4563` and a fourth comparison at `:4307`. Upstream's own comment at
`PageList.zig:4560-4562` says this should be a typed error rather than an
assert. Either add the predicate to the domain type once
(`Page.Capacity.fits()` / `Page.layoutChecked`) and call it from both the codec
and the growth path, or record this explicitly as an accepted deviation with the
reason (patch minimality against a pinned archive) so it is a decision rather
than an omission. Silently shipping a sixth restatement is the option I am
rejecting.

**D3 — P2 — "replay start" is defined twice and the coupling is unstated and
untested.** `stream_continuation.zig:304` versus `:213-244`, with `validate`
(`:62-66`) depending on the first while consuming the output of the second. See
section 4. Minimum: document the invariant at both sites and add
`try continuationpkg.validate(writer.buffered())` to the existing thirteen-state
table at `stream.zig:5026-5084`. Preferred: one shared canonicality function.

**D4 — P2 — a configured continuation budget is reported as data corruption.**
`error.ContinuationLimitExceeded` (`snapshot/continuation.zig:95`) is mapped to
`.limit_exceeded` with deliberate care at `c/snapshot.zig:500`, then discarded
by `scripts/native/checkpoint.c:30` and rendered as
`TerminalError::CorruptCheckpoint` at
`crates/infrastructure/src/terminal/mod.rs:75-79`. The domain has
`BudgetExceeded` for exactly this and the adapter already uses it correctly at
`mod.rs:59-61`. Pre-existing, not introduced here, and in scope only because
this review was asked to evaluate the collapse — but it is a one-line mapping
and it currently mislabels a configuration problem as corrupt data. If it is not
fixed in this scope, record it as a known gap next to the corpus counts, along
with the harness consequence in section 2.

Worth noting:

**N1** — `snapshot/page.zig:200` discards the validated capacity with `_ =` and
`:205`/`:224` re-derive it with `catch unreachable`. Parse-don't-validate: store
the `Capacity` on the `Decoder`. Using `catch unreachable` to carry an invariant
in the same file whose defect was an assertion evaporating under `ReleaseFast`
deserves a second look even though it is sound today.

**N2** — the codec now imports `../size.zig` and evaluates
`Page.layout(cap).total_size` itself (`snapshot/page.zig:593-594`), so a change
to `size.OffsetInt` or to `Page`'s member layout silently invalidates the
decoder's admission rule with no compile error. This is the concrete cost of
D2's duplication, stated as a risk rather than a style point.

**N3** — `ReplayStart` (`stream_continuation.zig:190-198`) carries both a
position (`index`) and a wire-encoding instruction (`c1`). The re-spelling is a
`write` concern; the type is named for the position.

**N4** — the new capacity test (`snapshot/page.zig:1363-1428`) drives
`decodePayload`, which is not the production entry point. Production reaches the
guard through `Decoder.init` (`:200`) from `screen.zig:384` and
`history.zig:218`. The guard is the same function so the behavior is the same,
and seed 201 exercises the real path, but the unit test does not prove the
production path rejects before allocation. (The organization review separately
covers the mislabelled and tautological "accepted" case at `:1411-1427`; I
concur and do not restate it.)

**N5** — `mod.rs:12` and `scripts/native/patches/README.md:90-95` describe the
compatibility marker as a binary/format identity, and
`crates/domain/src/terminal/checkpoint.rs:42-44` asks for one, but the value is a
patch-file digest. Neither correction in this scope changed the snapshot
contract, and both rotations discarded every existing checkpoint. Recommend a
codec generation moved by contract changes only, with the patch digest kept
alongside as provenance. I concur with the organization reviewer's P2 that the
literal is additionally unchecked against `verify_source.py:38`.

**N6** — `c1Introducer` (`stream_continuation.zig:246-255`) hand-copies
`parse_table.zig:68-85`. Add a test over `0x80..0x100` comparing it to the table
so upstream drift fails loudly rather than silently changing which sequences get
trimmed.

## Done well

The dependency direction is right and worth saying so explicitly, because it is
the thing this review exists to check. The page-capacity rejection happens in the
adapter's own anti-corruption layer before any allocation, and it produces a
typed error already inside `PayloadDecodeError`, so no error set was enumerated
by hand. Nothing native leaks upward: `TerminalError`
(`domain/src/terminal/mod.rs:88-108`) still carries no errno, no Ghostty enum,
and no payload, and `TerminalCheckpoint`'s bytes stay opaque with a redacted
`Debug`. The continuation correction is placed on the export path only, leaving
the per-feed hot path and its SIMD scan untouched, and it reuses the
`BoundaryScanner` that validation already uses rather than adding a fourth VT
model. `terminal-continuation-c1-introducer.md:76-79` reasons explicitly about
the one-byte-to-two-bytes growth and why the export can never exceed the
retained length — that is the right kind of argument to have written down, and I
checked it and agree.

## Verification limits

This is a source architecture and boundary-contract review. No build, gate,
native test, corpus, performance or coverage run was performed by this reviewer;
the coordinating agent owns the review-loop gate and the evidence ledger. I did
not verify any recorded SHA-256 and did not confirm that the vendored sources I
read are byte-identical to what the recorded logs were produced from — I read
the working tree cache under `work/experiment-cache/`. Behavioral correctness of
the trimming rule across the full parser state space, concurrency, resource
bounds, and the Linux gate remain with their own specialists. No P1 blocker was
found in the inspected scope; the four P2 findings above are unresolved and no
milestone acceptance follows from this review.
