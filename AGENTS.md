# Repository instructions

Read `coding_standards.md` before editing. Follow the ADRs. Keep
`docs/verification.md` honest about what works and what is still open. Every
review loop must run `python3 scripts/gate.py` and obtain independent specialist
agent reviews for DDD, organization/design patterns, and adversarial correctness.
Record and resolve findings before claiming a milestone passed.
User authorization permits commits and pushes to main in this private repository.
Do not turn experiment fixtures into claims of implemented runtime behavior.
