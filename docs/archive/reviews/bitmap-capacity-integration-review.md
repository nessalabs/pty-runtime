# Independent bitmap-capacity integration review

No P1/P2 finding in the reviewed guard and integration. No build input was changed
by this reviewer. Reviewed combined patch:
`84b1b7a84db7342a95a9cff941f2cb866f2370859c83daa25a8d106e06acc91d`.

The added `i >= bitmaps.len` check runs before the final word read in the large
span search. After the full-word loop, the remaining request is nonzero and at
most 64 bits; the previous check inside that loop did not cover the final word.
If that index is exhausted, starting later cannot produce a sufficiently long
span. Returning null is therefore the correct capacity rejection. No bitmap has
been modified before the new return. Public allocation converts null to
OutOfMemory before advancing search_start or constructing a returned slice.

The two author tests cover final-word remainders 1–64, both one-word and multiple
word spans, unchanged bitmap state on rejection, exact-capacity success, an
occupied prefix with nonzero search_start, data preservation, free and retry.
The retained author logs show before-fix bounds failures and 33 tests passing in
both ReleaseSafe and ReleaseFast. These logs were inspected, not regenerated.
This scoped review does not assert corpus completion or general allocator coverage.

The author bitmap patch digest `76c93c81d2e6b2f2aea1e730d9a3f2bb35ab86fa7746c199f08004b989070832`
is verified and its bytes exactly match the combined patch suffix. The corrected
bitmap source digest `32673b2a73f1cf5135fbb1aa4f07855bff6a0c3e42789a178e9ee93da046debc`
matches both author evidence and the verifier's third target. The two earlier
screen/PageList target digests remain unchanged.

Independently re-applied all three patch targets from original archive contents
in a temporary directory and checked all corrected hashes. The pinned archive
hash and normal source verifier pass. The three-target verifier stages every
original and verifies every result before publishing; the old two-target cache
can be prepared because the original bitmap source is an allowed preparation
state. Normal/link verification requires all three corrected states.

The runtime compatibility marker contains the full new combined patch digest.
The build recipe keys on patch/verifier/driver digests and library digest; CI
cache keys include the patch and verifier. Cargo invokes the built verifier and
tracks patch, verifier, build stamp and library changes. The completed cache
stamp now has the new patch identity and matching library digest, and an
independent `verify_source.py --built` run exits 0. Evidence and reviewed file
hashes are under `docs/verification/bitmap-capacity-integration-review`.

No corpus or runtime/native test suite was executed by this review. Root owns
fresh integrated tests, full gate, cross-platform execution and final acceptance.
