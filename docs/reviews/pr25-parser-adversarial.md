# PR25 parser adversarial review

Independent review of the parser repair and independently authored acceptance tests. The reviewer did not author either file. Outcome: no unresolved P1/P2 finding in this change. This is a scoped behavioral review, not the complete repository acceptance gate.

The live [OSC 7501 revision 0.4 specification](https://www.superlogical.com/rex/docs/build/program-status) was read on 2026-10-12. It explicitly skips malformed pairs, including values outside `[A-Za-z0-9_.,+/=-]*`, processes the remaining report, gives later repeated values precedence, and requires limits to be checked before stored records change. Those rules distinguish a malformed pair from a syntactically valid value that fails the app-specific grammar.

## Challenged behavior

- `report.rs` checks the 32-byte app limit before UTF-8 and general value grammar. Invalid punctuation, interior spaces, and non-UTF-8 bytes within the limit cannot overwrite the earlier app. A malformed 33-byte app still rejects the entire report, even with a later valid app. Trimming behavior and limit placement are unchanged.
- General-grammar-valid comma, slash, equals, or empty values reach the final app-specific filter; an invalid final app becomes absent rather than reviving an earlier duplicate. Valid later app values replace earlier ones.
- The removed early app branches cannot consume or suppress later state/message pairs: parsing continues at the next colon. The changed tests assert exact final state `Done`, decoded message `ok`, and both raw and resolved app, rather than merely accepting the report.
- Fragmented input retains scanner state across committed chunks. The new shared assertion exercises every two-chunk split, including within the OSC prefix, app bytes, and ESC/backslash terminator, with both BEL and ST. Existing bytewise, sequence-limit, query-offset, and stale-plan tests supplement this. The new 33-byte malformed-value rejection uses BEL; shared scanner handling and the existing ST byte-limit coverage were inspected as supporting evidence, not claimed as a new ST rejection test.
- Rejected oversize reports produce no mutation; the tests compare complete snapshots, including revision and protected earlier content. Existing eviction-order tests also check that rejected duplicate limits do not refresh stored records. The change creates no new allocation, ownership boundary, shared state, retry, or OS resource lifetime.

## Evidence and limitations

The retained pre-fix run has 17 passing tests and two expected failures, both demonstrating malformed duplicate app values erasing the earlier app. The same 19-test acceptance target passes after the repair. I inspected the source, scanner/store call path, assertions, and raw RED/GREEN logs; I did not launch another Rust build while the parent owned the mechanical-gate lane.

| File | SHA-256 |
| --- | --- |
| `crates/domain/src/terminal/program_status/report.rs` | `79dbcf2811d6e0386a7cc5bc2dec1a2bd69159d7f50ab38147613e7a82fd99fe` |
| `crates/domain/tests/program_status_acceptance.rs` | `99ace72430ee272720b11fa1606045f79712edf669b675509b6a78f4401bc724` |
| `work/pr25-blockers/domain-acceptance-red.log` | `542439c8df4493f6a414c11a8cadb5a80e7a8dc791dd30cd8332c0da7dfcc698` |
| `work/pr25-blockers/domain-acceptance-green.log` | `a48862df054d90e7fd856bb5e87fc3bf90522c9ff9a22f84d6238937a32dd701` |

This review does not establish full protocol coverage, native integration, cross-platform behavior, performance qualification, or 100% coverage. The benchmark cleanup implementation is deliberately outside this independent review because this reviewer authored it; separate reviewers cover that change.
