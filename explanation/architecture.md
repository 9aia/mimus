# Explanation: architecture

## Claims, not opinions

Docs and memory cards make *claims*: this path exists, this
command works, this symbol lives here, this number is current.
mimus extracts claims into a taxonomy — each type routes to the
checker that can actually prove or refute it:

| claim      | checker                | tier       |
| ---------- | ---------------------- | ---------- |
| `path`     | `check-fs`             | mechanical |
| `command`  | `check-cmd`            | mechanical |
| `symbol`   | `check-symbol` (kuda)  | mechanical |
| `count`    | `check-cmd`/`check-fs` | mechanical |
| `link`     | `check-link` (planned) | mechanical |
| `behavior` | `check-llm` (planned)  | semantic   |

## The evidence bar

A violation is claim + evidence + suggested fix. No evidence, no
finding — false positives kill a critic faster than false
negatives. The semantic tier can never gate CI: it reports, a
human adjudicates. Style opinions are never emitted at all.

## Ports and impls

Every concern is a trait crate + impl crates: checkers
(`fs` | `cmd` | `symbol` | `llm`), claim sources (md, comments,
cards), sinks (cli, commit-status). Rules are data
(`rules/*.toml`) — conventions as files, not code.

## Position in the ecosystem

- `kuda` builds the index; `mimus` doubts it — `audit`
  trust-scores cards against source via the `symbol` checker.
- `reconcile` (planned third tool) auto-fixes what mimus
  confirms: claims rewritten to match reality.
