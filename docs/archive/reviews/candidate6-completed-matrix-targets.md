# Candidate6 completed matrix groups: target assessment

Read-only analysis on 2026-09-09 of the preserved interrupted matrix. Used existing `scripts/release/load_support/reporting.py::trial_targets` with each raw identity record's actual configuration and `rollup`, not trial correctness as a substitute for target evidence. No tests, builds, source changes or load runs were performed.

## Result and scope

Eight complete groups contain **40/40 completed correctness passes**. Their explicit target records independently report **30/30 idle owner-CPU passes** and **10/10 active latency trial passes**, with **50/50 individual latency-boundary passes**, no missing applicable target records and no recorded target misses. All 30 idle target records have acceptance_duration=true. These statements apply only to the eight groups below.

The whole directory contains 47 trials: 45 correctness passes, one retained harness failure (resources-projected-1 repeat1, periodic census after closed checkpoint), and one incomplete trial (rate-1MiB repeat2). The additional five passes belong to unfinished groups: four projected-1 repeats and one rate-1MiB repeat. They do not complete those five-repeat cases. There is no final driver summary and the termination cause is not established. This report does not relabel either failure/incompletion or claim the full matrix passed.

## Source and workload

All 40 selected records identify source 46dbec64384e4519e751086ef47feb5bac11024d, empty source diff hash, binary SHA-256 `65eb7f670f17edabe24b7157e12c2cfad614ef0383facf2d0caac42f2273d303`, macOS26.6 arm64, Apple M5 Mac17,3, 10 logical CPUs, 25,769,803,776 bytes RAM, rustc/cargo1.98.1. Zig query is unavailable. Raw identity records explicitly require a retained build log to link binary and source; hashes alone do not establish that build link, and this report does not independently audit it.

Every selected trial is non-smoke, with 10-second warmup, 60-second requested measurement, 4,093-byte chunks and 80x24 grid. Active cases are projected, one observer/session, combined offered rate10MiB/s, default256 staging slots/session. The 128-active case has128 active producers; 128-mixed has32 active producers among128 sessions. Idle/resource cases have zero active producers, offered rate0 and observers0; raw/projected mode follows the case name. The fixture's 3,600-second park interval keeps projected models resident through these short idle runs; these are not automatic-parking measurements.

## Idle CPU targets

Percentages are percent of **one core**, from the owner process only. Each group has five complete target records, all <=1%. Latency targets are inapplicable to these zero-active cases.

| Group | Trials | Owner CPU range, % of one core | Recorded target passes | ADR interpretation |
| --- | ---: | ---: | ---: | --- |
| idle-64 | 5 | 0.164861–0.181633 | 5/5 | 64-idle reference population |
| resources-raw-1 | 5 | 0.066137–0.066213 | 5/5 | resource sweep; different population from64-idle reference |
| resources-raw-32 | 5 | 0.099086–0.115713 | 5/5 | resource sweep; different population from64-idle reference |
| resources-raw-128 | 5 | 0.247013–0.296456 | 5/5 | resource sweep; different population from64-idle reference |
| resources-projected-32 | 5 | 0.082553–0.115693 | 5/5 | resource sweep; different population from64-idle reference |
| resources-projected-128 | 5 | 0.246845–0.263404 | 5/5 | resource sweep; different population from64-idle reference |

ADR0002 specifies <=1% over60 seconds for64 idle sessions. Only idle-64 directly matches that population; the five resource groups are useful off-reference comparisons even though the runner applies the same numeric ceiling. Idle-64 CPU intervals span60.561–60.657 seconds. Its fixture processes separately consume8.372–8.570% of a core; that cost is not included in the owner CPU target.

## Active latency and CPU

The table shows the **worst per-trial p99** across five repetitions, in milliseconds, not a pooled percentile. Every individual latency record reports zero failures, zero unavailable samples and measurement_complete=true.

| Group | Input dispatch <=20ms | Raw output <=20ms | Projected output <=20ms | Resize <=100ms | Cancel <=100ms | Trial passes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 128-active | 3.000 | 0.400 | 3.900 | 2.300 | 6.044 | 5/5 |
| 128-mixed | 1.200 | 0.200 | 1.300 | 1.500 | 2.229 | 5/5 |

