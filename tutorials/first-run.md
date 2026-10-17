# Tutorial: your first check

Goal: verify a repo's documented claims in minutes.

## 1. Init

```sh
cd your-repo
malurus init
```

Detects claim sources (README, docs/, AGENTS.md, code comments,
`.kuda/` cards) and writes `malurus.toml` + a `rules/` dir.

## 2. Check the mechanical tier

```sh
malurus check
```

Extracts claims and verifies what can be proven:

- `path` — referenced files/dirs exist
- `command` — documented commands parse and exit clean
- `symbol` — names resolve (via the kuda index when present)
- `count` — asserted numbers recompute ("357 suites…")
- `link` — URLs resolve

Output is CI-gateable: violations only, each with claim +
evidence + fix.

## 3. Report

```sh
malurus report          # everything found, grouped by source
malurus check --all     # + semantic tier (advisory, never gates)
```

## 4. Wire into CI

```sh
malurus check --format json   # machine-readable for gates
```

Docs become tested artifacts — a claim that breaks fails the
build like a failing test.
