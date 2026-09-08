# Rust coverage readiness matrix, attempt 1

The frozen macOS source passed all three workspace test configurations and the
separately instrumented helper unit tests. Both strict coverage reports failed;
this is retained evidence of an unmet readiness target.

| Instrumented scope | Lines | Functions | Regions |
| --- | ---: | ---: | ---: |
| Workspace Rust, feature matrix | 7483 / 8162 (91.68%) | 720 / 851 (84.61%) | 10757 / 12043 (89.32%) |
| Guardian helper, unit tests only | 168 / 1449 (11.59%) | 17 / 106 (16.04%) | 301 / 2212 (13.61%) |

`metadata.json` binds the commands, fresh profile/build directories, source
inventory and platform to this run. Sources remained unchanged. Phase logs retain
all test and reporting outcomes; `summary.json` contains the per-file inventory.
The compressed LLVM exports retain the underlying coverage records.
The export includes test code retained by the coverage tool; this aggregate is
not a separately filtered production-only denominator.

The helper numbers describe only its unit tests. They do not measure the separate
real-process functional probes. Continuous profiling of the forking helper needs
its own counter-consistency validation before those results can be accepted.

No Rust branch coverage, other-platform configurations, C bridge, upstream native
dependency, Python or build tooling is measured here. Those scopes cannot be
called covered from this matrix. The strict command requires 100% and zero
uncovered lines/functions/regions; passing tests alone does not satisfy it.
