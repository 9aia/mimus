# Changelog

## [1.1.0] - 2026-11-25

## [1.0.0] - 2026-11-24

### Added

- **check-cmd:** wire allow_exec — opt-in bounded execution tier (`d81f48f`)
- [check] knobs — kinds, ignore_prefixes, binaries, base_dirs (`ad34cf7`)
- undocumented-fact class (ADR-0002 ph4) (`93b247d`)
- mimus-check-acp — semantic tier spawns ACP agent to verify card-prose/behavior claims; advisory warn-only; --all or [check].semantic + acp_cmd (`4b06450`)
- mimus-check-graph — index-edge stale-reference + unsupported-claim (ADR-0002 ph1) (`30ca02d`)
- contradiction buckets wired into check — ADR-0002 ph2 (`0d8b121`)

### Changed

- **config:** scarab-config(-file) discovery — mimus.{yaml,yml,toml} (P-017) (`42bb4d5`)
