# Command-Line Help for `mimus`

This document contains the help content for the `mimus` command-line program.

**Command Overview:**

* [`mimus`↴](#mimus)
* [`mimus init`↴](#mimus-init)
* [`mimus check`↴](#mimus-check)
* [`mimus audit`↴](#mimus-audit)
* [`mimus report`↴](#mimus-report)
* [`mimus doctor`↴](#mimus-doctor)

## `mimus`

Spots the cuckoo eggs in your repo — claim verification: docs-as-tests, memory audit

**Usage:** `mimus [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `init` — Detect sources -> mimus.toml + rules/ scaffold
* `check` — Verify claims. Mechanical tier by default
* `audit` — Trust-score .kuda/ cards vs source
* `report` — Full violations list with evidence
* `doctor` — Checker/config/kuda-index health

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
* `--no-cmd` — Never execute documented commands



## `mimus audit`

Trust-score .kuda/ cards vs source

**Usage:** `mimus audit`



## `mimus report`

Full violations list with evidence

**Usage:** `mimus report`



## `mimus doctor`

Checker/config/kuda-index health

**Usage:** `mimus doctor`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>



## claims

```console
$ mimus claims [--all]
```

Dump extracted claims (kind/source/line) — the tuning surface
for extractor precision. `--all` includes `.kuda` card prose.

## coverage

```console
$ mimus coverage
```

Reverse-direction check: indexed symbols (via `check.kuda_index`)
never named in any doc source. Per-module gap list; advisory.

## fix --semantic

```console
$ mimus fix --semantic [--dry]
```

Agentic repair tier: for grounded REFUTED findings the agent
proposes corrected claim text — `--dry` previews; prose docs only.
