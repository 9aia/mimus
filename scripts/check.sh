#!/usr/bin/env bash
# Local gate — what HEAD must satisfy before a push lands. There is
# no hosted CI by design; this script is the compile floor.
# Wire it: `git config core.hooksPath .githooks` (pre-push), or as a
# mimus job alongside release.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo check --workspace --all-targets
