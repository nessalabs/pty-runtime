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

| Input | SHA-256 |
| --- | --- |
| Patch | `84b1b7a84db7342a95a9cff941f2cb866f2370859c83daa25a8d106e06acc91d` |
| Original `src/terminal/snapshot/screen.zig` | `abc550e1b8cbee843f2ee5b2168602aff1ef66b11368f5c38394b7b27704fca7` |
| Corrected screen source | `4ae17bd3be6851083d4e60f8378ece70f6910a6e9de75a2fdfa1c9afb3beb819` |
| Original `src/terminal/PageList.zig` | `cd926e56749c014a8df7f30fff1f5c32548cb4a8731451e726d7171d25818fcb` |
| Corrected PageList source | `a7703d31bfc95c68446e466ba3cf5cc329bed405e3527fe5503d344547e50462` |
| Original `src/terminal/bitmap_allocator.zig` | `bac61a65b5a3141ccfad2d9d0a6a452be7106a647182470fcf38e1289b5f86e1` |
| Corrected bitmap allocator source | `32673b2a73f1cf5135fbb1aa4f07855bff6a0c3e42789a178e9ee93da046debc` |

`experiments/run.py` calls `scripts/native/verify_source.py --prepare` after
verifying/extracting the original archive, before selecting or rebuilding the
library. The verifier checks every existing archive native input; only these
three source files may be in their recorded original or corrected states. It
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

This is a fixed patch for four roundtrip defects and one allocator capacity defect, not an extensible patch
mechanism. The source diagnoses and verification limits are recorded in
`docs/reviews/terminal-resize-roundtrip.md` and
`docs/reviews/terminal-row-wrap-roundtrip.md` and
`docs/reviews/terminal-wide-cutoff.md` and
`docs/reviews/terminal-viewport-pin.md`. The earlier cursor-only patch
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
