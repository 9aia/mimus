# Reference: CLI

Generated from `malurus --help` by `bun run docs:cli` — seed listing:

```
malurus init            detect sources -> malurus.toml + rules/
malurus check           mechanical claims — CI-gateable
malurus check --all     + semantic tier (advisory only)
malurus audit           trust-score .kuda/ cards vs source
malurus report          full violations: claim + evidence + fix
malurus doctor          checker/kuda-index health
```

Flags: `--config <path>` (else `$MALURUS_CONFIG`, `./malurus.toml`,
`~/.config/malurus/malurus.toml`), `--format human|json`,
`--no-cmd` (never execute documented commands — untrusted repos).
