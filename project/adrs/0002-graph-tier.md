# ADR-0002: Graph tier — bidirectional + reflexive checking at mechanical speed

Status: accepted

## Context

mimus verifies *claims* against *reality* — doc→code. The ask is
full coverage in the other directions too: code→doc, code↔code,
doc↔doc, and reflexive (a source contradicting itself). The
semantic tier can find doc↔doc contradictions today, but it
round-trips an ACP agent per batch — minutes on a monorepo, and
warn-tier so it can't gate.

The gap is *structural* consistency: an import edge pointing at a
deleted module (invisible to a Rust compiler, real in TS/Python
monorepos), two docs asserting different objects for the same
slot, a card whose purpose contradicts its notes. An agent finds
these slowly; a hash bucket finds them in milliseconds.

## Options

**A. More semantic passes** — ask the agent "do these docs
contradict each other?" Works today, doesn't scale: O(agent calls),
minutes per sweep, warn-tier output, can't gate. Kept — demoted to
the *last resort* tier for fuzzy candidates.

**B. Parse code in mimus** — treesitter per language to extract
structural triples. Real work, duplicated effort: kuda already
indexes symbols + import/dependent edges for exactly this kind of
question. Rejected — read the index, don't re-parse.

**C. Triple extraction + bucketed contradiction detection
(recommended)** — reduce every source to `(subject, relation,
object)` triples with provenance; literal-grade conflicts emit
mechanically; the rest become *agent adjudication candidates*.
The semantic tier stays the primary semantic reasoner — the
bucket pass is its intake filter, not its replacement: instead
of the agent scanning every claim, it receives pre-bucketed
candidate pairs. Mechanical speed for what must gate, agent
reach for what needs understanding.

**D. Embedding-similarity over claims** — vectorize claims,
cluster near-duplicates, diff their objects. Solves the synonym
problem (`JWT` vs `token auth`) but adds a model dependency,
non-determinism, and can't cite *why* two claims conflict —
the evidence bar needs an inspectable bucket key, not a cosine
score. Rejected; subject normalization below is the
deterministic 80%.

## Proposal

### The model

```
sources ──► extract ──► triples ──► bucket ──► conflicts
  │           │          │          │           │
 docs       regex     (subj,     group by    same (subj,
 comments   tokenize   rel,       (subj, rel)  rel), diff
 .kuda      card       obj)                   object →
 cards      fields                    index   violation
 kuda idx   SQL        edges →               + provenance
                      triples
```

**Triple sources.**

- *Prose* (docs, comments, cards): token-level pattern extraction
  on claim spans the extractor already yields — `X uses Y`,
  `N workers`, `auth is JWT`. Only concrete-literal objects count;
  anything fuzzy stays out of the bucket (see "precision").
- *Code*: the kuda index (`[check].kuda_index`) — symbols,
  `Imports`/`PackageDep` edges, API surface. An edge is already a
  triple: `(importer, imports, target)`.

**Subject canonicalization** — buckets key on the normalized
subject: lowercase, strip stopwords/inflections
(`auth service`≡`auth`), resolve known aliases from a small
repo dictionary (`[check.aliases]` in `mimus.toml`). Aliasing is the
dominant recall risk; normalization is deterministic and
inspectable — every conflict can name the exact bucket key it
matched on.

**Conflict classes** (each maps to a direction of the ask):

