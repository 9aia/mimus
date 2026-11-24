<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->

<!-- sturn:agent-awareness -->

## Sturn awareness

- **No PRs.** The pushed branch/commit is the deliverable; review
  streams continuously (sturn WAL), merge is a human/sturn call.
- sturn-wired repo (`sturn.toml` present): `sturn claim "<glob>"
  --ws <ws>` *before* edits — claims surface conflicts in ms.
- Ops via `sturn write|rename|delete`; `sturn snap` folds the ws
  into a real commit; `sturn push` publishes heads + refs/sturn/*.
- `sturn leave` on exit — drops the actor's claims.

<!-- /sturn:agent-awareness -->

<!-- kuda:agent-awareness -->

## Kuda awareness

- `.kuda/` cards are committed artifacts — read them before
  re-exploring (`kuda map`, `kuda card <mod>`); `kuda status`
  reports per-card drift vs HEAD.
- Code changes that invalidate a card → regenerate it (`kuda
  sync`) in the same change; cards ride the diff they describe.
- `.kuda/index.sqlite` is gitignored + rebuildable; the `*.yaml`
  cards are the checked-in source of truth.

<!-- /kuda:agent-awareness -->

# Mimus

Verification engine — proofs over what agents claim. Named for
*Mimus polyglottos*, the mockingbird that spots impostor eggs in
its nest: mimus extracts claims from docs, comments, and `.kuda/`
cards; verifies them against the code — mechanically where it can,
flagged-with-evidence where it can't — and repairs the
mechanically-fixable ones. **kuda remembers; mimus doubts** —
`mimus audit` is the falsifier for the memory layer.

## Layout

Flat `crates/` workspace, ports-and-adapters. `mimus-core` is pure
domain (claims, violations, evidence); ports are traits, impls are
separate crates; `mimus-cli` (package `mimus`) is the engine lib
plus a compat `[[bin]]`; `mimus-driver` ships `tamias-driver-mimus`
(the `libraries/mimus` pack's `bins[]` shim target + pavo views +
ops surface).

| Crate | Role |
| --- | --- |
| `mimus-core` | claim/violation/evidence model — pure, no I/O |
| `mimus-cli` (pkg `mimus`) | engine lib — clap, `--json`, `manifest`/`schema`, `run_from()`; compat `mimus` bin |
| `mimus-driver` | `tamias-driver-mimus` — bins-shim `cli` passthrough + ops surface + views; e2e in `tests/` |
| `mimus-check` | the `Checker` port + `GraphChecker` set-level classes (ADR-0002 — contradictions, stale references at bucket speed) |
| `mimus-check-fs` / `-cmd` / `-symbol` / `-graph` | mechanical checkers — paths, commands (`[check].allow_exec` opt-in), `.kuda` symbols, claim-graph triples |
| `mimus-check-acp` | semantic tier — spawns an ACP agent; advisory only, can never gate |
| `mimus-claims` | claim extraction from markdown/comments/cards |
| `mimus-rules` | `rules/*.toml` conventions as data |
| `mimus-config` | `mimus.toml` loading via scarab-config |
| `mimus-spec` | `mimus standard` — declarative repo-conformance specs (P-001): file-set diff + path/git predicates + `providers.<ns>` dispatch (`cargo`, `repo`, `lasius` built in) |
| `cuora-secret` + `-cuora`/`-keyring`/`-dotenv` (git deps) | secrets chain — env → cuora vaults → keyring → dotenv file |
| `rules/` | the repo's own convention rules (`no-todos`, `no-unwrap-in-checkers`) |
| `libraries/mimus` | the tamias pack — `bins[]` shim + driver payload |
| `modules/dagger/` | shared pipeline module (Python SDK) |
| `scripts/` | `check.sh` (pre-push floor), `coverage.sh`, `pack.sh` |

## Commands

No xtask — plain cargo + scripts:

```bash
cargo build --workspace && cargo test --workspace   # unit + e2e
                                               # (e2e in crates/mimus-cli/tests)
scripts/check.sh             # cargo check --all-targets — pre-push floor
                             # (git config core.hooksPath .githooks)
scripts/coverage.sh          # llvm-cov HTML → target/llvm-cov/html;
                             # extra args pass through (--fail-under-lines 100)
scripts/pack.sh              # release tamias-driver-mimus →
                             # libraries/mimus/drivers/bin/<triple>/
tarsi-serve release --dir . --yes
                             # operator-run: version bump + tag
                             # (dry-run without --yes); regen docs
                             # refs first — see docs-check.sh
```

Deploy the command: `scripts/pack.sh` then `tamias install
--allow-drivers ./libraries/mimus` — or `cargo install --git
https://github.com/9aia/mimus mimus` for a source build (the
compat bin; not on crates.io — org policy).

## Boundary

mimus owns **verification** — claim extraction, checkers, rules,
`fix`, `audit`. Neighbors:

- **kuda** — code memory; mimus audits its cards (`mimus audit`)
  and reads its index for the `symbol` checker.
- **cuora** — secrets; the `cuora-secret*` crates resolve `cuora://` refs.
- **myco** — this repo's `project/` ledger.
- **tamias** — distribution; mimus ships as a pack + `mimus-rules`
  content type.
- **sturn** — ephemeral coordination.

Style opinions are never emitted — evidence or silence.

## Docs

`docs/` worktree — VitePress + Diátaxis (tutorials/how-to/reference/
explanation) → GitHub Pages at `9aia.com/mimus`. `cypera gen` (driven by `cypera.toml`)
renders cli/config/crate-map from source — keep `lib.rs` module
docs accurate.

## Specs

`specs/<name>.toml` is this repo's mimus-standard conformance spec —
approval-gated. Any change requires a live org proposal
(`proposal-NNNN`, status open|done) referenced in the spec header or
commit range, and no coverage regression vs merge-base. Guard:
workspace `../../scripts/spec-guard.py`; sweep:
`../../scripts/standard.sh`; regenerate coverage only via
`../../scripts/gen-repo-specs.py`.
