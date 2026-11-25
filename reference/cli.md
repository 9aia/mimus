---
url: /mimus/reference/cli.md
---
# Command-Line Help for `mimus`

This document contains the help content for the `mimus` command-line program.

Spots the cuckoo eggs in your repo — claim verification: docs-as-tests, memory audit

**Command Overview:**

* [`mimus`↴](#mimus)
* [`mimus init`↴](#mimus-init)
* [`mimus check`↴](#mimus-check)
* [`mimus fix`↴](#mimus-fix)
* [`mimus audit`↴](#mimus-audit)
* [`mimus report`↴](#mimus-report)
* [`mimus doctor`↴](#mimus-doctor)
* [`mimus coverage`↴](#mimus-coverage)
* [`mimus claims`↴](#mimus-claims)
* [`mimus standard`↴](#mimus-standard)

## `mimus`

Spots the cuckoo eggs in your repo — claim verification: docs-as-tests, memory audit

**Usage:** `mimus [OPTIONS] <COMMAND>`

###### **Commands:**

* `init` — Detect sources -> mimus.toml + rules/ scaffold
* `check` — Verify claims. Mechanical tier by default
* `fix` — Apply mechanical rewrites for confirmed violations
* `audit` — Trust-score .kuda/ cards vs source
* `report` — Full violations list with evidence
* `doctor` — Checker/config/kuda-index health
* `coverage` — Doc coverage — which indexed symbols no doc ever names. The reverse direction of `check`: code has it, docs silent
* `claims` — Dump extracted claims (kind/source/line) — tuning aid for the extractor's precision on real docs
* `standard` — Standard-tier repo conformance — declarative spec file-set diff plus predicate/provider tiers (P-001/P-002)

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help
* `-V, --version` — Print version

## `mimus init`

Detect sources -> mimus.toml + rules/ scaffold

**Usage:** `mimus init [OPTIONS]`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help

## `mimus check`

Verify claims. Mechanical tier by default

**Usage:** `mimus check [OPTIONS]`

###### **Options:**

* `--all` — Include the semantic tier (advisory only, never gates)
* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `--no-cmd` — Skip command claims — the PATH probe is machine-dependent
* `--diff <SPEC>` — Only check claims in files matching `git diff` SPEC — e.g. `--diff origin/main...HEAD` gates a PR on new drift
* `-h, --help` — Print help

## `mimus fix`

Apply mechanical rewrites for confirmed violations

**Usage:** `mimus fix [OPTIONS]`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--dry` — Show the plan, write nothing
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `--semantic` — Agentic tier: agent proposes corrected claim text for semantic (REFUTED) findings. Needs \[check].acp_cmd
* `-h, --help` — Print help

## `mimus audit`

Trust-score .kuda/ cards vs source

**Usage:** `mimus audit [OPTIONS]`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help

## `mimus report`

Full violations list with evidence

**Usage:** `mimus report [OPTIONS]`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help

## `mimus doctor`

Checker/config/kuda-index health

**Usage:** `mimus doctor [OPTIONS]`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help

## `mimus coverage`

Doc coverage — which indexed symbols no doc ever names. The reverse direction of `check`: code has it, docs silent

**Usage:** `mimus coverage [OPTIONS]`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help

## `mimus claims`

Dump extracted claims (kind/source/line) — tuning aid for the extractor's precision on real docs

**Usage:** `mimus claims [OPTIONS]`

###### **Options:**

* `--all` — Include card-prose Behavior claims too
* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `-h, --help` — Print help

## `mimus standard`

Standard-tier repo conformance — declarative spec file-set diff plus predicate/provider tiers (P-001/P-002)

**Usage:** `mimus standard [OPTIONS] --repo <REPO> --spec <SPECS>`

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--repo <REPO>` — Repo path to check
* `--format <FORMAT>` — Output format: human | json \[default: human]
* `--spec <SPECS>` — Spec file or directory of specs — repeatable. Specs whose `match` globs don't cover the repo name are skipped; empty `match` applies to every repo
* `-h, --help` — Print help
