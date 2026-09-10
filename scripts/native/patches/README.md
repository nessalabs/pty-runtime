# Reviewed Ghostty snapshot correction

`snapshot-pending-wrap.patch` applies only to Ghostty revision
`82232ecde55405559dec29c5466cb9e39938cb41`, whose original archive SHA-256 remains
`820d84e8cbc4be0ca26b9a1b71cfdc5befdc814c414bef7b0dbcf51069668c68`.

The decoder preserves active and saved cursor pending wrap when the original
coordinates are in range. A right margin or a preceding resize can leave pending
wrap away from the final screen column. Clipped coordinates retain the previous
last-column normalization. The patch changes the native normalization table and
adds a saved-cursor table test for valid and clipped positions.

The same patch also makes non-reflow width growth clear row wrap and continuation
flags when the existing page has spare capacity, matching the wider replacement
page path. This prevents page allocation capacity, which snapshots need not
preserve, from changing later cursor and grid behavior. A native test exercises
both capacity paths; public tests cover alternate-screen formatting and primary
screen reflow followed by input and a cursor-position query.

Non-reflow shrink also clears the base of a wide glyph when the cutoff removes
its spacer tail. The existing clear operation releases associated managed cell
state. Native tests cover widths 1 and 5, adjacent narrow content, and later growth;
public tests cover both screens and styled/hyperlinked combining content before
checkpoint, restore and later input.

Before trailing blank-row trimming, an inactive internal viewport pin is moved
to the first history row. Its incidental restored position no longer retains
blank rows and pushes live text into history during shrink. A real pinned
viewport and external tracked pins keep their existing retention behavior.

The bitmap allocator also checks that a final partial word exists before reading
it. Requests beyond capacity now return allocation failure with existing contents
and accounting intact. The independent allocator tests retain the failing and
passing evidence in `docs/verification/bitmap-capacity-independent`.

Continuation export also begins at the sequence the VT parser is still
building. The feed path records the last ESC as the replay start, but a C1
introducer abandons the current sequence too, so the retained bytes could still
open with a sequence that had since completed. Replaying that completed
sequence is committed work, which export cannot omit because ending a sequence
changes a parser state tag, and the encoder's own validation then rejected the
suffix and failed the whole snapshot. Export now trims to the final introducer
and spells a C1 introducer as `ESC` plus the byte minus `0x40`, since replay
starts from ground where a raw C1 byte is not an introducer. Trimming applies
only when the untrimmed bytes would replay committed work, so an inert
abandoned prefix still exports byte-identically. Native tests cover the
reachable introducers after an unfinished APC string, the re-export identity,
and the unchanged inert case.

Decoded page bytes are additionally held to a running total across the whole
decode, `PageList.max_decode_bytes`, checked before each page is created rather
than after the memory is taken. A capacity records what a page allocated, not
what it holds, so a blank wide page and an over-declared one are identical on
the wire and no payload-derived bound is both tight and non-rejecting. The
remaining control is to bound the resource. The value, its placement as a
constant rather than a decoder option, and the refusal being `OutOfMemory` are
judgements recorded in `docs/archive/reviews/terminal-page-admission.md`.

PAGE admission also bounds what a header can make a decoder allocate. The
capacity ceiling is a fixed 64 MiB, the shape the Kitty graphics decoder uses
for declared image sizes, because page memory comes from `mmap` and no
allocator limit observes it. The declared row count must additionally be paid
for by the record: every row costs `grid.row_header_len` bytes on the wire even
when entirely default, so a count the payload cannot support is one no encoder
produced. An earlier attempt bounded the running total by the scrollback budget
instead and refused legitimate alternate screens; that is reverted and recorded
in `docs/archive/reviews/terminal-page-admission.md`.

PAGE decoding also rejects a header whose advertised capacity needs more page
memory than a native page can address. Page creation only asserts that the
layout fits `size.max_page_size`, which release builds drop, so an untrusted
capacity above the limit truncated the layout arithmetic: the page was
allocated for the truncated size while its string allocator still handed out
the advertised capacity past the end of that memory. The completed layout is
now measured before anything is allocated, and a native test covers the string,
grapheme, and dimension capacities that each exceed the limit on their own.

| Input | SHA-256 |
| --- | --- |
| Patch | `745b5e98703259704c7ad9a5c2e1357817efda85cb6f60cd3db730d6d42f4947` |
| Original `src/terminal/snapshot/screen.zig` | `abc550e1b8cbee843f2ee5b2168602aff1ef66b11368f5c38394b7b27704fca7` |
| Corrected screen source | `4ae17bd3be6851083d4e60f8378ece70f6910a6e9de75a2fdfa1c9afb3beb819` |
| Original `src/terminal/PageList.zig` | `cd926e56749c014a8df7f30fff1f5c32548cb4a8731451e726d7171d25818fcb` |
| Corrected PageList source | `a7703d31bfc95c68446e466ba3cf5cc329bed405e3527fe5503d344547e50462` |
| Original `src/terminal/bitmap_allocator.zig` | `bac61a65b5a3141ccfad2d9d0a6a452be7106a647182470fcf38e1289b5f86e1` |
| Corrected bitmap allocator source | `32673b2a73f1cf5135fbb1aa4f07855bff6a0c3e42789a178e9ee93da046debc` |
| Original `src/terminal/snapshot/page.zig` | `2e58c7f15983cd365fc3b7f1aa7e28b515f3fe40654e950c1175761ed2acca6e` |
| Corrected snapshot page source | `3d10248e58c4b4019ff463e6bcc829fe808e82b9c88a448433d4cc7a178f5052` |
| Original `src/terminal/stream_continuation.zig` | `a86feef9e53dc62349e64ddb6d25f1d6d971b9813578a24e915e39daeb39a9a2` |
| Corrected stream continuation source | `8d36a4991ce7a9432857e2d12052d5f212c0faac619ff3e38901b11ada726734` |
| Original `src/terminal/stream.zig` | `1cb5d8b8f6493e8264fd1cd027821a36f0a6f88aae5e1eeeb291affdcd1ca6d6` |
| Corrected stream source | `7e2f63d504bdf558de243b837017a834a951faeecbe7691218a217b11d75f6bc` |

