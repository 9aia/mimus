# Done

- T-024 port to engine-as-lib + tamias driver ops pattern (sturn parity) · created:2026-11-10
- T-018 crates.io release — scripts/release.sh publishes the crate graph in dep order; needs a dry-run pass + tags · status:cancelled
- T-001 mimus-core — Claim / Violation / Evidence / TrustScore; claim taxonomy (path | command | symbol | count | link | behavior) decides which checker verifies it (ADR-0001).
- T-002 mimus-check port + mimus-check-fs adapter (paths exist)
- T-003 mimus-check-cmd adapter (commands parse / exit clean)
- T-004 mimus-check-symbol adapter — resolve symbols via a kuda index (first real cross-tool dep; mimus-kuda glue crate)
- T-005 mimus-rules — data-driven convention rules ( rules/\*.toml , e.g. "no kind == after role dispatch exists")
- T-006 mimus-claims — extract claims from md/comments/cards: backtick paths & commands, numbers, symbol names, behavior prose (semantic tier, deferred to llm adapter)
- T-007 mimus check — mechanical tier, CI-gateable output
