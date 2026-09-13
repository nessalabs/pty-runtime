# Repository instructions

Read `coding_standards.md` before editing. Follow the ADRs. Keep
`docs/verification.md` honest about what works and what is still open. Every
review loop must run `python3 scripts/gate.py` and obtain independent specialist
agent reviews for DDD, organization/design patterns, and adversarial correctness.
Record and resolve findings before claiming a milestone passed.
User authorization permits commits and pushes to main in this private repository.
Do not turn experiment fixtures into claims of implemented runtime behavior.

## Pull requests

Every PR uses [`.github/pull_request_template.md`](.github/pull_request_template.md)
and fills in every section. The rules behind it:

- **Write for someone with no context.** Plain English. A reader who has never
  seen this repository should understand what was broken and why it mattered
  before they meet a single identifier or acronym. Explain jargon at first use
  or drop it.
- **Every claim carries a number.** "Faster", "more reliable" and "should fix"
  are not claims, they are opinions. Give the before value, the after value,
  the target being measured against, how many trials, and where the raw data
  lives. State the trial count honestly - one trial is an observation, not
  evidence, and must be labelled as such.
- **Say what the change does not fix.** The limitations section is required,
  not optional. Name the cases still failing, the platforms with no coverage,
  and anything assumed rather than measured.
- **Correct the record in the open.** If the work overturned an earlier claim -
  in a doc, a previous PR, or your own earlier message - say so in the
  Corrections section rather than quietly editing it away.
- **Tell the reviewer where to look.** Two or three files that matter and the
  question to ask of each.

The description is the deliverable, not a formality: it is what makes the change
reviewable by someone who was not there when it was made.
