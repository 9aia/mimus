#!/usr/bin/env bash
# Run manually: `./scripts/release.sh` from the repo root.
# The leading sleep is the debounce window: a newer push kills this
# run via cancel_running, so only the last push in a quiet window
# releases. Runs inside a detached worktree — commits/tags are pushed
# from here, not from the main checkout.
#
# crates.io publish is retired: org policy keeps every repo but tamias
# off crates.io, and the mimus crates are internal lib-only engines
# behind `tamias-driver-mimus` — there is no tamias pack yet and no
# cargo-installed `mimus` binary on a registry; installs pin a git
# rev (`cargo install --git https://github.com/9aia/mimus mimus`,
# see README). A release is therefore one workspace version bump + a
# `mimus-v<ver>` tag. Binary dist (cargo-dist + gh release upload)
# is out of scope.
# Needs: `cargo-edit` (cargo set-version).
set -euo pipefail
# worktree-local env (gitignored) — kept for parity with the other
# org release scripts; nothing here needs secrets today.
[ -f ./.env ] && { set -a; source ./.env; set +a; }
sleep "${MIMUS_RELEASE_DEBOUNCE:-300}"

git fetch origin --tags -q
last="$(git describe --tags --abbrev=0 2>/dev/null || true)"
if [ -z "$last" ]; then
    echo "no tags yet — tag the first release manually to seed the flow"; exit 0
fi
changed="$(git diff --name-only "${last}..HEAD")"

# release-worthy paths: workspace crates, the example manifest
# compiled into the CLI (`examples/` is include_str!-ed), and the
# release machinery itself. Docs live on the orphan `docs` worktree
# branch — they never appear in this diff.
relevant="$(grep -E '^(crates/|examples/|Cargo\.(toml|lock)|scripts/release\.sh)' <<<"$changed" || true)"
if [ -z "$relevant" ]; then echo "no release-relevant changes since $last"; exit 0; fi

# every crate inherits `version.workspace` — one bump moves them all,
# and `cargo set-version --workspace` rewrites the workspace.package
# version, the workspace.dependencies reqs, and Cargo.lock together.
old="$(grep -m1 '^version = "' Cargo.toml | cut -d'"' -f2)"
cargo set-version --workspace --bump patch
ver="$(grep -m1 '^version = "' Cargo.toml | cut -d'"' -f2)"

# docs drift with the CLI — regen generated references before
# tagging so cli.md/config.md always match the released source.
# `docs/` is a separate worktree (orphan `docs` branch): in a
# detached release worktree it doesn't exist, so guard the writes.
if [ -d docs/reference ]; then
    cargo run -qp mimus -- gen-cli-docs > docs/reference/cli.md
    python3 docs/gen-config-docs.py
fi

git commit -aqm "release: bump mimus to ${ver} — tag release"
git tag "mimus-v${ver}"
git push -q origin HEAD:main --tags

echo "released: mimus ${ver} (tag mimus-v${ver})"
echo "next: no tamias pack yet — install stays 'cargo install --git https://github.com/9aia/mimus mimus'"
