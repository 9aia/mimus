# How to check repo conformance

`mimus standard` diffs a repo against declarative conformance specs —
the org's way of saying "every repo carries the same floor" without a
script per repo. Specs are TOML/YAML/JSON documents naming a `family`,
`match` globs, a `severity`, and the expected file set; anything
tracked in the repo that no spec covers is an `unlisted` violation.

## Run it

```sh
mimus standard --repo ../tamias --spec ../specs --spec ../tamias/specs
mimus standard --repo . --spec ./specs --format json
```

- `--spec` takes a spec **file or directory** and is repeatable.
  Specs whose `match` globs don't cover the repo name are skipped;
  an empty `match` applies to every repo.
- Repo enumeration prefers `git ls-files` — only tracked files are
  checked, so `target/` and other generated dirs need no carve-outs.
- Exit `1` when any `fail`-severity spec reports violations; `warn`
  specs report but never gate.

## What a spec checks

Three tiers, all declarative:

1. **File set** — exact `files` paths and `dirs` are required
   (missing → violation); glob `files` entries, `cover`, and
   `cover_dirs` whitelist tracked paths for the `unlisted` check
   without ever being required themselves. Coverage is unioned
   across every matched spec: a file covered by *any* spec is
   listed.
2. **Predicates** — `[path."<glob>"]` tables (`exists`, `contains`,
   `regex`, plus `except` — a repo-name glob list that skips the whole
   table for matching repos, the per-repo exemption hatch for shared
   base specs) and `[git]` claims (`untracked` globs flag
   untracked-but-not-ignored files).
3. **Providers** — `[providers.<ns>]` tables dispatch to a registered
   `StandardProvider`. Built in: `cargo` (org-local `[patch]`
   enforcement), `repo` (bupha.toml name/dir drift), `lasius`, `kuda`,
   `sturn`, `myco`, `cypera`. An unregistered namespace degrades to a
   `provider-unavailable` **warn** — never a hard fail.

## Reading the output

Each row is `(family, severity, violations)` — `missing` (required
files absent), `unlisted` (tracked files no spec covers), `predicate`
(path/git claim failures), and `provider` findings. `--format json`
emits the same rows machine-readably for CI.

## Verify

```sh
mimus standard --repo . --spec specs/base.toml
```

against a repo that meets the floor prints its matched families and
no `fail` violations; point `--spec` at a spec with a deliberately
absent `files` entry to see a `missing` violation fire.

## Deeper

- [CLI reference](../reference/cli.md#mimus-standard) — full flag list
- [Architecture](../explanation/architecture.md) — where the spec
  tier sits among the checkers
- Org usage: `specs/base.toml` is the fleet floor; per-repo specs live
  in `<repo>/specs/` and compose via union matching
