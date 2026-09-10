# Independent bitmap allocator capacity review

The large-span search reads beyond its final bitmap when a request needs another
1–64 chunks after the last available word. Confirmed with tests added first in a
scratch copy of the pinned source. No canonical dependency, verifier, patch,
compatibility identifier or native library was changed.

`findFreeChunks([0], 65)` should return null; ReleaseSafe instead panics at the
final `@ctz(bitmaps[i])`, index 1 with length 1. The public allocator also panics
when a 192-byte pool has one 64-byte allocation and receives a 129-byte request:
search_start correctly skips the first word, but the remaining two-word slice
is read at index 2 instead of returning OutOfMemory. Both red traces are retained.

The scratch correction is a bounds check immediately before the final-word
lookup. The earlier loop checks bounds only while more than 64 chunks remain;
it does not protect this lookup after its last increment. Returning null here
is correct because no remaining word exists, no later candidate could fit, and
bitmap marking has not started. The correction neither changes capacity nor
allocates additional storage.

Two focused tests cover all excess requests from one through 64 chunks, unchanged
bitmaps and already allocated contents after rejection, nonzero search_start,
a partially occupied first word, exact-capacity success, exhaustion, freeing,
and complete-capacity reuse. All 33 imported allocator/offset tests pass in both
ReleaseSafe and ReleaseFast with the same correction. This is genuine test-first
red/green allocator capacity evidence; it establishes no corpus/snapshot claim.

The integration candidate is
`docs/verification/bitmap-capacity-independent/bitmap-capacity.patch`, with exact
before/after/patch hashes in adjacent `source.json` and all red/green logs. Root
owns canonical patch integration, rebuild, specialist review and final gate.
