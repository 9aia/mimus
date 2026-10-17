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
