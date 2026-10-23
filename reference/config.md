---
url: /mimus/reference/config.md
---
# Config — `mimus.toml`

## Top level

| field | type | required | notes |
|---|---|---|---|
| `sources` | `SourcesCfg` | optional | — |
| `check` | `CheckCfg` | optional | — |
| `rules` | `RulesCfg` | optional | — |
| `report` | `ReportCfg` | optional | — |

## `[sources]`

| field | type | required | notes |
|---|---|---|---|
| `docs` | `list` | optional | Doc glob patterns mined for claims. |
| `comments` | `bool` | optional | Extract claims from code comments too. |
| `cards` | `string` | optional | Glob for memory cards to audit (empty = skip). |

## `[check]`

| field | type | required | notes |
|---|---|---|---|
| `mechanical` | `bool` | optional | Mechanical tier — deterministic, CI-gateable. |
| `semantic` | `bool` | optional | Semantic tier — advisory only, can never gate. |
| `kuda_index` | `path` | optional | kuda index path for the symbol checker. |
| `acp_cmd` | `string` | optional | ACP agent command for the semantic checker (e.g. "cline --acp"). Semantic tier is advisory only. |
| `base_dirs` | `Vec<PathBuf>` | optional | Extra repo-relative roots path claims resolve against — for docs that reference paths relative to a component root (e.g. "infrastructure/postgres"). |
| `kinds` | `Vec<ClaimKind>` | optional | Only check claims of these kinds (empty = all). |
| `ignore_prefixes` | `list` | optional | Skip claims whose text starts with any of these prefixes. |
| `binaries` | `list` | optional | Tool prerequisites the repo declares — commands invoking these skip the PATH probe (docs may legitimately require tools the checker's machine lacks). |

## `[rules]`

| field | type | required | notes |
|---|---|---|---|
| `dir` | `path` | optional | Dir of \*.toml rule files. |
| `strict` | `bool` | optional | Rule violations also fail `check` (not just report). |

## `[report]`

| field | type | required | notes |
|---|---|---|---|
| `format` | `string` | optional | "human" | "json". |
| `min_confidence` | `string` | optional | Suppress low-evidence findings ("high" | "all"). |

## Constraints

* `queue = "all"` rejects `cancel_running = true` at load —
  killing a run would silently drop a promised commit.
* `runner.kind` and `trigger.kind`/`status.kind` resolve to impl
  crates; unknown kinds fail at `mimus doctor`/startup.
* `deny_unknown_fields`: typos in field names fail the load.
