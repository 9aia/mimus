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

extends = ["9aia/base", "handl-lang/engine"]   # compose other packs

providers = {                            # opt into tool expertise
  lasius  = { require = "clean-clone" }, # git/platform facts
  bupha   = { context = "ci/bupha" },    # CI wiring present
  cypera  = { profile = "engine" },      # docs contract subset
  kuda    = { cards = "fresh" },         # memory layer in sync
}

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

### Composition & providers

Specs are **composable packs**, distributed like everything else
in the org — via **tamias** (`libraries/<org>-standard` content
packs; `mimus-rules` is already a registered content type, so the
plumbing exists).

- `extends` chains merge parent → child; child overrides
  `severity`, `files`, per-path predicates. Diamond-safe: merge
  order is declared, conflicts are last-wins with a warning.
- A spec only declares its **delta** — `handl-lang/engine`
  extends `9aia/base` (LICENSE, README, AGENTS, docs contract)
  and adds `xtask/`, `workflows/gate.yaml`, manifest fields.
- **Providers** are the integration seam: each 9aia tool that has
  fleet-relevant expertise exposes a predicate namespace mimus
  can call. The spec doesn't know *how* `lasius:` or `bupha:`
  checks run — it declares intent; the provider crate implements
  it. New provider = new namespace, no spec-format change.

| Provider | Namespace it could own |
| --- | --- |
| lasius | `git.*`, clone hygiene, workspace conformance |
| bupha | `ci.*` — gate wiring, required status contexts |
| cypera | `docs.*` — profile pages, generated-ref freshness |
| kuda | `memory.*` — card drift, index freshness |
| myco | `ledger.*` — proposal/ADR hygiene |
| tamias | `pack.*` — installed-driver/bins consistency |

Provider predicates appear in specs like any other rule:

```toml
[path."workflows/gate.yaml"]
bupha.context = "ci/bupha"        # provider predicate, not mimus core

[repo]
lasius.clean = true               # no dirty tree / unpushed commits
```

Unknown namespaces fail spec-load with a clear error — a typo'd
provider is a spec bug, not a silent pass. Providers that aren't
installed degrade to `warn`-severity "provider unavailable"
findings, never hard-fail, so a minimal mimus install still runs
the core tier.

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

- `mimus-check-standard` — `Checker` impl: resolve `extends`
  chain → load spec → walk repo (git-tracked files only) →
  unlisted-file violations + per-path predicates (`manifest:`,
  `regex:`, `prose:`, `contains:`, `git:`) → claim verification.
  Mechanical, deterministic, gates.
- **Provider port** — a `StandardProvider` trait alongside the
  existing checker ports: `ns() -> &str`, `check(claim) ->
  Finding`. Core predicates ship in mimus; provider predicates
  dispatch by namespace to installed provider crates (possibly
  via their tamias driver surface rather than new crates).
- Semantic claims route to the existing `mimus-check-acp` tier —
  advisory only, same rule as today.
- Cache: results keyed by `(spec_version, provider_versions,
  sha256(file-set))` — unchanged repos skip entirely; makes a
  ~150-repo fleet sweep cheap enough for every push.
- Parallel: rayon across repos × predicates; `--json` output for
  bupha/lasius consumption.

### Reuse & boundaries (anti-duplicate-work map)

| Need | Existing | Decision |
| --- | --- | --- |
| pack fetch/store | tamias | reuse — resolution only |
| config loading | scarab-config | reuse — via mimus-config |
| docs predicates | cypera check | provider namespace, not reimpl |
| git/clone facts, repo discovery | lasius | provider + `--repos`/`lasius status --json` |
| card freshness | kuda | provider namespace |
| spec schema + merge | scarab-template COMPOSITION.md | **reuse contract** — `extends`/`merge`/ancestor-order/diamond-collapse already specified, tamias-install already materializes parents |
| document-level composition kernel | peregrin-loader-yaml `expand.rs`+`tamias.rs` (~850 ln, Value-tree generic) | **re-home in scarab** as `scarab-compose` — not a peregrin extraction; scarab owns config/template expertise |
| fleet enumeration | lasius | delegate when available; `--root`/`--repos` for v1 |

Spec resolution delegates to `scarab-compose` (document-level
`include:`/`extends:`/`deep_merge` over `Value` trees +
`tamias:` refs, COMPOSITION.md semantics). Spec *schema* +
predicate evaluation live in `mimus-spec` inside mimus behind a
port — extract to a shared crate only when a second evaluator
(lasius/cypera) actually consumes it.

### Placement

- Spec packs distribute via **tamias** — `libraries/<org>-standard`
  packs beside `templates/README/`; `mimus-rules` is already a
  content type so install/fetch machinery exists. A repo opts into
  a pack via `mimus.toml`; org default comes from the workspace
  repo. `audit-*.md` becomes generated output, not hand-maintained
  truth.
- mimus ships a small builtin spec set (`9aia/base`) as fallback;
  org packs win via `extends` resolution order.
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

- Spec discovery: `expected/*.toml` packs vs `[[standard]]`
  tables in `mimus.toml` — likely both (packs for org standards,
  inline for repo-local deltas/overrides).
- Provider ABI: in-process trait vs driver/subprocess boundary —
  tamias drivers already expose ops surfaces; reusing that keeps
  providers in their own repos without mimus linking them.
- Repo discovery: `--repos`/`--root` args vs shelling to
  `lasius status --json` — probably both (explicit paths always
  work; lasius is an optimization when present).
- Merge semantics edge cases: two parents declaring the same
  `path."X"` predicate — concat vs override; probably concat for
  predicates, last-wins for scalars (`severity`, `family`).
- Glob semantics for `files`/`dirs` — gitignore-style vs
  globset; needs an "unlisted" carve-out story for generated dirs
  (`target/`, `node_modules/` are never tracked anyway — git
  walk makes this free).
- Spike scope: `engine` spec + file-set-diff-only checker against
  the handl-lang sibling clones.
