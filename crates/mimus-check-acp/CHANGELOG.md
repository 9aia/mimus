# Changelog

## [1.1.0] - 2026-11-25

## [1.0.0] - 2026-11-24

### Added

- adversarial verify prompt + doctor diversity check (`7c09e01`)
- semantic-tier hardening — verdict cache (.mimus/verdicts.json), evidence-grounded refutes (unproven = silence), dead-path triage (free verdicts), fix --semantic (agent proposes corrected claim text), doctor ACP probe (`5c36377`)
- checker batching — check_batch on the port, claims grouped per checker; AcpChecker folds N claims into one prompt (no provider spam) (`568041c`)
- mimus-check-acp — semantic tier spawns ACP agent to verify card-prose/behavior claims; advisory warn-only; --all or [check].semantic + acp_cmd (`4b06450`)
- mimus coverage (index symbols absent from docs corpus — reverse direction), code-comment claims wired (comments=true), doc↔doc contradiction pass in AcpChecker batches (subject clustering, ≤8 prompts) (`3be577f`)
- **spec:** repo provider — bupha.toml name/dir drift check (`18138f8`)
- contradiction buckets wired into check — ADR-0002 ph2 (`0d8b121`)

### Changed

- **check-acp:** consume canonical sepia-acp blocking client (PL-029) (`2cc946e`)

### Fixed

- **check-acp:** tool-agent deadlock, comment path triage, structured contradiction evidence (`d4b9261`)
- self-cleaning verdict cache (stale claim keys dropped per batch), identifier-shape subject clustering (filters natural words) (`cf0bccd`)
- compile — HashMap::retain returns (), not Option (`2d978d0`)