| Group | Owner CPU % of one core | Fixture CPU % of one core | Accepted throughput MiB/s |
| --- | ---: | ---: | ---: |
| 128-active | 73.604–77.761 | 30.507–32.166 | 9.997700–9.999569 |
| 128-mixed | 97.610–110.984 | 37.881–39.823 | 9.998950–9.999636 |

No active-CPU ceiling is defined by these target records; the mixed case's owner usage above100% means more than one core, not an idle-CPU failure. These128-session workloads are explicitly required additional scenarios in ADR0002/0004, but neither substitutes for the64-resident/16-active combined10MiB/s baseline, its detached/dominant variants, rate/fairness sweeps or500-session qualification. Accepted throughput is a measured producer-window rate, not an additional Boolean latency result or proof of all fairness requirements.

## Attribution limits and remaining gaps

CPU intervals expressly set complete_process_tree_accounting=false: only PIDs present in both snapshots contribute. All selected CPU records report empty unavailable/unmatched PID lists, but processes created and exited between snapshots remain unmeasured. Reported guardian/helper CPU is0.0 in these intervals; this is the census observation, not proof that helpers consume no CPU. Census overhead is included. Owner measurements include executable fixture/runtime components and do not isolate every internal worker. These records do not prove wakeup-count requirements, <=4KiB control-state attribution, complete kernel/helper memory accounting,12-hour soak, latest-source qualification or absence of unrelated failures.

## Reproducible input identity

Selected raw files remain untouched. The existing reporting implementation used here has SHA-256 `de5965d3d36551be64614ce4efce154d1815340db1bba907168d12d6ab578b59`. Raw input hashes:

