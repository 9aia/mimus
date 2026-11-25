# Changelog

## [1.1.0] - 2026-11-25

## [1.0.0] - 2026-11-24

### Added

- claim→triple extraction + contradiction bucket engine (ADR-0002 ph2 engine) (`ff5581a`)
- doc-claim extraction — prose assertions become Behavior claims for the acp checker (cap 10/file, fenced code skipped); docs refresh (`dd9164b`)
- mimus claims dump subcommand + sentence-aware prose extraction (paragraph join, sentence split, list-intro + vendored-file pruning) — 15 fragmented claims -> 45 whole-sentence assertions (`a9d78c9`)
- undocumented-fact class (ADR-0002 ph4) (`93b247d`)
- mimus coverage (index symbols absent from docs corpus — reverse direction), code-comment claims wired (comments=true), doc↔doc contradiction pass in AcpChecker batches (subject clustering, ≤8 prompts) (`3be577f`)
- extractor precision + inline suppression (`0c0e1dd`)

### Fixed

- **claims:** import specifiers are not path claims (`e69b188`)
- Command claims via binary allowlist — code keywords no longer extracted (2047->1530 findings, 0 noise) (`b8d8ceb`)
