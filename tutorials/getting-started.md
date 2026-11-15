---
url: /mimus/tutorials/getting-started.md
---
# Getting started

Point mimus at a repo's docs and see which claims are still true —
docs-as-tests, in minutes.

## Install

The `mimus` command ships as a tamias pack (`libraries/mimus`): a
generated `bins[]` shim → `tamias-driver-mimus cli` → the engine
lib's `run_from`. With `tamias` installed:

```sh
scripts/pack.sh && tamias install --allow-drivers ./libraries/mimus   # from a checkout
tamias install 9aia:mimus@<version>    # org registry, once published
```

A source build also works — the engine ships a compat `mimus` bin
(not on crates.io; org policy):

```sh
cargo install --git https://github.com/9aia/mimus mimus
```

## 1. Init

```sh
cd your-repo
mimus init
```

Writes a commented `mimus.toml` and a `rules/` dir. Defaults:

```toml
[sources]
docs = ["README.md", "docs/**/*.md", "AGENTS.md", "project/**/*.md"]
comments = true
cards = ".kuda/modules/*.yaml"

[check]
mechanical = true
semantic = false
```

`symbol` claims, `audit`, and `coverage` need a
[kuda](https://github.com/9aia/kuda) index (`kuda sync`) — without
one, the mechanical tier is paths + commands + rules.

## 2. Check

```sh
mimus check
```

Claim extraction scans every configured doc: backticked paths,
shell commands, symbol names, counts, links. Each claim is
verified against reality — the filesystem, PATH, the kuda index
(symbol claims need `kuda sync`; without an index they're
silence, not failures):

```
README.md:42 [error] `src/old_api.rs` — ./src/old_api.rs does not exist
docs/guide.md:17 [warn] `cargo bench` — binary `cargo-bench` not found in PATH
2 violation(s)
```

Exit code is `1` on any `error`-severity violation — docs drift
becomes a gateable signal. `mimus report` lists violations with
claim + evidence + fix.

## 3. Make a claim and watch it get caught

Add a line to your README referencing a file that does not exist:

```md
See `nonexistent/dead.rs` for the implementation.
```

```sh
mimus check
```

```
README.md:N [error] `nonexistent/dead.rs` — ./nonexistent/dead.rs does not exist
```

Every violation is claim + evidence + (when mechanical) a fix —
the evidence bar is the design rule, see
[architecture](/explanation/architecture).

## 4. Author a convention

`rules/` is data. Add `rules/no-todos.toml`:

```toml
name = "no-todos"
message = "TODO left in committed source"
severity = "warn"
files = "src/**/*.rs"

[match]
kind = "source-pattern"
pattern = "TODO|FIXME"
```

`mimus check` now flags every TODO — a convention encoded as a
file, not a code change. See [authoring rules](/how-to/rules-authoring).

## 5. Audit the memory layer

If the repo is kuda-indexed:

```sh
kuda sync && mimus audit
```

```
fresh        kuda-store
drifted      kuda-core
20 fresh, 1 drifted, 0 contradicted
```

Cards whose `verify:` files changed since sync report `drifted`;
missing files report `contradicted`.

## What you now know

* `mimus init` scaffolds `mimus.toml` (sources, `[check]` tiers)
  plus a `rules/` dir of data-driven conventions.
* `check` extracts factual claims from docs and verifies them
  against the filesystem, PATH, and the kuda index — violations
  are claim + evidence + fix, and `error` severity gates CI.
* `rules/*.toml` encode conventions without code changes;
  `audit` checks kuda card freshness (`fresh`/`drifted`/
  `contradicted`).

You have built a falsification layer: every factual claim in your
docs is now a test that fails loudly when reality moves.

## Where next

* **How-to** (tasks): [memory audit](/how-to/kuda-audit) ·
  [rules authoring](/how-to/rules-authoring) ·
  [CI gate](/how-to/ci-gate) · [multi-repo](/how-to/multi-repo)
* **Reference** (lookup): [cli](/reference/cli) ·
  [config](/reference/config) · [crate api](/reference/crate-api)
* **Explanation** (why): [architecture](/explanation/architecture) ·
  [concepts](/explanation/concepts) ·
  [claims & tiers](/explanation/claims) ·
  [decisions](/explanation/decisions) ·
  [release model](/explanation/release-model) ·
  [comparison](/comparison) — vs linters / LLM review
