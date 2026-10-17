# mimus

*Spots the cuckoo eggs in your repo.*

A critic for repos and agent memory. Extracts claims from docs,
comments, and index cards; verifies them against the code —
mechanically where it can, flagged-with-evidence where it can't.
Named for *Mimus polyglottos*, the mockingbird that spots
impostor eggs in its nest.

```
claims ──► extract ──► classify ──► verify ──► report
            │            │           │          │
        md/comments/  mechanical/  kuda index  CI gate +
        cards         semantic     + checkers  evidence
```

## Features

- evidence or silence — every violation carries the claim, the
  proof, and the fix; if a checker can't cite both, it doesn't
  emit (false positives kill critics faster than false negatives)
- three tiers: **mechanical** (paths/commands/symbols/counts —
  CI-gateable), **semantic** (prose vs behavior — reported for
  human adjudication), and a hard rule: no style opinions
- memory audit — verifies `kuda` cards against source and scores
  card trust; stale claims get demoted, not just refreshed
- every concern behind a port: checkers (`fs` | `cmd` | `symbol` |
  `llm`), claim sources, sinks — ports/impls are separate crates
- rules are data (`rules/*.toml`), not code — a convention like
  "dispatch via `lang.eval`, never `kind ==`" is a rule file
- docs-as-tests: `mimus check` in CI means docs can't silently rot

## vs. a linter

- linters check code shape; mimus checks *claims about reality* —
  "this command works", "this path exists", "this doc's numbers
  are current", "this ADR isn't superseded by the code"
- violations are enumerable and verifiable — a doc sweep becomes a
  checklist, not a vibe
- it audits the memory layer too: kuda remembers, mimus doubts

## Install

Platform: **Linux + git**. The `symbol` checker wants a `kuda`
index; the `llm` checker wants a provider key.

```sh
cargo install mimus                                  # crates.io (post-0.1.0)
cargo install --git https://github.com/9aia/mimus mimus   # today
```

## Quickstart

```sh
cd your-repo
mimus init            # detect sources -> mimus.toml + rules/
mimus check           # mechanical claims: paths, cmds, symbols
mimus check --all     # + semantic tier (reported, not gated)
mimus audit           # trust-score the .kuda/ cards
mimus report          # full violations list with evidence
mimus doctor          # checker/kuda-index health
```

## Docs

Full docs: **[9aia.com/mimus](https://9aia.com/mimus/)** —
tutorial, how-tos, cli/rules/crate-API reference, architecture
(Diátaxis).

[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) ·
[LICENSE](LICENSE) (MIT) · [project](project/index.md) — TODO, backlog, ADRs
