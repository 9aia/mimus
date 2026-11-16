---
url: /mimus/explanation/release-model.md
---
# The release model

Releases are **tag-only** — there is no crates.io publish and no
GitHub Releases artifact. The `mimus` crates are lib-only engines
behind `tamias-driver-mimus` (plus a compat `mimus` bin for source
installs); the user-facing command is the tamias pack in
`libraries/mimus`.

## The flow

`scripts/release.sh` is an operator-run shim over
[`tarsi-serve`](https://9aia.com/tarsi/) — the org release plane
(project ADR-0005). One call runs plan → changelog → bump → tag →
push; `tarsi-serve release` is dry-run by default and the script
passes `--yes` as the explicit opt-in.

Repo policy lives in `tarsi.toml`:

* `[publish.crates] enabled = false` — crates.io publishing is
  tamias-only (ADR-0004).
* `[publish.ghreleases] enabled = false` — org releases are
  tag-only; the capability stays for opt-in.
* `[release.sync_refs]` rewrites `libraries/mimus/tamias.yaml`'s
  `version:` inside the release commit, so the tagged tree is
  self-consistent.

Units default to cargo workspace members; versions are
workspace-inherited, so releasing units share one bump.

One repo-specific step survives inside the shim: because docs
drift with the CLI, the script regenerates `docs/reference/cli.md`
and `docs/reference/config.md` via `cypera gen` *before* tagging
(guarded — `docs/` is a separate worktree and absent in a
detached release worktree).

## Shipping the command

Tagging doesn't publish anything user-facing. The pack ships
manually:

```sh
scripts/pack.sh                                # stage driver payload
#   → release-builds tamias-driver-mimus into
#     libraries/mimus/drivers/bin/<triple>/ (gitignored)
tamias publish libraries/mimus                 # → 9aia:mimus@<ver>
```

Install is then `tamias install 9aia:mimus@<ver>`
(needs `TAMIAS_REGISTRY_TOKEN`) or
`tamias install --allow-drivers ./libraries/mimus`
from a checkout. `cargo install --git
https://github.com/9aia/mimus mimus` remains the source path
via the compat bin — pin a rev or tag.

## Constraints

* There is no crates.io publish path and no per-crate tag scheme —
  the whole workspace shares one version. The old push-driven
  `mimus deploy release` seed (which `cargo publish`ed every
  member and uploaded cargo-dist binaries to a GitHub Release) is
  retired: org policy keeps every repo but tamias off crates.io,
  and prebuilt binary distribution is not part of the flow.
