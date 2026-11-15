# Concepts

mimus's vocabulary centers on one idea: a **claim** is a statement
in your repo that is either still true or quietly false, and every
finding must carry **evidence** or not exist.

## Entities

- **Claim** — a checkable assertion extracted from a source:
  a backticked span, a code block, or a prose pattern. Always
  carries `source` + `line` so evidence can point back.
- **Claim kind** — the taxonomy bucket (`path`, `command`,
  `symbol`, `count`, `link`, `behavior`) that decides which
  checker applies. See [claims & tiers](claims.md) for the table.
- **Violation** — a proven false claim: `claim + evidence +
  fix(optional)`. No evidence, no violation.
- **Rule** — a convention declared as data in `rules/*.toml`
  (regex + message + severity), verified by the rules checker.
  A bad rule regex is silence plus a `doctor` complaint — rules
  can never crash the gate.
- **Trust score** — `mimus audit`'s verdict on a kuda card:
  how much of the card's provenance still matches source.

## Tiers

- **Mechanical** — deterministic, offline checkers (`fs`, `cmd`,
  `symbol`, `rules`). The `mimus check` gate; evidence is
  reproducible by anyone.
- **Graph** — the set-level pass (ADR-0002): claims become
  `(subject, relation, object)` triples bucketed by subject;
  contradictions, stale references, undocumented modules.
  Mechanical speed — buckets, not agents.
- **Semantic** — the `acp` checker asks an external agent to
  adjudicate fuzzy residue. Advisory only (`warn`), can never
  gate CI — that is load-bearing, not a limitation.

## Sources

`[sources]` in `mimus.toml`: `docs` (markdown globs), `comments`
(`///` and code comments), `cards` (the `.kuda/` artifacts —
kuda's memory audited as claims too).

## Sinks

Where findings go: human-readable report, `--json` for machines,
exit 1 for CI gating, and the `coverage` command's advisory
symbol list (never part of the gate).
