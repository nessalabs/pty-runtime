# Candidate5 Rust coverage readiness

Measured unchanged 293-source inventory matching the frozen 71c1d6f release build. All three workspace feature-matrix test phases and the separate helper unit-test phase passed. Both strict 100% coverage reports failed.

| Scope | Lines | Functions | Regions |
| --- | --- | --- | --- |
| Workspace Rust feature matrix | 7,801 / 8,401 (92.86%) | 744 / 866 (85.91%) | 11,278 / 12,460 (90.51%) |
| Guardian helper unit tests | 168 / 1,449 (11.59%) | 17 / 106 (16.04%) | 301 / 2,212 (13.61%) |

Workspace gaps are 600 lines, 122 functions and 1,182 regions. The prior scoped matrix reported 91.68%,84.61%,89.32%; source and denominators changed, so the difference is a descriptive coverage comparison rather than a controlled attribution to individual new tests. This export includes test code retained by the tool, not a separately filtered production-only denominator.

Metadata records commands, fresh build/profile directory and exact source hashes. Compressed LLVM JSON exports preserve underlying records; summary.json includes per-file counts. Helper coverage describes unit tests only, not real-process functional coverage; the separate continuous/forking helper counter-consistency limitation remains unresolved. Unattributed fallback .profraw files emitted by instrumented subprocesses are preserved separately in the local run and are not silently merged into these reports.

Native C/upstream dependencies, other platform configurations, branch/MC-DC and Python/build tooling remain unmeasured by this command. Zero branch counts mean unavailable instrumentation, not full branch coverage. Coverage readiness and release readiness remain incomplete.
