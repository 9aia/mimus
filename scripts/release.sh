#!/usr/bin/env bash
# Triggered by mimus watch on pushes to 9aia/mimus (script runner).
# The leading sleep is the debounce window: a newer push kills this
# run via cancel_running, so only the last push in a quiet window
# releases. Runs inside a detached worktree — commits/tags are pushed
# from here, not from the main checkout.
set -euo pipefail
# worktree-local secrets (gitignored); the service env file is canonical
[ -f ./.env ] && { set -a; source ./.env; set +a; }
sleep "${MIMUS_RELEASE_DEBOUNCE:-300}"

git fetch origin --tags -q
last="$(git describe --tags --abbrev=0 2>/dev/null || true)"
if [ -z "$last" ]; then
    echo "no tags yet — run 'mimus deploy release' to seed the flow"; exit 0
fi
changed="$(git diff --name-only "${last}..HEAD")"

# workspace crates in publish order (path-deps first)
order="mimus-core mimus-vcs mimus-config mimus-secret \
mimus-check mimus-claims mimus-rules \
mimus-git mimus-config-toml mimus-keyring mimus-dotenv \
mimus-check-fs mimus-check-cmd mimus-check-symbol \
mimus"

touched=""
for c in $order mimus-cli; do
    grep -q "^crates/$c/" <<<"$changed" && touched="$touched $c"
done
# the bin re-publishes whenever any crate changed
[ -n "$touched" ] && touched="$touched mimus"
if [ -z "$touched" ]; then echo "no crate changes since $last"; exit 0; fi
published=""; tags=""
for c in $order; do
    p="${c}"; cdir="$c"
    [ "$c" = mimus ] && cdir="mimus-cli"      # cli package dir
    grep -q " ${c} \|^${c} \?$\| ${c}$" <<<" $touched " || continue
    cargo set-version -p "$p" --bump patch
    for i in 1 2 3 4 5; do
        cargo publish -p "$p" --allow-dirty && break || sleep 30
    done
    published="$published $p"
    # Per-crate tag — versions diverge independently in a monorepo.
    ver_c="$(grep -m1 '^version' "crates/$cdir/Cargo.toml" | cut -d'"' -f2)"
    tags="$tags $p-v$ver_c"
done

git commit -aqm "release: bump crate versions"
for t in $tags; do git tag "$t"; done
ver="$(grep -m1 '^version' crates/mimus-cli/Cargo.toml | cut -d'"' -f2)"
git push -q origin HEAD:main --tags

# binaries: local dist build (host arch) → GitHub Release.
# Other arches join when the remote-runner pair lands (TODO).
if command -v dist >/dev/null && command -v gh >/dev/null; then
    dist build --output-format=json > /tmp/dist-manifest.json 2>&1 || true
    gh release create "mimus-v${ver}" --generate-notes         --title "mimus ${ver}" 2>/dev/null || true
    for f in target/distrib/*; do
        [ -f "$f" ] && gh release upload "mimus-v${ver}" "$f" --clobber
    done
fi
echo "released:${published} (tags:${tags})"
