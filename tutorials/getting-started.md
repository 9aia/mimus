# Getting started

Pick by what you need to do.

- **Tutorials** (learn): [first-run](/tutorials/first-run.md)
- **How-to** (tasks): [memory audit](/how-to/kuda-audit.md) ·
  [rules authoring](/how-to/rules-authoring.md)
- **Reference** (lookup): [cli](/reference/cli.md) ·
  [config](/reference/config.md) ·
  [crate api](/reference/crate-api.md)
- **Explanation** (why): [architecture](/explanation/architecture.md) ·
  [concepts](/explanation/concepts.md) ·
  [claims & tiers](/explanation/claims.md) ·
  [decisions](/explanation/decisions.md) ·
  [release model](/explanation/release-model.md) ·
  [comparison](/comparison.md) — vs linters / LLM review

Install — the `mimus` command ships as a tamias pack
(`libraries/mimus`): a generated `bins[]` shim →
`tamias-driver-mimus cli` → the engine lib's `run_from`. With
`tamias` installed:

```sh
scripts/pack.sh && tamias install --allow-drivers ./libraries/mimus   # from a checkout
tamias install 9aia:mimus@<version>    # org registry, once published
```

A source build also works — the engine ships a compat `mimus` bin
(not on crates.io; org policy):

```sh
cargo install --git https://github.com/9aia/mimus mimus
```

First check:

```sh
cd your-repo
mimus init    # detect claim sources -> mimus.toml + rules/
mimus check   # mechanical tier: paths, commands, symbols
mimus report  # violations with claim + evidence + fix
```

`symbol` claims, `audit`, and `coverage` need a
[kuda](https://github.com/9aia/kuda) index (`kuda sync`) — without
one, the mechanical tier is paths + commands + rules.
