---
url: /mimus/explanation/decisions.md
---
# Decisions

ADR-lite — the decisions that shape mimus, the alternatives
rejected, and the consequence each carries.

## No evidence, no finding

**Decision:** every violation must cite claim + proof (+ optional
fix). A checker that cannot produce evidence emits nothing.

**Alternatives:** best-effort heuristics that flag *suspected*
issues (false positives kill a critic faster than false negatives);
LLM-judged everything (non-deterministic, ungateable, expensive).

**Consequence:** the mechanical tier is conservative — command
claims need a binary allowlist, ambiguous spans classify as prose,
bad rule regexes are silence + a doctor complaint. Coverage is
traded for trust.

## The semantic tier can never gate

**Decision:** the `acp` checker reports `warn`-tier findings only;
`mimus check`'s exit code is decided by mechanical + graph alone.

**Alternatives:** let the agent vote in CI (non-determinism makes
flaky gates; a provider outage would block merges); drop semantic
entirely (behavior claims — the majority of real doc rot — go
unchecked).

**Consequence:** semantic findings inform humans and agents but
can never break a build. Its intake is the graph tier's fuzzy
residue — pre-bucketed candidate pairs — not the raw claim set,
so the agent never scans every claim.

## Rules are data, not code

**Decision:** conventions live in `rules/*.toml` — regex, message,
severity — interpreted by one checker.

**Alternatives:** per-convention checker crates (adapter sprawl
for what is fundamentally a pattern match); hard-coded checks in
Rust (every org convention becomes a release).

**Consequence:** "dispatch via roles, never `kind ==`" is a file a
repo adds in a diff. The regex engine is the only thing that can
be wrong, and its failure mode is silence, not a crash.

## Claims fold into batches, debounced

**Decision:** semantic-tier prompts batch claims and pace calls;
mechanical findings short-circuit before any agent is invoked.

**Alternatives:** one prompt per claim (provider spend and rate
limits scale with doc size); synchronous agent calls inside the
gate (latency + flakiness in CI).

**Consequence:** the critic is polite to providers by
construction, and `--diff` scopes both extraction and graph
findings to what actually changed.
