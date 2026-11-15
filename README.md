# mimus

*Verification engine — proofs over what agents claim.*

A critic for repos and agent memory. Extracts claims from docs,
comments, and `.kuda/` cards; verifies them against the code —
mechanically where it can, flagged-with-evidence where it can't —
and repairs the mechanically-fixable ones. Named for *Mimus
polyglottos*, the mockingbird that spots impostor eggs in its nest.

```
sources ──► extract ──► classify ──► verify ──► report/fix
    │         │           │           │           │
 docs/*.md  claims     path/cmd/   fs PATH     violations
 comments   +lines    symbol/     kuda idx    + evidence
 .kuda/               count…      rules/*.toml + rewrites
 cards
```

## Quickstart

Platform: **Linux + git**. The `symbol` checker, `audit`, and
`coverage` want a [`kuda`](https://github.com/9aia/kuda) index —
without one the mechanical tier is paths + commands + rules.

```sh
# install — tamias pack (libraries/mimus):
scripts/pack.sh && tamias install --allow-drivers ./libraries/mimus
# or a source build (compat bin, not on crates.io):
cargo install --git https://github.com/9aia/mimus mimus

cd your-repo
mimus init          # mimus.toml + rules/
mimus check         # mechanical tier; exit 1 on errors
mimus check --diff "origin/main...HEAD"   # only claims in changed files
mimus fix --dry     # preview dead-path rewrites; mimus fix applies
mimus audit         # trust-score .kuda/ cards
mimus doctor        # checker/config/index health
```

Two tiers: **mechanical** (paths, commands, symbols, rules —
CI-gateable) and **semantic** (advisory, human adjudicates — can
never gate). Every violation carries claim + proof + fix; a checker
that can't cite both emits nothing.

## Why

Linters check code shape; mimus checks *claims about reality* —
"this command works", "this path exists", "this symbol lives where
the doc says". It's the falsifier for agent memory: `mimus audit`
trust-scores kuda's cards against source. **kuda remembers; mimus
doubts.**

## Docs

- [Docs site](https://9aia.com/mimus) — tutorials, how-tos,
  reference, explanation (VitePress + Diátaxis, `docs/` worktree)
- [AGENTS.md](AGENTS.md) — crate map, commands, rules, boundary
- [CONTRIBUTING.md](CONTRIBUTING.md) · [SECURITY.md](SECURITY.md)
- [LICENSE](LICENSE) (MIT) · [project ledger](project/index.md)
