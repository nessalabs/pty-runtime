# Final census versus normal owner exit

The load driver could report a sampling failure after a successful final `closed`
census. The retained candidate-6 record shows that census had one owner, zero
zombies, and no unavailable PIDs. After the driver acknowledged it, the owner was
free to emit `complete` and exit. A periodic census immediately afterward raced
that exit: the live `poll()` observation did not guarantee the owner remained
available to `ps`/`lsof`. The failure record retained queued `complete`, exit code
0, and `killed_by_driver: false`.

Evidence: `docs/verification/release/candidate6-matrix-interrupted/resources-projected-1-1.jsonl`,
SHA-256 `208df4c7eeebcadcca005d24f9ae0c38dcef6d2b75c0a523459b80d21b385478`.
This failed trial remains failed and unchanged; the fix does not retroactively
convert it to acceptance evidence. The frozen candidate-6 checkout is untouched.

`scripts/release/load.py` now excludes periodic samples after the closed census
has been recorded. The existing final census, descendant/zombie assertions,
checkpoint acknowledgement, output draining, required `complete`, and exit-code
validation remain in place. A census exception before that boundary still takes
the existing failure path. No collector exceptions are swallowed.

## Deterministic evidence

`scripts/tests/test_load_final_census.py` runs real Python child processes speaking
the checkpoint/acknowledgement protocol. The test controls only orchestration:
a virtual sampling clock makes sampling due at the final checkpoint, and a poll
hook returns a live observation with the real child already reaped before the
next collector call. That call raises `ProcessLookupError`, representing the
retained `ps`/`lsof` race. This is a driver regression, not a new OS census test.
Negative controls require failure for pre-closed disappearance, missing complete,
nonzero exit, descendants, and zombies. Owned pipes must be closed in each case.

Raw results in `docs/verification/release/final-census-race/`:

- `initial-fixture-errors.log`: retained original test run. The intended regression
  failed, but two negative controls also hit fixture-only timeouts because the
  poll hook waited before an invalid closed census could be acknowledged. The
  hook now advances the sampling clock only for valid census snapshots and forces
  its live observation once. These errors were not production-defect evidence.
- `red.log`: corrected fixture against unchanged driver, 4 tests, exactly 1
  intended failure (after-closed periodic `ProcessLookupError`), exit 1.
- `green.log`: fixed driver, all 22 `test_load_*.py` tests pass, exit 0.

Commands:

```
python3 -m unittest discover -s scripts/tests -p test_load_final_census.py -v
python3 -m unittest discover -s scripts/tests -p 'test_load_*.py' -v
git diff --check -- scripts/release/load.py scripts/tests/test_load_final_census.py
```

Native macOS, Python 3.9; base HEAD `46dbec64384e4519e751086ef47feb5bac11024d`,
with working-tree changes. File identities:

| File | SHA-256 |
| --- | --- |
| Driver before fix | `6e6c0e48dbbdd849ccd065648a1dbda7953f79f40cdfad586187320918085125` |
| Driver after fix | `9176921b0e505773b53ad767a01ee559ef2ad099d5f9116905d1bc612c9be5ec` |
| Corrected regression, RED and GREEN | `039306032f261869136f01aeb23accc788af8f0f6e80d12033f2bd41096954eb` |

No Rust build, full matrix, scoped throughput measurement, Linux execution, or
coverage run was performed in this bounded Python task. Repository gate and
independent reviews are coordinated by the parent task; this author record alone
does not claim a release milestone or independent review completion.
