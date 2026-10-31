---
url: /mimus/reference/cli.md
---
# Command-Line Help for `mimus`

This document contains the help content for the `mimus` command-line program.

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

## `mimus`

Spots the cuckoo eggs in your repo — claim verification: docs-as-tests, memory audit

**Usage:** `mimus [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `init` — Detect sources -> mimus.toml + rules/ scaffold
* `check` — Verify claims. Mechanical tier by default
* `fix` — Apply mechanical rewrites for confirmed violations
* `audit` — Trust-score .kuda/ cards vs source
* `report` — Full violations list with evidence
* `doctor` — Checker/config/kuda-index health
* `coverage` — Doc coverage — which indexed symbols no doc ever names. The reverse direction of `check`: code has it, docs silent
* `claims` — Dump extracted claims (kind/source/line) — tuning aid for the extractor's precision on real docs

###### **Options:**

* `--config <CONFIG>` — Config path (else $MIMUS_CONFIG, ./mimus.toml, ~/.config/mimus/mimus.toml)
* `--format <FORMAT>` — Output format: human | json

  Default value: `human`

## `mimus init`

Detect sources -> mimus.toml + rules/ scaffold

**Usage:** `mimus init`

## `mimus check`

Verify claims. Mechanical tier by default

**Usage:** `mimus check [OPTIONS]`

###### **Options:**

* `--all` — Include the semantic tier (advisory only, never gates)
* `--no-cmd` — Skip command claims — the PATH probe is machine-dependent

## `mimus fix`

Apply mechanical rewrites for confirmed violations

**Usage:** `mimus fix [OPTIONS]`

###### **Options:**

* `--dry` — Show the plan, write nothing
* `--semantic` — Agentic tier: agent proposes corrected claim text for semantic (REFUTED) findings. Needs \[check].acp_cmd

## `mimus audit`

Trust-score .kuda/ cards vs source

**Usage:** `mimus audit`

## `mimus report`

Full violations list with evidence

**Usage:** `mimus report`

## `mimus doctor`

Checker/config/kuda-index health

**Usage:** `mimus doctor`

## `mimus coverage`

Doc coverage — which indexed symbols no doc ever names. The reverse direction of `check`: code has it, docs silent

**Usage:** `mimus coverage`

## `mimus claims`

Dump extracted claims (kind/source/line) — tuning aid for the extractor's precision on real docs

**Usage:** `mimus claims [OPTIONS]`

###### **Options:**

* `--all` — Include card-prose Behavior claims too

&#xA;This document was generated automatically by
clap-markdown.&#xA;
