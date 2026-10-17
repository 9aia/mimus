# TODO — 0.1.0

## Core model
- [ ] `malurus-core` — Claim / Violation / Evidence / TrustScore;
      claim taxonomy (path | command | symbol | count | link |
      behavior) decides which checker verifies it (ADR-0001).

## Checkers (ports + first impls)
- [ ] `malurus-check` port + `malurus-check-fs` impl (paths exist)
- [ ] `malurus-check-cmd` impl (commands parse / exit clean)
- [ ] `malurus-check-symbol` impl — resolve symbols via a `kuda`
      index (first real cross-tool dep; `malurus-kuda` glue crate)
- [ ] `malurus-rules` — data-driven convention rules (`rules/*.toml`,
      e.g. "no `kind ==` after role dispatch exists")

## Claims + reporting
- [ ] `malurus-claims` — extract claims from md/comments/cards:
      backtick paths & commands, numbers, symbol names, behavior
      prose (semantic tier, deferred to llm impl)
- [ ] `malurus check` — mechanical tier, CI-gateable output
- [ ] `malurus audit` — trust-score `.kuda/` cards vs source
- [ ] `malurus report` — violations with claim+evidence+fix

## Scaffold debt (forked from bupha — rewrite crate-by-crate)
- [x] Topology landed: CI crates dropped (recoverable from history),
      check/claims/rules crates stubbed, `malurus-core` rewritten
      to the claim/violation domain, `malurus` CLI works — `init`
      and `check` (fs+cmd checkers + rules) are functional.
