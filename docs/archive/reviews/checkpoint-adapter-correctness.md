# Checkpoint adapter adversarial correctness review

Reviewed 2026-09-08: domain checkpoint values, application store/protector ports,
`FileCheckpointStore`, its anchored filesystem helper, `CheckpointProtector`,
existing contract/fault tests, and pinned RustCrypto/zeroize implementation.
This is adapter-only evidence. **Default parking/G3 and integrated native G2
remain pending.** No provider/runtime race or release qualification is inferred.

## Findings

| Priority / status | Evidence | Finding |
| --- | --- | --- |
| P2, reproduced, pending fix | `checkpoint/file.rs::Drop` final `remove_dir(&self.path)` | Object files are correctly removed through the original anchored descriptor, but directory removal follows a saved path. Rename the owned directory and create an unrelated empty replacement at that path: Drop removes the replacement it never owned. Independent `checkpoint_adversarial` test reproduces this, cleaning up its fixtures before assertion. Verify namespace identity for final directory cleanup and document the trusted-parent/concurrent-rename boundary; do not use recursive deletion of an unchecked replacement. |
| P2, memory-erasure hardening pending | `checkpoint/protector.rs::protect`, before creation of `Zeroizing`; `terminal/checkpoint.rs::Drop` | Limit, metadata or entropy rejection occurs before the plaintext is wrapped in Zeroizing. Those paths rely on ordinary `Vec::fill(0)` in domain Drop, which does not guarantee compiler-resistant erasure. Take the plaintext into the zeroizing owner before any fallible return when claiming guaranteed erasure. |
| P2, spare-capacity hardening pending | `checkpoint/protector.rs::protect` encrypt-then-reserve path | Encrypting initialized bytes before reallocation is good, but a caller/engine Vec can contain previously initialized plaintext beyond its truncated length. That spare storage is not encrypted before growth and can be left in the freed old allocation, or transferred into a nonzeroizing ProtectedCheckpoint. Clear spare capacity before possible growth, or explicitly limit the erasure claim to initialized plaintext. The native adapter currently zero-initializes checkpoint capacity; the injectable boundary accepts other producers. |

No cryptographic authentication bypass was found in reviewed scope. The memory
findings concern retained/freed process memory, not observed plaintext publication
to the store. The namespace defect deletes an unrelated empty directory; the
existing nonrecursive deletion does not delete its nonempty contents.

## Independent executed checks

Added `crates/infrastructure/tests/checkpoint_adversarial.rs` and executed:

`cargo test --locked -p pty-runtime-infrastructure --test checkpoint_adversarial`

On macOS arm64: **two pass, one fails** before fixes. Sixty-four protections of
the same key/generation/plaintext produce distinct 192-bit nonces and authenticate
back to the expected bytes. Changed file permissions and added hardlinks cause
reads to fail; restoring private permissions/single-link ownership allows read.
The replaced-directory Drop regression fails as described above. Nonce sampling
is a regression sanity check, not a statistical proof of CSPRNG security.

## Cryptography and buffer audit

The pinned `chacha20poly1305 0.10.1` cipher Drop explicitly zeroizes its stored
key. Key construction uses a `Zeroizing<[u8; 32]>`; nonces come from OS CSPRNG.
The protocol uses XChaCha20-Poly1305 with detached encryption, appends the tag and
nonce, and authenticates a domain-separated, unambiguously length-delimited
metadata representation. Owner/session sequence, parking generation, processed
byte position, ordered control generation and engine compatibility all enter
AAD. Decryption compares the caller's trusted expected key/descriptor before
using ciphertext metadata. Wrong owner key and mutated ciphertext/tag/nonce/AAD
are covered by existing tests.

Ciphertext length checks precede decryption; subtraction is guarded by the
40-byte minimum. The configured 512 MiB maximum plus 40-byte overhead cannot
overflow usize on the intended 64-bit targets. Decryption uses a zeroizing
mutable owner on authentication failure and transfers successful plaintext into
the terminal checkpoint owner. `zeroize 1.9.0` explicitly zeroes a Vec's full
capacity when invoked; its own documentation cannot promise erasure of storage
lost in prior reallocations. This is why reserve ordering and spare capacity
matter. Random nonces avoid a counter shared across concurrent calls; owner-key
lifetime and quota/pin ownership still need runtime integration.

## Storage semantics and limits

New namespace creation uses 0700 and rejects existing roots/symlinks. Child
operations use openat/unlinkat/renameat on an owned directory descriptor.
Checkpoint opens use O_NOFOLLOW, exclusive create for staging, private 0600
mode and single-link regular-file checks; read uses nonblocking open to avoid a
replacement FIFO blocking the caller. Reads compare committed length before
allocation, read exactly that length, then reject extra bytes. Altered ciphertext
of the correct length is intentionally authenticated by the protector, not the
opaque store.

Commit serializes quota reservation and I/O, writes a private temporary file,
fsyncs it, then atomically renames before publishing the in-memory entry. No
ordinary fallible operation follows rename that would falsely report an error
for a completed publication. This is owner-lifetime readability, not crash-durable
recovery: the directory is not synced and metadata/keys are not persisted. That
matches the no-restart-recovery boundary, but the ADR's bounded abandoned-data
cleanup remains separate work.

The byte quota uses remaining-capacity subtraction before increments. Failed
partial-file cleanup stays conservatively charged as an orphan so repeated
failures cannot bypass the cap. Minimum ciphertext size also bounds entry/orphan
counts by byte quota, although per-runtime operation/pin/metadata admission still
belongs in the application. A slow store mutex serializes later calls, including
capacity reporting; runtime must run these synchronous calls on bounded workers,
not hold a model/global runtime lock or accumulate unbounded blocked callers.

Existing failed-write tests inject ENOSPC/EIO/WriteZero into the private writer
seam; they are useful deterministic fault coverage, not proof of real disk-full
behavior. Concurrent commit tests establish serialized quota enforcement. The
injected memory provider contract covers basic same-result behavior, not delayed
commit cancellation, stale parking generations, immutable pins, restoration
races or default-store construction by the runtime. Those remain G3 gates.
