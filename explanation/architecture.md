---
url: /mimus/explanation/architecture.md
---
# Explanation: architecture

mimus is the falsification half of the ecosystem: *"every claim
in your docs, comments, and memory — still true?"* It exists
because memory without doubt decays into confident confabulation.

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

* `mimus-check` (the checker port) → `mimus-check-fs`,
  `mimus-check-cmd`, `mimus-check-symbol` (against kuda's index)
* claim sources: markdown today, comments/cards wired in config
* `mimus-rules` — conventions as `rules/*.toml` data
* `mimus-vcs`→`mimus-git`, `mimus-config`→`mimus-config-toml`,
  `mimus-secret`→`mimus-keyring`/`mimus-dotenv`

A new checker = new adapter crate; a new convention = a new
rule file.

## The evidence bar

A violation is claim + evidence + suggested fix. No evidence,
no finding — false positives kill a critic faster than false
negatives. The semantic tier can never gate CI: it reports, a
human adjudicates. Style opinions are never emitted at all.

## Position in the ecosystem

* `kuda` builds the index; `mimus` doubts it — `audit`
  trust-scores cards against source via provenance hashes.
* `reconcile` (planned third tool) auto-fixes what mimus
  confirms: claims rewritten to match reality — it consumes the
  JSON violation contract, not the checkers.

See [claims & tiers](claims.md) for the taxonomy and why the
two tiers can never merge.
