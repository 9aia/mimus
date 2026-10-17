# TODO — 0.1.0

## Core model
- [ ] `mimus-core` — Claim / Violation / Evidence / TrustScore;
      claim taxonomy (path | command | symbol | count | link |
      behavior) decides which checker verifies it (ADR-0001).

## Checkers (ports + first impls)
- [ ] `mimus-check` port + `mimus-check-fs` impl (paths exist)
- [ ] `mimus-check-cmd` impl (commands parse / exit clean)
- [ ] `mimus-check-symbol` impl — resolve symbols via a `kuda`
      index (first real cross-tool dep; `mimus-kuda` glue crate)
- [ ] `mimus-rules` — data-driven convention rules (`rules/*.toml`,
      e.g. "no `kind ==` after role dispatch exists")

## Claims + reporting
- [ ] `mimus-claims` — extract claims from md/comments/cards:
      backtick paths & commands, numbers, symbol names, behavior
      prose (semantic tier, deferred to llm impl)
- [ ] `mimus check` — mechanical tier, CI-gateable output
- [ ] `mimus audit` — trust-score `.kuda/` cards vs source
- [ ] `mimus report` — violations with claim+evidence+fix

## Scaffold debt (forked from bupha — rewrite crate-by-crate)
- [x] Topology landed: CI crates dropped (recoverable from history),
      check/claims/rules crates stubbed, `mimus-core` rewritten
      to the claim/violation domain, `mimus` CLI works — `init`
      and `check` (fs+cmd checkers + rules) are functional.
