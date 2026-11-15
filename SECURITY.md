# Security Policy

Report vulnerabilities privately to the maintainers via GitHub:
https://github.com/9aia/mimus/security/advisories/new

Do not open public issues for security reports.

## Scope

- `mimus check`'s `cmd` checker verifies documented commands — it
  only probes that the invoked binary resolves on PATH and never
  executes them (`--no-cmd` skips the checker entirely).
- `mimus` reads source trees and doc files; it does not write to
  the repo except `mimus.toml`/`rules/` on `init`.
- Secrets resolve via env var → `cuora` vaults → OS keyring
  (`secret-tool`) → `~/.config/mimus/env` (chmod 600) — the
  `mimus-secret` chain; values never appear in `mimus.toml` or
  reports.
- The semantic tier (`--semantic`, needs `[check].acp_cmd`) spawns
  an ACP agent to verify prose `behavior` claims — it is opt-in,
  advisory only, and never gates `check`; point `acp_cmd` at an
  agent you trust with the repo's claim text.

## Notes

- Reports contain file paths, claim text, and evidence snippets —
  they inherit the repo's confidentiality.
