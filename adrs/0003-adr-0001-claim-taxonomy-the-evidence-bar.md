+++
id = "ADR-0003"
title = "ADR-0001: Claim taxonomy + the evidence bar"
status = "open"
priority = "medium"
+++

# ADR-0001: Claim taxonomy + the evidence bar

Status: proposed

## Context

Repo docs, comments, and agent-memory cards accumulate claims that
drift from reality. A critic tool dies from false positives within
a week of adoption — so the design must start from *what can be
proven*, not from what an LLM finds suspicious.

## Options

**A. LLM review pass over docs** — broad coverage, unverifiable
output, noisy. Rejected as the core mechanism (viable as one
checker behind the port, report-tier only).

**B. Rule-matching linter over prose** — catches known-bad patterns
but can't verify factual claims. Partial — kept as `mimus-rules`.

**C. Claim extraction + typed verification (recommended)** — parse
claims into a taxonomy; each claim type routes to the checker that
can actually prove or refute it; unverifiable claims don't produce
findings.

## Proposal

- Claim types: `path` (file/dir referenced exists), `command`
  (shell line parses / runs clean), `symbol` (name resolves in the
  kuda index), `count` (asserted numbers recompute), `link` (URL
  resolves), `behavior` (prose vs implementation — llm tier).
- Tiers: **mechanical** = deterministic, CI-gateable; **semantic**
  = reported with evidence, human adjudicates; **style** = never
  emitted.
- Every violation = claim + evidence + suggested fix. No evidence,
  no finding ("evidence or silence").
- `mimus audit` applies the same machinery to `.kuda/` cards —
  memory gets a trust score, not a free pass.
- Rules are data (`rules/*.toml`) — conventions as files, not code.

## Consequences

- Coverage is deliberately narrow at first; a small true-positive
  list beats a large maybe-wrong one.
- Depends on `kuda` for the `symbol` checker — first real cross-
  tool dep in the 9aia ecosystem, exercised early.
- Semantic tier is opt-in and can never gate CI — keeps the tool's
  credibility for the tier that can.