| Raw trial | SHA-256 |
| --- | --- |
| `idle-64-1.jsonl` | `844d63faa66b783093ebfd4de20decf1aafe990bdc3cc1df40a8560a2dd2205b` |
| `idle-64-2.jsonl` | `2fc0c455e9de0309ae20e66665ec26ea4de065807cb239aa604a3b5284172b33` |
| `idle-64-3.jsonl` | `c35cfa8918b600d45e11d0df9424683140d2c46724526b4cde0490d6b9d1d4b5` |
| `idle-64-4.jsonl` | `c9209809aa4cead9cb4b32db0b52a446672ee21e832a6d22640f5d9297a75b3f` |
| `idle-64-5.jsonl` | `9a8e5226eb95f73bf9fb4f566c743d47eaf422c50076dd45a759942183f242f6` |
| `resources-raw-1-1.jsonl` | `32870c5a642d8f9460e7c25d74f7c7165e7753f391b0cc8c7d05e15592dcdc14` |
| `resources-raw-1-2.jsonl` | `72bf466277e03abbeb20f828bf50c69e75e66aaa575f0b53107112428f91699b` |
| `resources-raw-1-3.jsonl` | `cd8008e0de03075cfc181a7f32cb9194e12cc33f9ef75fcff0c99e9b949b0636` |
| `resources-raw-1-4.jsonl` | `5ecea96b3dc9ab69c4b7704f6c724e94fb5df1194cce2cf6db597a938cceb408` |
| `resources-raw-1-5.jsonl` | `e13552006c880dd4201b742234eae7435d8c98951219d7746883385dce56b013` |
| `resources-raw-32-1.jsonl` | `0f7e3c92fc973f1754064b3ddd8bf29f09953f05f836f829f420e7fc0ed598b2` |
| `resources-raw-32-2.jsonl` | `105022d06076cd6d777386b1ff2c896d1bbb80d4e6f2813c8acc11fe9793971e` |
| `resources-raw-32-3.jsonl` | `0777a92ed22d8debbdf0ab2d1fdb95823060adeab177ba7d30dbe37128a3c21b` |
| `resources-raw-32-4.jsonl` | `a9a7f5a5cdb52988860dd1ff10d8df62b4ed1358ffd20303cd8707f50beaf291` |
| `resources-raw-32-5.jsonl` | `628f01d693894439956d2e2d08fc5047fe960c2d56087db2f2379ec67187dafd` |
| `resources-raw-128-1.jsonl` | `3c11855a594c3ff96189f8fa1962d618f8170571fca6d3b1714afbd6e5a0c208` |
| `resources-raw-128-2.jsonl` | `0c993afd2e44d7efec5c2e155a12ec0a5bb694556d2fc12d1a0fa1204b1cb5b0` |
| `resources-raw-128-3.jsonl` | `1aaec2ace022aae8ea217b005b6be461c82879902666fc6a73c84cdde31f4619` |
| `resources-raw-128-4.jsonl` | `6a664ec296fa5e1a19eb79f23d1e91e1de41593d34a1b4e6afaaab7fa2ab9e7a` |
| `resources-raw-128-5.jsonl` | `f52121945181a8fcbc4bbc5cad2afaedf800f448ee1597c29d958ffcc06d046d` |
| `resources-projected-32-1.jsonl` | `37771375c11fd53ac593fefa76be48fc5f820fa80736c3fccc0cf4aea64021a2` |
| `resources-projected-32-2.jsonl` | `78b3cdf54cb376271b54899d61808ba967e12a281623e2157f73b2238ceb304d` |
| `resources-projected-32-3.jsonl` | `7be293610b83457db775d709a212f8851cafd4f736c1598177844d23bfa6e199` |
| `resources-projected-32-4.jsonl` | `540bd8aab6a5d1457ad2b5edd212658da4de77655dccc75e14af5291c55e0eba` |
| `resources-projected-32-5.jsonl` | `739cb24c02a0d9265fe3754ea5112ffe97f158aa2c57f21e6c5223494c43f4e2` |
| `resources-projected-128-1.jsonl` | `de950addb547180fbcbd91cf7f4054a9330918c3c33b431ac7c4f85c3c0f689c` |
| `resources-projected-128-2.jsonl` | `39c6f31fe72677eae09e2cf8a98655863aaec01b4623bd37d9b15dc32cc58ed5` |
| `resources-projected-128-3.jsonl` | `db4d89e14d44cc87d9df86d7eac6188f61952bd430b686bfc646b6d3f2b01713` |
| `resources-projected-128-4.jsonl` | `a3a7368a25e13bc6299899c9a77d2b3ac70e55f3100400256db61d285244feca` |
| `resources-projected-128-5.jsonl` | `0a9cd1d39d57100d567aff2ca05f52d4e45fbed13c167c93183525f9afa65e22` |
| `128-active-1.jsonl` | `23815febadfcf92be7e8be07f94eba9eec458eeafc2cf022cf8ad8dc1fd09b4d` |
| `128-active-2.jsonl` | `16cbf8d1e1d6d998275b2b3f3e4e2ba82f6844e7e556b88a28cbf15eb7794bbd` |
| `128-active-3.jsonl` | `41adc0fa1cdd837d7cb1900e9e71771746b3fd39a5825ed4039fbe7d5c1536a4` |
| `128-active-4.jsonl` | `7402e4bcd70db87f03a33683d512b872eec1fca48064433981079c817dfe0b05` |
| `128-active-5.jsonl` | `a83ef4202648f1dc6cd6d299f46cd6f11e8d98a13e9edaaece62cec1e00fb92f` |
| `128-mixed-1.jsonl` | `4e9b34a831b743b9031d57c811dca7d4610f3b06a81b3ac03fac00913ffff565` |
| `128-mixed-2.jsonl` | `7a56b6ab968f9cb5ab789f0f3a72d32196deb5371b2622886929be29a6d426dc` |
| `128-mixed-3.jsonl` | `c652573708053b6f7337a487771640f26e758434fc3da838f2204ac251081133` |
| `128-mixed-4.jsonl` | `07e9b473e1b56b1c4aebc9d2808fc3bbdffbd2a4078b391953a3a6d4ee6fbd73` |
| `128-mixed-5.jsonl` | `a07f3b0e31f36e0db4b1dec9a160da6a76c106b1391d9b9767d54c6d9d645397` |
