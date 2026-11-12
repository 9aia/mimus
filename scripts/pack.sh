#!/usr/bin/env bash
# Stage the tamias pack payload (P-021): release-build
# `tamias-driver-mimus` and drop it under
# `libraries/mimus/drivers/bin/<host-triple>/` — the bundled-driver
# dir `tamias install` registers into `<tamias-home>/drivers/`.
# Gitignored on purpose: the binary is a build artifact; the
# committed pack is just `tamias.yaml` + this script.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p mimus-driver

triple="$(rustc -vV | sed -n 's/^host: //p')"
triple="${triple:-x86_64-unknown-linux-gnu}"
dest_dir="libraries/mimus/drivers/bin/$triple"
mkdir -p "$dest_dir"

# CARGO_TARGET_DIR-aware: `cargo build` honors it, so the artifact
# lookup must too (default: ./target).
target_dir="${CARGO_TARGET_DIR:-$PWD/target}"
cp "$target_dir/release/tamias-driver-mimus" "$dest_dir/tamias-driver-mimus"
chmod 755 "$dest_dir/tamias-driver-mimus"

echo "pack staged: $dest_dir/tamias-driver-mimus"
echo
echo "install the mimus command (generated bins[] shim):"
echo "  tamias install --allow-drivers ./libraries/mimus"
echo "  # or a pinned tarball: tamias bundle pack libraries/mimus"