| Class | Bucket rule | Direction |
|---|---|---|
| contradiction | same (subj, rel), different obj, different files | doc↔doc |
| self-contradiction | same, within one file | reflexive |
| stale-reference (error tier — mechanical-grade evidence: edge exists, target doesn't) | code edge → target not in index | code↔code |
| unsupported claim | prose triple asserts an edge the index lacks | doc→code |
| undocumented fact | public module (a dependency target) no doc text names | code→doc |

**Precision rule** — the evidence bar (ADR-0001) applies: a
conflict emits only when both objects are concrete literals
(identifier, number, quoted path). Fuzzy pairs become *semantic
candidates* — the agent tiers them, they never gate directly.

### Speed

- Extraction is regex/tokenization for prose + a SQL read for the
  index — no agent, no parsing in mimus. O(n) over sources.
- Compare is a hash bucket by `(subject, relation)` — O(n), not
  pairwise O(n²).
- Triples are cached per file by `(content hash, extractor
  version)` (sqlite, same pattern as kuda's index) → only
  changed files re-extract → `mimus check --diff` + cache ≈
  near-instant PR sweeps on a 10k monorepo.

### Placement

New crate `mimus-check-graph`. The existing `Checker` trait is
per-claim (`check(&Claim)`) — contradiction classes are
*claim-vs-claim*, so the graph pass gets a second port:
`GraphChecker::run_set(&[Claim], &Graph) -> Vec<Violation>`.
Index-backed classes (`stale-reference`, `unsupported-claim`)
also route per-claim where convenient. Warn-tier by default;
`stale-reference` is error-tier (mechanical-grade). Wired into
`mimus check`: the index-edge classes run when `[check].kuda_index`
is set; the prose contradiction classes are index-free and run on
every claim set — no `graph` flag landed.

## Consequences

**Good.** Bidirectional + reflexive coverage at mechanical-tier
speed and mechanical-tier precision; the agent stops being the
bottleneck for bulk contradiction work; kuda's index gains a
second consumer (the ecosystems interlock deepens).

**Costs.** Another crate + a small triples cache; prose triple
extraction is heuristic — the precision rule keeps it honest but
it will miss non-literal contradictions (that's the semantic
tier's job — it's not deleted, it's scoped); stale-reference
findings depend on index freshness (stale index → stale graph —
`mimus doctor` should report index age).

## Rollout

1. `mimus-check-graph`: triples from kuda index edges only —
   `stale-reference` + `unsupported claim` classes, smallest real
   win on TS/Python monorepos. **Done** — gated on
   `[check].kuda_index` being set (no separate `graph` flag);
   `mimus doctor` reports index age (`meta.generated_at`, mtime
   fallback, `!!` over 7 days). Phase-1 detail: a missing edge
   target flags only when *internal-shaped* — it shares the
   repo's own naming family (`@scope/` or a ≥3-char prefix
   segment carried by ≥2 indexed modules), since external deps
   are also absent from the module set; `workspace-member`
   targets flag unconditionally. The unsupported-claim matcher
   is token-level: `X imports|depends on|requires|uses Y`, both
   sides must resolve to indexed modules — anything fuzzier is
   silence.
2. Prose triple extraction + contradiction/self-contradiction
   buckets (docs, comments, cards). **Done** —
   `mimus-claims::triples` (`extract_triples` →
   `contradictions_with_aliases`) wired through
   `mimus-check-graph::ProseContradictionChecker` in `check`'s
   pipeline. Both classes are warn-tier; evidence names the bucket
   key plus every object's `file:line` site, and the violation
   anchors on the first-seen claim. Index-free, so the pass runs
   on every claim set — the `kuda_index` gate stays with the
   index-edge classes only. Subject aliases are a `[check.aliases]`
   table in `mimus.toml` (`alias = "canonical"`, keys normalized
   like subjects) rather than a separate `.mimus/aliases.toml` —
   one config file, no extra loader.
3. Per-file triples cache + `--diff` interplay. **Done** —
   `mimus-check-graph::cache`: sqlite at `.mimus/triples.db`
   (gitignored), keyed on `(path, content-hash, TRIPLES_VERSION)`
   — a `files` marker row holds the key so zero-triple files
   cache too; `triples` payload rows are rewritten wholesale per
   file in one transaction. `extract_triples_cached` groups
   claims by `source`, hashes the file's bytes, and serves
   stored rows on hit; `ProseContradictionChecker` gained a
   `cache: Option<TriplesCache>` field, opened at
   `.mimus/triples.db` in `collect` (open failure → stderr note +
   uncached run). `--diff` interplay: diff filters *claims* for
   the per-claim checkers, but the buckets still read every
   file's triples — a contradiction needs both sides, and the
   cache makes the unchanged side cheap. Findings are gated by
   `in_scope`: a conflict emits only when ≥1 object site sits in
   a checked source, and the anchor prefers the in-scope site.
   `mimus doctor` reports `triples cache ok/--` + row count
   (read-only probe, never creates the db).
4. `undocumented fact` class (reverse coverage — joins the
   existing `mimus coverage` command's direction). **Done** —
   `mimus-check-graph::UndocumentedFactChecker` (`run_facts`),
   wired in `collect` under the `kuda_index` gate, warn tier.
   Two design calls keep it precise where the raw ADR row
   ("index edge no doc triple covers") would drown in noise:
   * module granularity — every indexed import is a fact no doc
     asserts, so edge-level overlap flags the whole repo; the
     **public surface** is instead the set of modules the index
     records as a dependency target (`imports`/`package-dep`/
     `workspace-member` from another module — `calls`, externals
     and self-edges never count);
   * the haystack is mined **document text**, not claims — a crate
     index page that lists `mimus-check-acp` documents it without
     asserting anything checkable, and claim-level matching flagged
     all nine such crates in this repo (measured, then discarded).
   Names match `canon_subject`-style (case, `-`/`_`, plurals,
   `[check].aliases`), and `Graph.module_files` (new provenance
   column read, PRAGMA-guarded so minimal indexes degrade) gates
   findings under `--diff` on the module's own sources. The
   `mimus coverage` distinction is documented in
   `docs/explanation/claims.md`.
5. Semantic tier rescoped to fuzzy-candidate adjudication. **Done** —
   `mimus-check-graph::semantic_candidates` turns the triples the
   buckets can't settle into the agent's intake list: canon-*differing*
   subjects (canon-equal is the buckets' own job), the same relation,
   ≥1 shared canon token (so `auth flow` pairs with `auth service`,
   `auth` never with `cache`), and different objects. One unordered
   pair emits once, sorted, with the original spellings + sites.
   Wiring: `collect` appends one `Behavior` claim per candidate to
   `scoped` after `run_prose` (which keeps its `triples`, so nothing
   re-extracts), so the ACP checker receives pre-bucketed pairs
   instead of scanning every claim — warn tier, advisory. Pairing
   is grouped by relation with precomputed token sets, so the
   intake stays a bucket-ish pass, not O(n²) string work.
6. Docs each phase: `README.md`, `docs/reference/config.md`
   (`[check].graph`, `aliases.toml`), `docs/how-to/ci-gate.md`
   (`--diff` is already shipped + documented), and this ADR's
   status → accepted when phase 2 lands.

## Acceptance criteria

- ≥95% precision on a planted-contradiction fixture corpus
  (docs, comments, cards with known conflicts).
- Sub-second graph pass on a 10k-file monorepo (index-read +
  bucket phase, cache warm).
- `mimus doctor` reports kuda index age; stale index warns.
