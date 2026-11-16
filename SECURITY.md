# Security Policy

Report vulnerabilities privately by emailing
luisfuturist+9aia@gmail.com with "SECURITY" in the subject — do
not open public issues, do not use GitHub's advisory or
private-reporting features, and do not disclose details publicly;
contact us first and we will coordinate disclosure privately.

## Scope

- `mimus check`'s `cmd` checker verifies documented commands — by
  default it only probes that the invoked binary resolves on PATH
  and never executes them. Opt-in `[check].allow_exec` additionally
  runs each resolved command as argv (no shell, bounded by a
  timeout) and refutes on non-zero exit — only enable it on repos
  whose docs you trust (`--no-cmd` skips the checker entirely).
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
