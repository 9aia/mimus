# BACKLOG — horizon

Ideas parked, not scheduled. Pull into project/TODO.md when picked up.

## Checkers
- [ ] `mimus-check-llm` adapter — semantic tier: prose claims vs code
      behavior ("retries 3 times" — code says 5); human-in-loop
      report, never a gate
- [ ] `mimus-check-link` — dead URLs, moved repos, stale badges
- [ ] ADR-drift checker — decisions vs current code state

## Rules
- [ ] Rule packs per ecosystem (rust/go/ts conventions as data)
- [ ] Import rules from existing linters' config (read-only reuse)

## Surfaces
- [ ] `mimus-mcp` — agents ask "is this claim still true" mid-plan
- [ ] `mimus watch` — re-verify claims on file events
- [ ] PR/commit-status sink — post violations as review comments
      (reuse the forked status/github crates?)

## Ecosystem
- [x] `mimus fix` — auto-fix confirmed violations (landed;
      dead-path rewrites today). Widen: rule fixes, claim deletion
      for deleted targets, `kuda sync` for drifted cards
- [ ] Dogfood: run against the handl-lang AGENTS.md corpus —
      ~70 repos of accumulated prose claims

## Release
- [ ] crates.io release — `scripts/release.sh` publishes the
      crate graph in dep order; needs a dry-run pass + tags
