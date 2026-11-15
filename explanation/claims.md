# Explanation: claims, tiers, and the evidence bar

mimus is built on one observation: documentation is full of
*claims* — statements that are either still true or quietly
false. A critic's job is to enumerate them and prove each one,
or stay silent.

## Claim taxonomy

Extraction classifies backticked spans, code blocks and prose
patterns into:

| kind      | example claim                        | verified by                |
| --------- | ------------------------------------ | -------------------------- |
| `path`    | `` `src/lib.rs` ``                   | filesystem                 |
| `command` | `` `cargo test -p foo` ``            | binary on PATH (no exec)   |
| `symbol`  | `` `IndexStore` ``                   | kuda index (symbol checker)|
| `count`   | "357 suites / 4,298 tests"           | recomputation              |
| `link`    | `[docs](https://…)`                  | link checker               |
| `behavior`| prose asserting code does X          | rules + semantic tier      |

Command claims are conservative by design: only spans whose
first token is a known binary (`cargo`, `git`, `npm`, `dagger`,
`kuda`, `mimus`, …) classify as commands — otherwise `pub fn`
and `data Eq` get read as shell.

## Verification tiers

- **Mechanical** — deterministic and offline: paths, commands,
  rules. This is the `mimus check` gate; if it flags something,
  the evidence is reproducible by anyone.
- **Graph** (ADR-0002 — all six classes live) — set-level pass,
  not per-claim: claims and kuda-index edges become
  `(subject, relation, object)` triples bucketed by subject. Same
  bucket, different object → `contradiction` (doc↔doc) or
  `self-contradiction` (reflexive), warn-tier — index-free, runs
  on every claim set; index edge with no target →
  `stale-reference` (error tier); prose edge the index lacks →
  `unsupported claim`; index module no document names →
  `undocumented fact` (warn, see below). `[check].aliases` folds
  differently-spelled subjects into one bucket. A per-file triples
  cache (`.mimus/triples.db`, gitignored) skips re-extracting
  unchanged files, and under `--diff` every graph finding is gated
  to sources the run actually looked at. The mechanical pass is also
  the semantic tier's *intake filter*: what the buckets can't settle
  (near-duplicate subjects — canon-differing but sharing a token —
  asserting different objects for one relation) becomes a pre-bucketed
  candidate pair the agent adjudicates, so it never scans every claim.
  Mechanical speed — buckets, not agents.

### `undocumented fact` vs `mimus coverage`

Both run code→doc, but they answer different questions and sit on
different tiers:

- `mimus coverage` (command, advisory, symbol level) lists
  *symbols* the index holds and no doc source ever names — a
  to-do list for writers, never part of `check`.
- `undocumented fact` (graph class, warn, module level) flags a
  **public module** — one the index records as a dependency
  target of another module — that no document in the mined set
  names. It rides the `check` pipeline (gated by
  `[check].kuda_index`, scoped by `--diff` on the module's own
  files) because a missing module page is a contract gap, not a
  per-symbol chore.

Both match names canonically: `mimus-core`, `mimus_core` and any
`[check].aliases` spelling count. The fact class is
deliberately coarse — every indexed import is a fact nobody
documents, so edge-level reverse coverage would flag the whole
repo; a module is the smallest unit a doc page can cover.
- **Semantic** — advisory: the `acp` checker spawns an ACP agent
  (`acp_cmd`, e.g. `env CLINE_PROVIDER=cline-pass
  CLINE_MODEL=cline-free/mimo-v2.6-flash cline --acp` —
  provider/model are env vars; -P/-m flags are ignored in ACP mode) to verify `behavior` claims —
  including kuda card prose. Runs with `--all` or
  `[check].semantic`; every finding is `warn` + evidence.
  **Can never gate** — that is load-bearing, not a limitation.
  Claims fold into one prompt per batch, and each prompt is
  debounced — polite to the provider. Its intake is the graph
  tier's fuzzy residue (pre-bucketed candidate pairs), not the raw
  claim set.

Style opinions are never emitted at all.

## The evidence bar

Every violation is `claim + evidence + fix(optional)`. If a
checker cannot cite its proof, it emits nothing — false
positives kill a critic faster than false negatives ever could.
This is why `command` has an allowlist, why a bad rule regex is
silence + a doctor complaint, and why the semantic tier exists
separately from the gate.

## Sources

`[sources]` in `mimus.toml`: `docs` (glob list), `comments`
(code comments — TODOs, claims in `///`), and `cards` — the
kuda artifacts get audited as claims too, closing the loop:
kuda remembers, mimus doubts, even about the memory itself.
