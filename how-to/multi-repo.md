# How-to: multi-repo checking

Docs in one repo make claims about sibling repos — moved crates,
`see ../<repo>`, cross-repo commands. mimus verifies those claims
where it can reach reality.

## Per-repo config

Each repo keeps its own `mimus.toml` and `rules/` — conventions
are repo-scoped (a rule that is right for `platform` is noise in
`lsp`). Run `mimus check` per repo, or script the sweep:

```sh
for r in ~/GitHub/handl-lang/*/; do
  [ -f "$r/mimus.toml" ] && (cd "$r" && mimus check)
done
```

## Cross-repo claims today

- **Paths** are resolved relative to the repo root — a
  ../sibling/crate claim in docs verifies if the sibling exists.
- **Commands** check PATH, which is host-global — a doc telling
  users to run `hnl` is verified wherever `hnl` is installed.
- **Symbols** need a kuda index per repo; the `symbol` checker
  reads `.kuda/index.sqlite` of the *current* root. Cross-repo
  symbol resolution rides on kuda workspace mode.

## Rules at scale

Keep org-wide conventions in one place and symlink or vendor
them per repo — a shared rules/org/ directory fetched in CI
works today; first-class rule inheritance is a backlog item.

## Where it goes next

`kuda sync --workspace` produces per-root indexes across the
sibling set; the `symbol` checker then resolves claims like
`` `hnl` lives in the cli repo`` against the merged stores —
the "which repo owns this" question becomes verifiable.