`experiments/run.py` calls `scripts/native/verify_source.py --prepare` after
verifying/extracting the original archive, before selecting or rebuilding the
library. The verifier checks every existing archive native input; only these
eight source files may be in their recorded original or corrected states. It
patches temporary copies of the originals and checks all results before writing
corrected source. This also upgrades an existing cursor-only patched cache.
Repeated preparation accepts the exact corrected files without reapplying them.
Preparation alone also accepts the previous reviewed PageList growth-only hash
`8b844ab0976ecf9551db24508d7e934d9aa9739d79849baeb0c8492de63ce393`; normal
verification and linking reject that incomplete state. Preparation also accepts
the previous wide-cutoff PageList hash
`a288b692c579a47411affc8d543e89d57690a587bc4e7eebe6d95c9010fd3aa7`.
The normal verifier requires the corrected state. Cargo adds `--built`, requiring
the recorded patch identity and compiled-library digest before linking.

The existing build recipe stamp includes the patch and verifier digests, and the
library digest. CI cache keys include patch and verifier files. The runtime
compatibility marker includes the full patch digest, so checkpoints from the
uncorrected codec do not silently cross this compatibility boundary. Optional
historical baseline measurements use a separate cache to avoid compiling the
current patched source as an earlier revision.

This is a fixed patch for four roundtrip defects, one allocator capacity defect,
two decoder admission defects, and one continuation-export defect, not an
extensible patch mechanism. The source diagnoses and verification limits are recorded in
`docs/archive/reviews/terminal-resize-roundtrip.md` and
`docs/archive/reviews/terminal-row-wrap-roundtrip.md` and
`docs/archive/reviews/terminal-wide-cutoff.md` and
`docs/archive/reviews/terminal-viewport-pin.md` and
`docs/archive/reviews/terminal-page-capacity-admission.md` and
`docs/archive/reviews/terminal-continuation-c1-introducer.md` and
`docs/archive/reviews/terminal-page-admission.md`. The earlier cursor-only patch
identity `0a945af64ff9636971fe89b88d1aca95eb5867ae4e61397b1e8b1e92f5e0c67b`
and its gate/performance evidence are superseded for the combined source;
its cursor regression evidence remains a valid pre/post record of that defect.

The intermediate growth-corrected patch identity
`d3cee6c6548ab641d4f528a5ce6280424a1caece5f67a4d48074f0c48f1b0af4`
is likewise superseded by the wide-cutoff correction; its red/green proof remains
historical scoped evidence, not a gate on the newest combined source.

The wide-cutoff patch identity
`a3a8c854621c53054cf8089da225258d9bd82907f78f90ec2cf5104b2c1208fa`
is superseded by the inactive viewport-pin correction. Earlier targeted proof
remains scoped evidence; full acceptance must use the newest combined source.

The viewport-pin-only patch identity
`c50f296f19e488833dc1c57a16a3f2c631546bb2e1070343d33763b1b2e3c1f0`
is superseded by the allocator capacity correction. Corpus and final gate
acceptance must use the newest combined source.

The allocator-capacity patch identity
`84b1b7a84db7342a95a9cff941f2cb866f2370859c83daa25a8d106e06acc91d`
is superseded by the decoder capacity-admission correction. Its evidence
remains a valid record of the earlier defects; corpus and gate acceptance must
use the newest combined source. That identity also appears in the previous
runtime compatibility marker, so checkpoints written by it are rejected by
this build rather than decoded.

The decoder capacity-admission patch identity
`2f94f382946d59e97b6d03b625092964e8cb62662acff75e9aec09866d4b71e7`
is superseded by the continuation-export correction. Its seed 201 and gate
evidence remain valid for that defect; corpus and gate acceptance must use the
newest combined source. Both identities appeared in runtime compatibility
markers, so checkpoints written by either are rejected by this build rather
than decoded.

Both snapshot decoder defects are reported upstream as Ghostty discussions
14185 and 14186. Drafts and the reporting constraints are in `docs/upstream/`.
The corrections remain local until upstream resolves them.

Identities are no longer maintained by hand. The patch, every recorded source
hash, and the runtime compatibility marker are regenerated from the corrected
tree together, because `--prepare` re-applies the patch and a hand-edited
source without a regenerated identity is silently restored.
