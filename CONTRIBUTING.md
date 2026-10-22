# Contributing

Issues and PRs are **disabled** — the project is agent-maintained and
dogfoods its own CI. Fork freely under MIT.

## Layout

`crates/` — rust workspace. `mimus-core` is pure domain (claims,
violations, evidence); ports are crates: `mimus-check`,
`mimus-claims`, `mimus-rules`, `mimus-vcs`; adapters:
`mimus-check-fs`/`mimus-check-cmd`/`mimus-check-symbol`/
`mimus-check-llm` (checkers), `mimus-kuda` (kuda index glue),
`mimus-git` (vcs); `mimus` (cli) is the composition root.
`modules/dagger/` is the shared pipeline module (Python SDK).

## Rules of thumb

- New checker/claim-source = a new adapter crate, not a core change.
  New convention = a `rules/*.toml` file, not an engine edit.
- Evidence or silence: a checker that can't cite claim + proof
  emits nothing.
- `cargo build --workspace` and `cargo test --workspace` must stay green
  (unit tests + the e2e in `crates/mimus-cli/tests/` — no docker needed).
- Local gate: `git config core.hooksPath .githooks` makes
  `scripts/check.sh` (`cargo check --workspace --all-targets`) run
  pre-push — the compile floor, since no hosted CI watches HEAD.
- Keep the binary dependency-light and sync; no async runtime.
- Commits: conventional prefixes (`feat:`/`fix:`/`refactor:`/`docs:`/`chore:`).
- Never commit secrets — `mimus.toml` takes names of env vars, not values.

## Releasing

Seed once with `mimus deploy release` (publishes workspace crates,
tags `{repo}-v{ver}`). Afterwards `scripts/release.sh` runs on each
`main` push via mimus's own dogfood loop — bump, publish, tag, dist
binaries. See `docs/explanation/release-model.md`.

## Docs

Follow Diátaxis: `docs/tutorials/` (learn) · `docs/how-to/` (tasks) ·
`docs/reference/` (lookup) · `docs/explanation/` (why). API reference
is generated from doc comments with
docs codegen: `docs/gen-*.py` renders cli/config/crate-map from source —
keep `lib.rs` module docs accurate; they feed the generated docs.
