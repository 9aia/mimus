---
url: /mimus/getting-started.md
---
# Getting started

Pick by what you need to do.

* **Tutorials** (learn): [first-run](tutorials/first-run.md)
* **How-to** (tasks): [memory audit](how-to/kuda-audit.md) ·
  [rules authoring](how-to/rules-authoring.md)
* **Reference** (lookup): [cli](reference/cli.md) ·
  [config](reference/config.md) ·
  [crate api](reference/crate-api.md)
* **Explanation** (why): [architecture](explanation/architecture.md) ·
  [release model](explanation/release-model.md) ·
  [comparison](comparison.md) — vs linters / LLM review

Install:

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
