# mimus

*Spots the cuckoo eggs in your repo.*

A critic for repos and agent memory. Extracts claims from docs,
comments, and index cards; verifies them against the code —
mechanically where it can, flagged-with-evidence where it can't —
and **repairs** the mechanically-fixable ones. Named for *Mimus
polyglottos*, the mockingbird that spots impostor eggs in its nest.

```
sources ──► extract ──► classify ──► verify ──► report/fix
    │         │           │           │           │
 docs/*.md  claims     path/cmd/   fs PATH     violations
 comments   +lines    symbol/     kuda idx    + evidence
 .kuda/               count…      rules/*.toml + rewrites
 cards
```

## Features

- **Evidence or silence** — every violation carries claim + proof
  + fix; a checker that can't cite both emits nothing. False
  positives kill a critic faster than false negatives
- **Two tiers**: **mechanical** (paths, commands, symbols, rules —
  CI-gateable, exit 1) and **semantic** (advisory, human adjudicates
  — can never gate). No style opinions, ever
- **`mimus fix`** — rewrites dead `path` claims to the moved file
  (unique-basename match); `--dry` previews. Everything else flags
  for a human — no silent "fixes"
- **Memory audit** — `mimus audit` trust-scores `.kuda/` cards
  against source via provenance hashes (fresh / drifted /
  contradicted). kuda remembers; mimus doubts, even the memory
- **Checkers**: `fs` (paths) · `cmd` (binary on PATH — never
  executes) · `symbol` (real `.kuda/index.sqlite` schema —
  namespaced claims refuted, externals silent) · `rules/*.toml`
  conventions as data
- **Command extraction is conservative** — first-token binary
  allowlist, so `pub fn` never reads as shell

## vs. a linter

- linters check code shape; mimus checks *claims about reality* —
  "this command works", "this path exists", "this symbol lives
  where the doc says"
- real-world dogfood: 1,530 stale references found in a ~100-module
  repo's docs; 340 of them mechanically fixable

## Install

Platform: **Linux + git**. `symbol` checker wants a `kuda` index.

```sh
cargo install --git https://github.com/9aia/mimus mimus
```

## Quickstart

```sh
cd your-repo
mimus init          # mimus.toml + rules/
mimus check         # mechanical tier; exit 1 on errors
mimus check --no-cmd    # skip the command checker
mimus fix --dry     # preview dead-path rewrites
mimus fix           # apply them
mimus audit         # trust-score .kuda/ cards
mimus report        # full list w/ evidence (--format json)
mimus doctor        # checker/config/index health
```

## The pair

**mimus** doubts; **[kuda](https://github.com/9aia/kuda)** remembers
— index, cards, queries, MCP/ACP surfaces. `mimus audit` is the
falsifier for that memory.

## Docs

Full docs: **[9aia.com/mimus](https://9aia.com/mimus/)** —
tutorial, how-tos, reference, explanation (Diátaxis).

[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) ·
[LICENSE](LICENSE) (MIT) · [project](project/index.md)
