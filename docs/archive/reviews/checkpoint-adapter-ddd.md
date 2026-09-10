# Checkpoint adapter domain and contract review

Reviewed 2026-09-08 on macOS arm64. Scope is the checkpoint domain values,
application protector/store ports, infrastructure encryption/file adapters, and
their focused tests. No production code was edited. This does not review or
approve a parking coordinator, default runtime wiring, native-to-storage
integration, Linux behavior, or G3 completion.

## Findings

### C-D01 — P2: file storage knows a specific encryption envelope size

`crates/infrastructure/src/checkpoint/file.rs::commit_with` rejects ciphertext
shorter than 40 bytes. Forty is the built-in protector's 16-byte tag plus 24-byte
nonce overhead; the store port does not define that layout. Storage therefore
rejects a potentially valid ciphertext emitted by a different injected protector
with a smaller envelope, despite the requirement that it stores opaque bytes.

Keep envelope-size validation in `CheckpointProtector::open`. If generic storage
requires nonempty objects, state that invariant in its port and enforce it
without a cipher-specific constant. Add a short opaque-byte roundtrip store
test that does not call the built-in protector, retaining existing truncated
envelope rejection in the protector tests. Status: open at inspection.

### C-D02 — P2: failed-cleanup capacity semantics contradict the public contract

`ICheckpointStore::commit` says a failure releases reservations, while the file
adapter correctly retains a full byte charge when deleting a failed temporary
write also fails. That charge is added to `State::committed`, but
`CheckpointCapacity::committed_bytes` is documented as committed bytes even
though the orphan has no committed readable reference. This can mislead callers
diagnosing storage pressure or deciding what can be released.

Preserve conservative orphan accounting. Amend the port's failure contract and
capacity representation to make abandoned/unreleased storage explicit, or
clearly define the retained total to include it. A failed write must not imply
capacity was recovered when cleanup failed. Add an injected unlink-failure test
proving the retained charge prevents quota bypass. Status: open at inspection.

## Boundary assessment

- Domain and application contain no crypto/filesystem implementation imports.
  Ciphertext, reference identity, ordering metadata, capacity, and typed errors
  are local types; infrastructure maps external failures.
- The protector authenticates a version/domain separator, owner/session lifetime,
  parking generation, processed byte position, ordered control generation, and
  length-delimited compatibility identity. `open` requires independently supplied
  expected metadata and rejects mismatch before exposing terminal state.
- Encryption keys are generated internally, never exposed through the ports,
  and owned by the protector. The built-in algorithm and OS randomness remain
  infrastructure choices. This is source/contract review, not cryptographic
  implementation certification.
- Encryption reuses the consumed plaintext buffer and grows it only after the
  bytes have become ciphertext, avoiding an encryption-time reallocation that
  abandons an uncleared plaintext buffer. Decryption consumes ciphertext and
  holds failed processing buffers in `Zeroizing` wrappers.
- The file adapter does not parse native snapshots. It uses private anchored
  directory operations, checks regular-file shape/permissions, writes a temporary
  file, then publishes through rename. Unique object identities prevent stale
  references from reading/deleting a later object with the same checkpoint key.
- Synchronous operations are acceptable behind this port if their caller owns
  bounded worker admission and stale-result cleanup. Once commit starts, dropping
  its wait cannot revoke publication; that rule is explicit.

## Capacity, blocking, and proof limits

The file store holds its mutex throughout file I/O. Consequently `capacity()`
waits behind reads/writes and cannot provide a live observation of an ongoing
write's `inflight_bytes`. This is a consistent serialized snapshot, not a
nonblocking progress counter. State this explicitly on the port/implementation,
and avoid promising finite wall-clock duration merely because byte quotas are
finite. A blocked filesystem operation can also block the capacity query. Never
call it while holding a terminal, session-control, or global registry lock.

The serialized commit implementation prevents two writers from independently
oversubscribing its logical disk quota. This quota excludes filesystem block
rounding, cache, ciphertext buffers owned by the caller, and descriptor/map
metadata, as the implementation documents. It does not establish the runtime's
separate global staging, immutable-pin, or restore-memory limits.

Protect/open validate payload length, but consumed vectors can have larger
allocated capacities than their lengths. Native checkpoint buffers can likewise
retain their configured maximum after truncation. Future runtime admission must
charge actual owned capacity and temporary overlap, not only ciphertext length.
This is an integration obligation rather than a claim that the file byte quota
already includes caller-owned memory. Likewise, provider entry/metadata counts
need a finite runtime admission bound before arbitrary injected providers are
used by the default parking path.

The store is owner-lifetime only: descriptor metadata lives in memory, and no
restart recovery is claimed. Default unique directory selection, runtime key
lifecycle, stale checkpoint cleanup, abandoned namespace cleanup after a crash,
and parking fallback when storage fails are not implemented by these adapters
alone. Do not infer them from the constructor or normal Drop cleanup tests.

## Executed evidence

`cargo test --locked -p pty-runtime-infrastructure --no-default-features --test
checkpoint_contract` passed **5 tests**. The suite covers built-in/injected
provider roundtrips, precise reference identity, quota, normal cleanup/private
permissions, bad roots, ciphertext corruption/truncation, altered authenticated
metadata, short files, and symlink replacement.

`cargo test --locked -p pty-runtime-infrastructure --no-default-features --lib
checkpoint::file::tests` passed **2 tests**. These cover partial failed writes,
full-disk/write-zero outcomes, and simultaneous commits constrained by a single
quota. They do not inject failed orphan deletion or test another encryption
envelope format.

These **7 passing adapter tests** do not prove generation revalidation after
storage commit, model release atomicity, repeated native parking/restoration,
full parser staging, observer snapshot-to-live transfer, disk wake latency,
Linux qualification, or full G3 resource and failure gates.

## Immediate fix re-review

C-D01 is resolved: the store now requires only nonempty opaque ciphertext, while
the built-in protector retains its own 40-byte envelope validation. Added the
independent `tests/checkpoint_opaque_envelope.rs` under infrastructure, which
roundtrips a 17-byte opaque fixture without a protector. Its payload is solely a
storage-format fixture, not a claimed cryptographically valid encryption scheme.

`cargo test --locked -p pty-runtime-infrastructure --no-default-features --test
checkpoint_opaque_envelope` passed **1 test**, and its rustfmt check passed. The
total focused evidence executed by this reviewer is now **8 passing tests**.

The capacity port now explicitly warns that it can block behind provider I/O
and must not be called under native/control ownership. That clarification is
verified in source. C-D02 remains open pending the failure-accounting contract
clarification and orphan-delete failure evidence.
