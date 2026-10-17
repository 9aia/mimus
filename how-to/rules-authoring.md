# How-to: author rules

Conventions live in `rules/*.toml` — data, not code. A new rule
is a new file; the engine vocabulary stays fixed. Rules run in
`mimus check` alongside the claim checkers.

## The format

```toml
name = "no-positional-dispatch"
message = "dispatch via lang.eval roles — never kind == shape checks"
severity = "error"          # error (gates check) | warn (report only)
files = "crates/plugin-engine-*/src/**/*.rs"

[match]
kind = "source-pattern"
pattern = 'kind\s*==\s*"(assign|block|paren|return)"'
```

| field      | meaning                                            |
| ---------- | -------------------------------------------------- |
| `name`     | identifier — shown in output + rules listing       |
| `message`  | the *evidence* attached to each finding            |
| `severity` | `error` fails `mimus check`; `warn` reports        |
| `files`    | glob under the repo root                           |
| `[match]`  | `kind = "source-pattern"` + a regex                |

## Semantics

Every line matching `pattern` in every file matching `files`
becomes a `Behavior` violation carrying `message` as evidence.
Lines are matched individually — craft patterns for what a single
line can express.

## The evidence bar applies to rules too

A rule that cannot cite a specific line emits nothing — there is
no "file smells wrong" rule kind. And an invalid regex is not a
finding either: it is silence plus a `mimus doctor` complaint.
If your rule keeps flagging real code, the rule is wrong —
narrow the `files` glob or the pattern.

## Examples

Ban a deprecated API in a bound region:

```toml
[match]
kind = "source-pattern"
pattern = '\bold_api::'
```

`files = "crates/**/src/**/*.rs"` — one file, one convention,
reviewable in PRs like code.

Require README badges? That is a *presence* check — not what
`source-pattern` does; absence-of-pattern rules are a planned
`kind`, deliberately out until the evidence story is right.

## Verify

```sh
mimus check            # runs rules + claim checkers
mimus report --format json | jq '.[] | select(.claim.kind=="behavior")'
```
