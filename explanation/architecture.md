# Explanation: architecture

mimus is the falsification half of the ecosystem: *"every claim
in your docs, comments, and memory — still true?"* It exists
because memory without doubt decays into confident confabulation.

## Where it sits

The org stack is **delivery → memory → process → interface**.
mimus is a **process-layer organism** — repo-local, deterministic at
its core, with advisory AI tiers that can never gate. It consumes
`kuda`'s index (symbol claims, card audits, the graph pass's edges)
and `sturn`'s WAL where present; its findings feed CI gates,
`pavo` views, and `peregrin` campaigns. It is the designated doubter
of every other organism's output.

## The pipeline

```
sources ──► extract ──► classify ──► verify ──► emit
   │          │           │           │          │
 docs/*.md  claims     kinds:     checkers:   human |
 comments   +lines     path…      fs cmd      json
 .kuda/                 behavior  symbol      exit 1
 cards                            rules
```

**Extract** pulls claim candidates (backticked spans, code
blocks, prose patterns) with source+line.

**Classify** sorts them into the taxonomy — conservatively: a
span that might be a command but isn't in the binary allowlist
is *not* a command claim.

**Verify** runs the mechanical checkers plus `rules/*.toml`.

**Emit** reports violations — or exits 1 to gate CI.

## Ports and adapters

`mimus-core` holds domain types only (`Claim`, `Violation`,
`TrustScore`, config). Every concern is a trait crate + adapter
crates:

- `mimus-check` (the checker port) → `mimus-check-fs`,
  `mimus-check-cmd`, `mimus-check-symbol` (against kuda's index),
  `mimus-check-acp` (the semantic tier);
  `mimus-check-graph` (ADR-0002) runs a *set-level*
  pass: claims + code edges become `(subject, relation, object)`
  triples, bucketed by subject — contradictions and stale
  references at mechanical speed; a second port, not a
  `Checker` impl, since it compares claims to each other
- claim extraction → `mimus-claims`; conventions → `mimus-rules`
  (`rules/*.toml` data, verified by the rules checker)
- `mimus-config` (port + scarab-config discovery,
  `mimus.{yaml,yml,toml}`); secrets resolve through cuora's
  `cuora-secret*` family (git deps — env → cuora vaults →
  keyring → dotenv)
- `mimus-spec` — the `mimus standard` tier: declarative
  repo-conformance specs (file-set diff + path/git predicates +
  `[providers.<ns>]` dispatch) as data, evaluated in-crate
- git access goes through the shared `lasius-vcs` port
  (`lasius-vcs-git` impl), not ad-hoc shell-outs

A new checker = new adapter crate; a new convention = a new
rule file; a new conformance rule = a spec file.

## The evidence bar

A violation is claim + evidence + suggested fix. No evidence,
no finding — false positives kill a critic faster than false
negatives. The semantic tier can never gate CI: it reports, a
human adjudicates. Style opinions are never emitted at all.

## Position in the ecosystem

- `kuda` builds the index; `mimus` doubts it — `audit`
  trust-scores cards against source via provenance hashes.
- `reconcile` (planned third tool) auto-fixes what mimus
  confirms: claims rewritten to match reality — it consumes the
  JSON violation contract, not the checkers.

See [claims & tiers](claims.md) for the taxonomy and why the
two tiers can never merge.

See [Boundaries](boundaries.md) for owns / does-not-own.
