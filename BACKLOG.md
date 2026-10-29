# Backlog

## Checkers

- T-009 mimus-check-llm adapter — semantic tier: prose claims vs code behavior ("retries 3 times" — code says 5); human-in-loop report, never a gate
- T-010 mimus-check-link — dead URLs, moved repos, stale badges
- T-011 ADR-drift checker — decisions vs current code state

## Rules

- T-012 Rule packs per ecosystem (rust/go/ts conventions as data)
- T-013 Import rules from existing linters' config (read-only reuse)

## Surfaces

- T-014 mimus-mcp — agents ask "is this claim still true" mid-plan
- T-015 mimus watch — re-verify claims on file events
- T-016 PR/commit-status sink — post violations as review comments (reuse the forked status/github crates?)

## Ecosystem

- T-017 Dogfood: run against the handl-lang AGENTS.md corpus — ~70 repos of accumulated prose claims

## Release

- T-018 crates.io release — scripts/release.sh publishes the crate graph in dep order; needs a dry-run pass + tags
- T-019 AcpChecker: batch claims per prompt (e.g. 5/session/prompt) + optional inter-request delay — 46 back-to-back prompts is rude to the provider and hits rate limits
