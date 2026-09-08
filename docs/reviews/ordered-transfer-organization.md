# Independent ordered-transfer DDD and organization review

Reviewed by root on 2026-09-08, independently of scheduler_adapter's transfer
implementation. Scope: domain transfer order/boundaries/errors; application
journal, transfer, snapshot, stream End, coordinator admission and their public
facade wiring. This review complements the separate adversarial transfer and
READY restoration reviews; it does not self-review root's event-stream adapter.

Domain owns lifetime/sequence validation, lost continuation boundaries and
terminal ordering policy. Original byte position and ordered mutation sequence
are distinct values. Application owns admitted observers, journal payload leases,
checkpoint/continuation capture and wake registration. It obtains immutable
native checkpoint bytes through the terminal port; native restore and event-store
records do not enter the domain or application policy.

The journal is a specific bounded continuation mechanism, not a generic event
repository. Its payload reservations follow retained Arc-backed events after
eviction. Per-session/shared byte and slot bounds therefore apply to slow
consumers retaining results. No observer means no payload journal allocation;
sequence still advances so later observers cannot invent omitted history.
Snapshot admission reserves both checkpoint and continuation ownership at the
same serialized boundary. Splitting the returned parts preserves their independent
lifetimes without detaching a native buffer from its budget.

The End owner seals mutation admission and waits for the admitted mutation prefix,
actual output drain and outstanding restore-source validation. It does not
substitute process exit for drain or let later observations append mutations
after End. Failed-prefix completion remains explicit. READY work alternates one
admitted live operation with one history unit; checkpoint capture waits for a
validated source. Inapplicable history pages have explicit portable outcomes and
cumulative accounting, rather than being labeled complete history.

Waker clone, replacement and destruction are outside session, ticket and journal
locks; wake panics are contained per observer. The independent adversarial review
found and fixed the earlier deadlock and terminal-End ordering defects, with
regressions. Returned views/checkpoints expose borrowed buffers so callers cannot
move owned memory out while dropping its quota holder. Explicit caller copies
have separate ownership.

Modules are organized around policy, admission/leases, journal observation,
snapshot capture and terminal completion. No service locator, duplicate mutable
repository state, per-byte port dispatch or unnecessary interface hierarchy was
introduced. All sources satisfy the 350-nonblank-line inventory. No unresolved
P1/P2 organization or dependency-direction issue was found in this scope.

Evidence is the actual transfer/READY native and integration regressions linked
from their correctness reports. Consolidated gate, source manifest, target runs
and full-load/soak qualification remain separate from this structural assessment.
