# Backlog

## Checkers

- T-009 mimus-check-llm adapter — semantic tier: prose claims vs code behavior ("retries 3 times" — code says 5); human-in-loop report, never a gate
- T-010 mimus-check-link — dead URLs, moved repos, stale badges
- T-011 ADR-drift checker — decisions vs current code state
- T-022 presentation deck — pending: run via ~/.9aia-pres pipeline (pres kick mimus or supervisor); agent writes presentation/ on branch presentation/initial, slidev build verify, push; no PR (sturn flow) — the pushed branch is the deliverable; merge is a human/sturn call; status board: ~/.9aia-pres/STATUS.md · created:2026-11-02

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

- T-019 AcpChecker: batch claims per prompt (e.g. 5/session/prompt)
  - T-020 optional inter-request delay — 46 back-to-back prompts is
