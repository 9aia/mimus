# Reference: CLI

Generated from `mimus --help` by `bun run docs:cli` — seed listing:

```
mimus init            detect sources -> mimus.toml + rules/
mimus check           mechanical claims — CI-gateable
mimus check --all     + semantic tier (advisory only)
mimus audit           trust-score .kuda/ cards vs source
mimus report          full violations: claim + evidence + fix
mimus doctor          checker/kuda-index health
```

Flags: `--config <path>` (else `$MIMUS_CONFIG`, `./mimus.toml`,
`~/.config/mimus/mimus.toml`), `--format human|json`,
`--no-cmd` (never execute documented commands — untrusted repos).
