# TODO — 0.1.0

## Core model

- [ ] T-022 presentation deck — pending: run via ~/.9aia-pres pipeline (pres kick mimus or supervisor); agent writes presentation/ on branch presentation/initial, slidev build verify, push; no PR (sturn flow) — the pushed branch is the deliverable; merge is a human/sturn call; status board: ~/.9aia-pres/STATUS.md · created:2026-11-02
- [ ] T-023 agent-awareness managed block — when/why agents run mimus audit (independent card verification vs kuda status) as a \<!-- mimus:agent-awareness --> block — pattern: myco init/check · created:2026-11-02

## Checkers (ports + first adapters)

- [ ] T-003 mimus-check-cmd adapter (commands parse / exit clean)
- [ ] T-004 mimus-check-symbol adapter — resolve symbols via a kuda index (first real cross-tool dep; mimus-kuda glue crate)
- [ ] T-005 mimus-rules — data-driven convention rules ( rules/\*.toml , e.g. "no kind == after role dispatch exists")

## Claims + reporting

- [ ] T-006 mimus-claims — extract claims from md/comments/cards: backtick paths & commands, numbers, symbol names, behavior prose (semantic tier, deferred to llm adapter)
- [ ] T-007 mimus check — mechanical tier, CI-gateable output
- [ ] T-008 mimus report — violations with claim+evidence+fix

## Scaffold debt (forked from bupha — rewrite crate-by-crate)
