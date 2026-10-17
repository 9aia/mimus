# How-to: author convention rules

Conventions live as data in `rules/*.toml` — never in engine
code. A new rule is a file, not a release.

## A rule

```toml
# rules/no-kind-dispatch.toml
name = "no-kind-dispatch"
message = "dispatch via lang.child_role_index — kind == is a regression"
severity = "error"
match = { kind = "source-pattern", pattern = 'kind == "(assign|block|paren)"' }
files = "crates/**/src/*.rs"
```

Rule kinds:

- `source-pattern` — regex over source files
- `doc-claim` — a prose claim that must verify (e.g. "README's
  build command exits clean")
- `structure` — graph assertions ("engine crates never depend
  on cli")

## Semantics

- `severity = "error"` fails `malurus check`; `"warn"` only
  appears in `report`.
- Keep rules mechanically checkable — if a rule can't carry
  evidence, it doesn't belong here (see ADR-0001).
- Rules are per-repo — convention packs for ecosystems are a
  backlog item.
