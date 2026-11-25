# Changelog

## [1.1.0] - 2026-11-25

## [1.0.0] - 2026-11-24

### Added

- checker batching — check_batch on the port, claims grouped per checker; AcpChecker folds N claims into one prompt (no provider spam) (`568041c`)
- mimus-check-graph — index-edge stale-reference + unsupported-claim (ADR-0002 ph1) (`30ca02d`)
- contradiction buckets wired into check — ADR-0002 ph2 (`0d8b121`)
