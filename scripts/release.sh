#!/usr/bin/env bash
# Triggered by malurus watch on pushes to 9aia/malurus (script runner).
# The leading sleep is the debounce window: a newer push kills this
# run via cancel_running, so only the last push in a quiet window
# releases. Runs inside a detached worktree — commits/tags are pushed
# from here, not from the main checkout.
set -euo pipefail
# worktree-local secrets (gitignored); the service env file is canonical
[ -f ./.env ] && { set -a; source ./.env; set +a; }
sleep "${MALURUS_RELEASE_DEBOUNCE:-300}"

git fetch origin --tags -q
last="$(git describe --tags --abbrev=0 2>/dev/null || true)"
if [ -z "$last" ]; then
    echo "no tags yet — run 'malurus deploy release' to seed the flow"; exit 0
fi
changed="$(git diff --name-only "${last}..HEAD")"

# workspace crates in publish order (path-deps first)
order="malurus-core malurus-vcs malurus-config malurus-secret \
malurus-check malurus-claims malurus-rules \
malurus-git malurus-config-toml malurus-keyring malurus-dotenv \
malurus-check-fs malurus-check-cmd malurus-check-symbol \
malurus"

touched=""
for c in $order malurus-cli; do
    grep -q "^crates/$c/" <<<"$changed" && touched="$touched $c"
done
# the bin re-publishes whenever any crate changed
[ -n "$touched" ] && touched="$touched malurus"
if [ -z "$touched" ]; then echo "no crate changes since $last"; exit 0; fi
published=""; tags=""
for c in $order; do
    p="${c}"; cdir="$c"
    [ "$c" = malurus ] && cdir="malurus-cli"      # cli package dir
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
ver="$(grep -m1 '^version' crates/malurus-cli/Cargo.toml | cut -d'"' -f2)"
git push -q origin HEAD:main --tags

# binaries: local dist build (host arch) → GitHub Release.
# Other arches join when the remote-runner pair lands (TODO).
if command -v dist >/dev/null && command -v gh >/dev/null; then
    dist build --output-format=json > /tmp/dist-manifest.json 2>&1 || true
    gh release create "malurus-v${ver}" --generate-notes         --title "malurus ${ver}" 2>/dev/null || true
    for f in target/distrib/*; do
        [ -f "$f" ] && gh release upload "malurus-v${ver}" "$f" --clobber
    done
fi
echo "released:${published} (tags:${tags})"
