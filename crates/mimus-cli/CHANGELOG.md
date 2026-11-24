# Changelog

## [1.0.0] - 2026-11-24

### Added

- **spec:** cargo provider — org-local [patch] enforcement (`fc3cbba`)
- gen-cli-docs for docs pipeline (`fb0d3e3`)
- **spec:** kuda/sturn/myco/cypera providers (`f7f2f58`)
- mimus check --diff <spec> — scope checks to changed files (`ecdca5d`)
- secrets chain migrates to arca — mimus-arca adapter + composition (`d857282`)
- **check-cmd:** wire allow_exec — opt-in bounded execution tier (`d81f48f`)
- audit — trust-score kuda cards vs source (fresh/drifted/contradicted); dogfooded on 9aia/kuda (`c81b73e`)
- semantic tier intake from contradiction candidates (ADR-0002 ph5) (`bab6af0`)
- [check] knobs — kinds, ignore_prefixes, binaries, base_dirs (`ad34cf7`)
- mimus claims dump subcommand + sentence-aware prose extraction (paragraph join, sentence split, list-intro + vendored-file pruning) — 15 fragmented claims -> 45 whole-sentence assertions (`a9d78c9`)
- undocumented-fact class (ADR-0002 ph4) (`93b247d`)
- **spec:** StandardProvider port — namespaced [providers] claims dispatch (`85e51fd`)
- adversarial verify prompt + doctor diversity check (`7c09e01`)
- per-file triples cache (ADR-0002 ph3) (`7ae6d75`)
- engine-as-lib + tamias driver ops pattern (sturn parity) (`5dff55c`)
- **spec:** phase-2 predicate tier — path tables + git.untracked (`5ce70e1`)
- semantic-tier hardening — verdict cache (.mimus/verdicts.json), evidence-grounded refutes (unproven = silence), dead-path triage (free verdicts), fix --semantic (agent proposes corrected claim text), doctor ACP probe (`5c36377`)
- mimus-check-acp — semantic tier spawns ACP agent to verify card-prose/behavior claims; advisory warn-only; --all or [check].semantic + acp_cmd (`4b06450`)
- **standard:** coverage-only keys + union unlisted across matched specs (`3e6f65e`)
- mimus coverage (index symbols absent from docs corpus — reverse direction), code-comment claims wired (comments=true), doc↔doc contradiction pass in AcpChecker batches (subject clustering, ≤8 prompts) (`3be577f`)
- mimus fix — mechanical rewrites for dead path claims (unique moved-file match), rest flagged for human; eslint --fix shape (`37421fd`)
- mimus-check-graph — index-edge stale-reference + unsupported-claim (ADR-0002 ph1) (`30ca02d`)
- **spec:** org-parameterized cargo provider + lasius provider (`29958f1`)
- fs checker — glob claims expand, ~/$/<> unproveable silence; doc-source pruning of node_modules/dist/.cache/target (1153->62 findings on kuda dogfood) (`191701f`)
- **standard:** mimus-spec + `mimus standard` — declarative repo conformance spike (P-001) (`1826ccf`)
- **spec:** repo provider — bupha.toml name/dir drift check (`18138f8`)
- .kuda/files/**/*.yaml file cards feed the semantic claim path (`138dad3`)
- contradiction buckets wired into check — ADR-0002 ph2 (`0d8b121`)

### Changed

- **vcs:** route git access through lasius-vcs (P-050) (`8a56403`)
- **config:** scarab-config(-file) discovery — mimus.{yaml,yml,toml} (P-017) (`42bb4d5`)
- **secret:** secrets chain on cuora-secret crates (P-043) (`402c05f`)
- **BREAKING** rename arca adapter → cuora (mimus-cuora) (`2b9b94a`)

### Fixed

- e2e — audit exits 0 without cards since c81b73e (`fffb8e4`)
- **check-acp:** tool-agent deadlock, comment path triage, structured contradiction evidence (`d4b9261`)
