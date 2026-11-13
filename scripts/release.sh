#!/usr/bin/env bash
# mimus release — thin shim over tarsi-serve (the org release plane,
# project ADR-0005; repo config: ./tarsi.toml). One call runs
# plan → changelog → bump → tag → push; publish targets are off
# (ADR-0004: crates.io is tamias-only, GitHub Releases unused).
# Operator-run on main — `tarsi-serve release` is dry-run by
# default; --yes is the explicit opt-in.
#
# Preserved repo steps: docs drift with the CLI, so generated
# references are regenerated *before* tagging (guarded — `docs/` is
# a separate worktree and doesn't exist in a detached release
# worktree), and the tamias pack manifest version is synced to the
# new version ahead of the release commit. The pack publish itself
# stays manual: `scripts/pack.sh` stages the driver payload, then
# `tamias install --allow-drivers ./libraries/mimus`.
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

ws_version() { grep -m1 '^version = "' Cargo.toml | cut -d'"' -f2; }

# Learn the version tarsi will bump to (all releasing units share one
# workspace version — empty output means "don't sync").
new="$(tarsi-serve plan --dir . | python3 -c '
import json,sys
d=json.load(sys.stdin)
if not d.get("ok"): sys.exit(0)
units=[u for u in d["result"]["plan"]["units"] if u.get("releases")]
vs={u["new_version"] for u in units}
print(vs.pop() if len(vs)==1 else "")
')"
old="$(ws_version)"
if [ -n "$new" ] && [ "$new" != "$old" ]; then
    sed -i "s/^version: \"${old}\"/version: \"${new}\"/" libraries/mimus/tamias.yaml
    git add libraries/mimus/tamias.yaml
    git commit -qm "chore(release): sync mimus@${new} version ref ahead of tarsi bump"
fi

tarsi-serve release --dir . --yes

echo "released: mimus ${new:-$old} (tag mimus-v${new:-$old}, per-unit tags for the rest)"
echo "next: scripts/pack.sh && tamias install --allow-drivers ./libraries/mimus  # ship mimus@${new:-$old} (git-rev install: cargo install --git https://github.com/9aia/mimus mimus)"
