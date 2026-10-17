# Tutorial: your first audit

Goal: point mimus at a repo's docs and see which claims are still
true — docs-as-tests, in minutes.

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

## 2. Check

```sh
mimus check
```

Claim extraction scans every configured doc: backticked paths,
shell commands, symbol names, counts, links. Each claim is
verified against reality — the filesystem, PATH, the kuda index:

```
README.md:42 [error] `src/old_api.rs` — ./src/old_api.rs does not exist
docs/guide.md:17 [warn] `cargo bench` — binary `cargo-bench` not found in PATH
2 violation(s)
```

Exit code is `1` on any `error`-severity violation — docs drift
becomes a gateable signal.

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
[architecture](../explanation/architecture.md).

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
file, not a code change. See [authoring rules](../how-to/rules-authoring.md).

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

## You have built

A falsification layer: every factual claim in your docs is now a
test that fails loudly when reality moves.

Next: [gate CI on it](../how-to/ci-gate.md),
[write rules](../how-to/rules-authoring.md).
