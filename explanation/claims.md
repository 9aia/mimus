---
url: /mimus/explanation/claims.md
---
# Explanation: claims, tiers, and the evidence bar

mimus is built on one observation: documentation is full of
*claims* — statements that are either still true or quietly
false. A critic's job is to enumerate them and prove each one,
or stay silent.

## Claim taxonomy

Extraction classifies backticked spans, code blocks and prose
patterns into:

| kind      | example claim                        | verified by                |
| --------- | ------------------------------------ | -------------------------- |
| `path`    | `` `src/lib.rs` ``                   | filesystem                 |
| `command` | `` `cargo test -p foo` ``            | binary on PATH (no exec)   |
| `symbol`  | `` `IndexStore` ``                   | kuda index (symbol checker)|
| `count`   | "357 suites / 4,298 tests"           | recomputation              |
| `link`    | `[docs](https://…)`                  | link checker               |
| `behavior`| prose asserting code does X          | rules + semantic tier      |

Command claims are conservative by design: only spans whose
first token is a known binary (`cargo`, `git`, `npm`, `dagger`,
`kuda`, `mimus`, …) classify as commands — otherwise `pub fn`
and `data Eq` get read as shell.

## Verification tiers

* **Mechanical** — deterministic and offline: paths, commands,
  rules. This is the `mimus check` gate; if it flags something,
  the evidence is reproducible by anyone.
* **Semantic** — advisory: prose-vs-behavior ("retries 3 times"
  vs the code's 5), comment-vs-adjacent-code, ADR-vs-reality.
  Reports with evidence; a human adjudicates. **Can never gate**
  — that is load-bearing, not a limitation.

Style opinions are never emitted at all.

## The evidence bar

Every violation is `claim + evidence + fix(optional)`. If a
checker cannot cite its proof, it emits nothing — false
positives kill a critic faster than false negatives ever could.
This is why `command` has an allowlist, why a bad rule regex is
silence + a doctor complaint, and why the semantic tier exists
separately from the gate.

## Sources

`[sources]` in `mimus.toml`: `docs` (glob list), `comments`
(code comments — TODOs, claims in `///`), and `cards` — the
kuda artifacts get audited as claims too, closing the loop:
kuda remembers, mimus doubts, even about the memory itself.
