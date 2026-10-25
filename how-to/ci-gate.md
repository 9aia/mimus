---
url: /mimus/how-to/ci-gate.md
---
# How-to: gate CI on docs

`mimus check` is the mechanical tier — deterministic, offline,
safe to gate. Exit `1` when any `error`-severity violation
exists.

## The gate

```sh
mimus check
```

Wire it like a test: run in CI on every PR. Doc paths, commands,
symbols and your `rules/*.toml` conventions all gate the same
way.

## `--no-cmd` — never execute anything

The `cmd` checker verifies documented commands exist on PATH —
it does **not** run them. But if you want to be explicit that a
job runs nothing executable:

```sh
mimus check --no-cmd
```

`--no-cmd` skips the command checker entirely; path/symbol/rules
checks still run.

## Severity is the gate boundary

* `error` — mechanical, reproducible: dead paths, missing
  binaries, rule violations marked `severity = "error"`.
* `warn` — env-dependent or heuristic: shown in output and
  `--format json`, never fails the run.

Tune per rule in `rules/*.toml` (`severity = "warn"`).

## Report for humans

```sh
mimus report                    # always exits 0 — full list
mimus check --format json       # machine-readable for tooling
```

`report` is the audit-view command: same violations, never a
gate. `reconcile`-style tooling consumes the JSON.

## Recommended setup

```yaml
# whatever your CI is
- run: mimus check --no-cmd
```

Docs that can drift silently are docs that will. One line keeps
them honest.

### Gating only new drift (large backlogs)

`mimus check --diff <spec>` runs the same checkers but only over
claims in files matching `git diff --name-only <spec>` — PR
checks fail on *new* drift while the existing backlog is paid
down:

```yaml
- run: mimus check --diff "origin/${{ github.base_ref }}...HEAD"
```

`--diff` needs a git checkout — outside a repo it exits 2.

See [claims & tiers](../explanation/claims.md) for why the
semantic tier can never be this gate.
