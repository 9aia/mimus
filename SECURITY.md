# Security Policy

Report vulnerabilities privately to the maintainers via GitHub:
https://github.com/9aia/mimus/security/advisories/new

Do not open public issues for security reports.

## Scope

- `mimus check` executes shell lines it finds in docs — it
  verifies *your own repo's* documented commands; never point it at
  untrusted sources without `--no-cmd`.
- `mimus` reads source trees and doc files; it does not write to
  the repo except `mimus.toml`/`rules/` on `init`.
- Secrets (`MIMUS_LLM_KEY` and future provider keys) resolve via
  env var → OS keyring (`secret-tool`) → `~/.config/mimus/env`
  (chmod 600); values never appear in `mimus.toml` or reports.
- The `llm` checker sends claim+context fragments to a third-party
  provider — it is opt-in (`semantic = true`); the mechanical tier
  is fully local.

## Notes

- Reports contain file paths, claim text, and evidence snippets —
  they inherit the repo's confidentiality.
