+++
id = "P-001"
title = "Standard tier — file-based repo conformance for the org fleet"
status = "open"
created = "2026-11-23"
owner = "devin"
priority = "med"
+++

# Standard tier — file-based repo conformance for the org fleet

## Problem

Org repos drift from the house standard (README rules, manifest
metadata, license, gate wiring, `.hnl`-only targets). Today the only
enforcement is prose — `templates/README/README.md` rules,
`workspace/audit-*.md` snapshots, and ad-hoc `scripts/*-gate.sh`
loops. Nothing is declarative, mechanically checkable, or cacheable.

## Idea

Add a **standard tier** to mimus: repo conformance is just another
kind of claim — "this repo matches its family's expected tree."

### Expected trees (file-based contract)

The standard is expressed as a directory of expected files —
`templates/expected/<family>/` in the owning workspace repo —
where the filename itself carries the check:

| Filename form | Meaning |
| --- | --- |
| `README.md` / `README.md.required` | file must exist |
| `<dir>/.required-dir` | directory must exist |
| `LICENSE!MIT` | file must exist and match `MIT` |
| `<name>.check` | sidecar predicates, one per line (`field:`, `regex:`, `forbid:`) |
| `<name>.prose` | prose DSL (`words=20..400`, `bare-url`, `forbid-phrase=`) |
| `.forbid/<path>` | path must not exist / not be git-tracked |

A sibling `_config.toml` carries what filenames can't:

```toml
family   = "engine-compile"
match    = ["engine-compile-*", "engine-*"]   # repo-name globs
severity = "fail"                              # or warn per repo
claims   = ["@handl:family", "@handl:license"] # in-file annotations
semantic = ["README describes what the crate does"]  # LLM tier
```

In-file `@handl:` annotations (comment syntax per language: `#`,
`//`, `<!-- -->`, `//!`) let a repo self-declare facts the tree
verifies — family, stack, license, gate — keeping claims next to
the truth they describe.

### Evaluator

New `mimus standard` surface inside the existing ports-and-adapters
topology:

- `mimus-check-standard` — `Checker` impl: walk expected tree vs
  repo tree, eval `.check`/`.prose` predicates, verify `@handl:`
  claims. Mechanical, deterministic, gates.
- Semantic claims route to the existing `mimus-check-acp` tier —
  advisory only, same rule as today.
- Cache: results keyed by `(rule_version, sha256(file-set))` —
  unchanged repos skip entirely; this is what makes a ~150-repo
  fleet sweep cheap enough to run on every push.
- Parallel: rayon across repos × predicates; `--json` output
  consumable by bupha/lasius.

### Placement

- `expected/` trees live in the workspace repo next to
  `templates/README/` — file-tree is the machine-readable twin of
  the prose rules; `audit-*.md` becomes generated output, not
  hand-maintained truth.
- mimus ships a small builtin `expected/` fallback; org trees win
  via `mimus.toml`.
- Output plugs into the existing fleet machinery: bupha campaign
  for continuous drift, `lasius check` domain later if a workspace
  adopts lasius.

## Non-goals (v1)

- No auto-fix beyond what `mimus fix` already does for mechanical
  violations (scaffold-missing is `lasius conform`'s job).
- Semantic tier behind a flag until the mechanical tier proves out.
- Not a replacement for cypera (docs content) or the build gates —
  standard tier checks *structure and claims*, not prose quality.

## Open questions

- Predicate DSL final grammar for `.check` sidecars.
- Whether `@handl:` annotation prefix should be org-parameterized
  (`@9aia:`?) so mimus serves multiple orgs.
- Spike scope: `expected/engine/` tree + file-diff-only checker
  against the handl-lang sibling clones.
