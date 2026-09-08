# Exact-cap deadline boundary correction

Root and independent interactive review found a P2 control-flow defect: if the last successful write reaches the cap and the outer deadline check exits the loop, the old code never reaches the next-loop cap assignment. It could return `capped=false` despite `bytes==cap`, allowing a censored trial to succeed.

The retained `producer-before.rs.txt` is the exact pre-fix source. The fix computes `capped` from final accepted bytes immediately before returning, independently of loop exit path. Writes cannot exceed the cap because each prepared slice is limited to remaining capacity and the byte ledger advances only by the actual successful write return value.

There is no existing clock/write injection seam to force this race deterministically. No timing-dependent test or production abstraction was added merely to manufacture a red result. This finding is supported by static control-flow evidence, **not a reproduced red regression**. Existing real Unix-socket tests validate both exact cap exhaustion and deadline-limited backpressure with exact partial-write accounting. Focused tests and warnings-as-errors Clippy were rerun after the fix; independent re-review and the coordinating gate remain required.
