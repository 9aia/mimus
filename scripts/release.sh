#!/usr/bin/env bash
# mimus release — thin shim over tarsi-serve (the org release plane,
# project ADR-0005; repo config: ./tarsi.toml). One call runs
# plan → changelog → bump → tag → push; publish targets are off
# (ADR-0004: crates.io is tamias-only, GitHub Releases unused).
# Operator-run on main — `tarsi-serve release` is dry-run by
# default; --yes is the explicit opt-in.
#
# Preserved repo step: docs drift with the CLI, so generated
# references are regenerated *before* tagging (guarded — `docs/` is
# a separate worktree and doesn't exist in a detached release
# worktree). Pack-ref sync (`[release.sync_refs]` in tarsi.toml)
# rewrites the tamias pack manifest version inside the release
# commit. The pack publish itself stays manual: `scripts/pack.sh`
# stages the driver payload, then `tamias install --allow-drivers
# ./libraries/mimus`.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
# worktree-local env (gitignored) — kept for parity with the other
# org release scripts; nothing here needs secrets today.
[ -f ./.env ] && { set -a; source ./.env; set +a; }

# docs drift with the CLI — regen generated references before
# tagging so cli.md/config.md always match the released source.
if [ -d docs/reference ]; then
    cargo run -qp mimus -- gen-cli-docs > docs/reference/cli.md
    python3 docs/gen-config-docs.py
fi

tarsi-serve release --dir . --yes

echo "released: see tags mimus-*-v<ver>"
echo "next: scripts/pack.sh && tamias install --allow-drivers ./libraries/mimus  # ship mimus@<ver> (git-rev install: cargo install --git https://github.com/9aia/mimus mimus)"
