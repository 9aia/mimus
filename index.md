---
layout: home

hero:
  name: malurus
  text: spots the cuckoo eggs in your repo
  tagline: claim verification — docs-as-tests, memory audit
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
    details: paths, commands, symbols, counts in your docs verified mechanically — malurus check in CI means docs can't silently rot
    link: /tutorials/first-run
    linkText: first check
  - icon: 🧠
    title: Memory audit
    details: trust-scores .kuda/ cards against source — kuda remembers, malurus doubts
    link: /how-to/kuda-audit
    linkText: auditing cards
  - icon: 📐
    title: Rules as data
    details: conventions live in rules/*.toml — "dispatch via roles, never kind ==" is a file, not a code path
    link: /how-to/rules-authoring
    linkText: authoring rules
  - icon: 🧩
    title: Ports, not frameworks
    details: checkers (fs | cmd | symbol | llm), claim sources, sinks — swap via config or a new impl crate
    link: /explanation/architecture
    linkText: architecture
  - icon: 🤖
    title: Built for agents
    details: mid-plan "is this claim still true" becomes a tool call, not a gamble
---
