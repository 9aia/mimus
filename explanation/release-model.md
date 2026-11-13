---
url: /mimus/explanation/release-model.md
---
# The release model

mimus releases itself — there is no hosted CI and no release tool
in the loop. Releases are manual: `scripts/release.sh` is run by an
operator on `main`. The script's leading sleep is the debounce
window — a newer push kills a running release via
`cancel_running`, so only the last push in a quiet window releases.

The org release plane is **tarsi** (`tarsi-serve` ops — project
ADR-0005): `scripts/release.sh` is being converted to a thin shim
over it (PL-014 E1), and until then this script implements the
same tag-only model directly.

**No crates.io publish**: org policy keeps every repo but tamias off
crates.io, and the mimus crates are internal lib-only engines behind
`tamias-driver-mimus`. There is no tamias pack yet either — installs
stay `cargo install --git https://github.com/9aia/mimus mimus`,
pinning a rev or a tag.

## What a release run does

1. **Diff since the last tag** — release-worthy paths are the
   workspace crates (`crates/`), the example manifest compiled into
   the CLI (`examples/` is `include_str!`-ed), and the release
   machinery itself. Changes to anything else trigger nothing. No
   release-set changes, no release.
2. **Workspace version bump** — every crate inherits
   `version.workspace`, so `cargo set-version --workspace --bump
   patch` moves them all at once and rewrites the
   `workspace.dependencies` version reqs and `Cargo.lock`
   together.
3. **Docs regen** — `docs/` is a separate worktree (orphan `docs`
   branch); when present, the script regenerates
   `docs/reference/cli.md` and the config reference so they match
   the released CLI.
4. **Commit + tag + push** — one `release: bump mimus to <ver>`
   commit, tagged `mimus-v<ver>`, pushed to `main` with the tag.

With no tags at all the script exits early — the flow is seeded
by tagging once.

## Constraints

* There is no crates.io publish path and no per-crate tag scheme —
  the whole workspace shares one version and one `mimus-v<ver>`
  tag. The old push-driven `mimus deploy release` seed (which
  `cargo publish`ed every member and uploaded cargo-dist binaries
  to a GitHub Release) is retired: org policy keeps every repo but
  tamias off crates.io, and prebuilt binary distribution is not
  part of the flow.
* Install is `cargo install --git` until a tamias pack exists —
  a `libraries/mimus` pack (mirroring kuda's `bins[]` shim over
  `tamias-driver-mimus`) is the planned distribution path.
