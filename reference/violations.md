# Reference: violation format

`mimus check --format json` (and `mimus report --format json`)
emit a JSON array of violations — the contract anything
consuming mimus output (reconcile, CI annotations, dashboards)
should code against.

## Shape

```json
[
  {
    "claim": {
      "kind": "path",
      "text": "crates/old-crate/src/lib.rs",
      "source": "docs/guide.md",
      "line": 17
    },
    "evidence": "./crates/old-crate/src/lib.rs does not exist",
    "fix": "remove the reference, or restore the file",
    "severity": "error"
  }
]
```

## Fields

| field             | meaning                                            |
| ----------------- | -------------------------------------------------- |
| `claim.kind`      | `path` `command` `symbol` `count` `link` `behavior` |
| `claim.text`      | the claim as written in the source                  |
| `claim.source`    | file the claim was extracted from                   |
| `claim.line`      | 1-based line number                                 |
| `evidence`        | what reality says instead — the proof               |
| `fix`             | suggested correction, or `null`                     |
| `severity`        | `error` (gates `check`) \| `warn` (report only)     |

## Human format

Default output is one line per violation:

```
docs/guide.md:17 [error] `crates/old-crate/src/lib.rs` — ./crates/old-crate/src/lib.rs does not exist
N violation(s)
```

`source:line [severity] `claim` — evidence` — stable enough to
grep, intended for terminals not parsers.

## Exit codes

| code | meaning                                         |
| ---- | ----------------------------------------------- |
| 0    | clean — or `report`, which never gates          |
| 1    | `error`-severity violation(s), or usage failure |

`audit` always exits 0 — its scores are report-tier.

## What is silence, not a violation

mimus's rule is evidence-or-silence: a claim a checker cannot
prove false emits nothing. Concretely, none of these produce a
violation:

- `/leading-slash` spans — read as URL routes or host-absolute
  paths, never repo-relative claims
- `a/b`-shaped misses (`application/pdf`, `tools/list`) — two
  lowercase-word segments read as MIME types / RPC methods
- `VAR=value` env assignments, `...`-/brace-/`<>`-template
  spans, bare extensions (`.md`), serial placeholders
  (`adr-mkt-0N.md`)
- `~`- and `$`-prefixed paths, and globs verified by expansion
- command claims whose binary is declared in
  `[check].binaries`, or all command claims under `--no-cmd`

## Tuning and suppression

- `[check].kinds` / `[check].ignore_prefixes` / `base_dirs` —
  see [config](config.md)
- `<!-- mimus-ignore -->` — alone on a line, suppresses the next
  line's claims; inline, suppresses its own line.
  `<!-- mimus-ignore-file -->` skips the whole file.
- the `cmd` checker probes the machine's PATH — results vary
  across machines by design (missing binary = `warn`, never
  `error`). Declare prerequisites in `[check].binaries`, or run
  `check --no-cmd`.
