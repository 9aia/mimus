---
layout: home

hero:
  name: mimus
  text: spots the cuckoo eggs in your repo
  tagline: Verification engine — proofs over what agents claim
  actions:
    - theme: brand
      text: Getting Started
      link: /getting-started
    - theme: alt
      text: Tutorial
      link: /tutorials/first-run

features:
  - icon: 🪺
    title: Evidence or silence
    details: every violation carries claim + proof + fix — a checker that can't cite both emits nothing
    link: /explanation/architecture
    linkText: architecture
  - icon: 🧪
    title: Docs-as-tests
    details: paths, commands, symbols, counts in your docs verified mechanically — mimus check in CI means docs can't silently rot
    link: /tutorials/first-run
    linkText: first check
  - icon: 🧠
    title: Memory audit
    details: trust-scores .kuda/ cards against source — kuda remembers, mimus doubts
    link: /how-to/kuda-audit
    linkText: auditing cards
  - icon: 📐
    title: Rules as data
    details: conventions live in rules/*.toml — "dispatch via roles, never kind ==" is a file, not a code path
    link: /how-to/rules-authoring
    linkText: authoring rules
  - icon: 🧩
    title: Ports, not frameworks
    details: checkers (fs | cmd | symbol | llm), claim sources, sinks — swap via config or a new adapter crate
    link: /explanation/architecture
    linkText: architecture
  - icon: 🤖
    title: Built for agents
    details: mid-plan "is this claim still true" becomes a tool call, not a gamble
---

mimus is the org's **verification engine** — it enumerates the
claims in your docs, comments, and kuda cards and proves each one
or stays silent. It owns verification and nothing else: memory is
kuda's, fixing is reconcile's (planned), distribution is tamias's.

## Which quadrant answers which goal

- **Learn it** → [Tutorial: first check](tutorials/first-run.md)
- **Do a task** → [How-to](getting-started.md) — CI gate, kuda
  audit, rules authoring, multi-repo
- **Understand why** → [Architecture](explanation/architecture.md),
  [claims & tiers](explanation/claims.md),
  [concepts](explanation/concepts.md),
  [decisions](explanation/decisions.md)
- **Look something up** → [CLI](reference/cli.md),
  [config](reference/config.md),
  [violations](reference/violations.md),
  [crate API](reference/crate-api.md)
