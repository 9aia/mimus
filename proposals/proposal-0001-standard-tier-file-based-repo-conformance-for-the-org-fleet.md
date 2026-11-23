+++
id = "P-001"
title = "Standard tier — declarative repo conformance for the org fleet"
status = "open"
created = "2026-11-23"
owner = "devin"
priority = "med"
+++

# Standard tier — declarative repo conformance for the org fleet

## Problem

Org repos drift from the house standard (README rules, manifest
metadata, license, gate wiring, `.hnl`-only targets). Today the only
enforcement is prose — `templates/README/README.md` rules,
`workspace/audit-*.md` snapshots, and ad-hoc `scripts/*-gate.sh`
loops. Nothing is declarative, mechanically checkable, cacheable,
or composable.

## Idea

Add a **standard tier** to mimus: repo conformance is just another
kind of claim — "this repo's file set and contents match its
family's declared spec." The spec is **data**: a serializable
config, not a file tree. mimus already loads config
(`mimus-config`/scarab-config); the same machinery loads org
standard packs.

### Spec format — include-list, not deny-list

A standard is a named set of **expectations**. Semantics are
whitelist: **anything in the repo not covered by the spec is a
violation** (`unlisted`), so specs stay minimal and composition is
encouraged — a repo only declares what it needs, and adding a file
means adding it to the spec.

```toml
# expected/engine-compile.toml  (or inline in mimus.toml [[standard]])
family   = "engine-compile"
match    = ["engine-compile-*"]        # repo-name globs
severity = "fail"                        # or warn, per spec/repo

# file-set: exact paths or globs; directories declared explicitly
files = [
  "Cargo.toml", "README.md", "LICENSE", "AGENTS.md",
  "xtask/Cargo.toml", "workflows/gate.yaml",
  "src/**/*.rs",
]
dirs  = ["src", "tests", "xtask"]

# per-path predicates
[path."Cargo.toml"]
manifest.fields = ["description", "license", "repository", "readme"]
manifest.license = "MIT"
regex.forbid    = ['^\[patch\.']

[path."README.md"]
prose."## Description".words = "20..400"
prose."## Documentation".kind = "bare-url"     # or absent
forbid-phrase   = ["ADR-0", "part of the", "formerly known as"]

[path."LICENSE"]
contains = "MIT License"

# git-level rules
git.untracked = ["Cargo.lock"]     # must not be tracked
```

Non-file rules live in the same doc — manifest fields, prose
constraints, git-tracking, per-path regexes — because filenames
can't carry them and a tree-of-files spec can't express globs,
ranges, or "absent" cleanly.

### In-file claims (optional layer)

`@handl:` annotations (per-language comment token: `#`, `//`,
`<!-- -->`, `//!`) let a repo self-declare facts the spec verifies
— `family=`, `stack=`, `license=`, `gate=`. Claims sit next to the
truth they describe; the standard tier checks `claims ⊇ spec`
plus the mechanical predicates. Prefix org-parameterized (`@9aia:`)
so mimus serves multiple orgs.

### Evaluator

New `mimus standard` surface inside the existing
ports-and-adapters topology:

- `mimus-check-standard` — `Checker` impl: load spec → walk repo
  (git-tracked files only) → unlisted-file violations + per-path
  predicates (`manifest:`, `regex:`, `prose:`, `contains:`,
  `git:`) → claim verification. Mechanical, deterministic, gates.
- Semantic claims route to the existing `mimus-check-acp` tier —
  advisory only, same rule as today.
- Cache: results keyed by `(spec_version, sha256(file-set))` —
  unchanged repos skip entirely; makes a ~150-repo fleet sweep
  cheap enough for every push.
- Parallel: rayon across repos × predicates; `--json` output for
  bupha/lasius consumption.

### Placement

- Spec files live in the workspace repo beside `templates/README/`
  — serializable twin of the prose rules; `audit-*.md` becomes
  generated output, not hand-maintained truth.
- mimus ships a small builtin spec set as fallback; org specs win
  via `mimus.toml`.
- Output plugs into existing fleet machinery: bupha campaign for
  continuous drift; a `lasius check` domain later if wanted.

## Non-goals (v1)

- No auto-fix beyond `mimus fix`'s existing mechanical repairs
  (scaffold-missing stays `lasius conform`'s job).
- Semantic tier behind a flag until mechanical proves out.
- Not a cypera replacement (docs content) or build-gate
  replacement — standard tier checks structure, claims, and
  file-set hygiene only.

## Open questions

- Spec discovery: `expected/*.toml` dir vs `[[standard]]` tables
  in `mimus.toml` — likely both (dir for org packs, inline for
  repo-local overrides).
- Glob semantics for `files`/`dirs` — gitignore-style vs
  globset; needs an "unlisted" carve-out story for generated dirs
  (`target/`, `node_modules/` are never tracked anyway — git
  walk makes this free).
- Spike scope: `engine` spec + file-set-diff-only checker against
  the handl-lang sibling clones.
