# mimus vs the alternatives

## vs linters

|                     | linters              | mimus                       |
| ------------------- | -------------------- | ----------------------------- |
| checks              | code shape           | claims about reality          |
| docs                | mostly ignored       | first-class source            |
| output              | style/correctness    | claim + evidence + fix        |
| rules               | per-language config  | `rules/*.toml`, evidence-bound|

They complement each other — a linter says the code is shaped
right; mimus says the docs tell the truth.

## vs LLM doc review

|                     | "review the docs" prompt | mimus                    |
| ------------------- | ------------------------ | -------------------------- |
| coverage            | broad, vague             | only what's checkable      |
| false positives     | high                     | evidence-or-silence        |
| CI gate             | unsafe                   | mechanical tier is safe    |
| cost                | per-run                  | mechanical tier is free    |

The LLM still has a role — as the `check-llm` impl for the
semantic tier, where findings are advisory by design.

## vs doctest-style tools

|                     | doctests/link-checkers | mimus                       |
| ------------------- | ---------------------- | ----------------------------- |
| scope               | code blocks only       | all factual prose             |
| claim types         | one or two             | path/cmd/symbol/count/link    |
| memory audit        | n/a                    | `.kuda/` card trust scores    |
