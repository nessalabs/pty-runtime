# PAGE admission must bound what a header can make a decoder allocate

Status: author targeted tests pass, corpus seeds 1-1000 and the macOS gate are
recorded under `docs/verification/page-admission/`. Root owns independent
review and the Linux gate. No release acceptance is claimed.

## Failure

The independent correctness review of the capacity correction found that
rejecting unaddressable capacities closed only half of PAGE admission. An
*addressable* capacity is still an unbounded allocation. Measured on the
corrected source before this change:

- a 2,155,823,104-byte page from a 20-byte header, and
- roughly 34 GB of resident row headers from a 2.2 MB checkpoint.

Neither is refused by anything. `PageList.Builder.finish` installs the byte
limits but never compares them against what was decoded, `screen.decode`
accepts up to 65,535 page records, and page memory comes from `PageAlloc`,
which is `mmap` directly, so neither the embedder's allocator nor the runtime's
`native_bytes` budget ever sees it.

## A first attempt that was wrong

The first correction bounded the running total in `Builder.allocatePage` by
`options.max_size` plus one standard page. Corpus seed 3 failed immediately:
`max_size` is zero for alternate screens by design, and an alternate screen can
legitimately span more than one page, so the bound refused valid snapshots.

`max_size` is a scrollback *policy* number. It was the wrong authority for an
admission question, and borrowing it produced a bound that could not tell a
hostile header from a legitimate one.

Reverting it required re-synchronizing the patch identity: `verify_source.py
--prepare` re-applies the patch, so editing a corrected source without
regenerating the recorded hashes silently restores the edit. The first revert
measurement was invalid for this reason and was repeated.

## What the codebase already does

Ghostty solves this class in four places, none of which reach a decoded page:

- `kitty/graphics_image.zig` checks a declared size against both a fixed
  ceiling and the bytes actually available:
  `if (data_size > max_size or data_size > available) return error.InvalidData`.
- The same file rejects declared dimensions against `max_dimension` before
  allocating, and runs image decode through a `LimitedAllocator` whose
  `limit_exceeded` separates a limit from real memory exhaustion.
- The snapshot decoder already carries an embedder-supplied ceiling for its
  other untrusted length, `max_continuation_bytes`, exposed through the C API.
- `page.zig` itself notes that a large declared payload "is either hostile or a
  page far beyond native capacities", and bounds the staging buffer at 8 MiB.

Pages have no equivalent: no decoder option, no ceiling, and an allocator no
limit can observe.

## Bounded correction

Two checks, both modelled on the above rather than on a memory budget.

`Header.pageCapacity` refuses a layout larger than `max_page_bytes`, a fixed
64 MiB ceiling. This is the shape Kitty graphics uses: a constant far above any
page a terminal produces and far below what a hostile header asks for.
Representability is still asked first; the two answer different questions, so
both remain even though the ceiling is stricter today.

`Decoder.init` refuses a declared row count the record cannot pay for. Every
row costs `grid.row_header_len` bytes on the wire even when entirely default,
which the decoder reads unconditionally before any row content, so
`rows * row_header_len > payload_len - Header.len` is a property no encoder can
produce. That constant is now named in `grid.zig` and used by both the row
decoder and this check, so the bound cites the format rather than repeating a
literal. The new error is `CapacityUnjustified`.

This is admission, not policy: neither check consults a configured budget, so
neither can refuse a snapshot that a differently-configured runtime produced.

## Verification

macOS arm64, pinned Zig 0.16.0, baseline CPU configuration.

- Native tests cover both halves. A record carrying only its header while
  declaring 4096 rows is refused, and the same 4096 rows are accepted once the
  record actually carries them. A 128 MiB string capacity is refused as
  addressable but unaffordable. Unfiltered `zig build test-lib-vt` exits 0
  (`native-tests/full-green.log`); `zig fmt --check` passes.
- Corpus seeds 1-1000 passes with an unchanged source inventory
  (`corpus-1-1000/`) and reports counts identical to the run immediately before
  these bounds: 102036 malformed mutations, 101771 rejected, 265 accepted. The
  bounds therefore refuse nothing the corpus considers legitimate.
- `python3 scripts/gate.py` is recorded in `macos-gate/`.

## What is still open

`columns` is not payload-justified: a default row costs three bytes whatever
its width, so an over-declared `columns * rows` cell array is still admitted up
to the 64 MiB ceiling rather than being refused as inconsistent. String and
grapheme capacities are likewise not payload-justified, because a real page can
legitimately hold capacity it is not currently using. Closing those needs the
missing decoder option rather than a fixed ceiling; that is the shape proposed
upstream.

The 64 MiB ceiling is a judgment, not a derived bound. It is far above any page
this runtime has produced, but a caller with a genuinely enormous terminal
could reach it, and nothing here proves otherwise.

`native-tests/releasefast-representable-disabled-seed201.log` records a
measurement worth keeping: with `Capacity.representable` forced true, seed 201
passes in ReleaseFast rather than faulting, because these admission checks now
refuse the record first. The defenses overlap, so the original ReleaseFast
red for representability can no longer be reproduced by disabling that
predicate alone. The ReleaseFast evidence for it remains
`docs/verification/resumed/native-seed201`, taken when neither defense existed.
