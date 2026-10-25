# ADR-0002: Graph tier — bidirectional + reflexive checking at mechanical speed

Status: proposed

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
object)` triples with provenance; conflicts fall out of a hash
bucket, not an agent. Mechanical speed, agent-grade reach.

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
repo dictionary (`.mimus/aliases.toml`). Aliasing is the
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
| undocumented fact | index edge no doc triple covers | code→doc |

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
`mimus check` behind `[check].graph = true` (or when
`kuda_index` is set).

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
   win on TS/Python monorepos.
2. Prose triple extraction + contradiction/self-contradiction
   buckets (docs, comments, cards).
3. Per-file triples cache + `--diff` interplay.
4. `undocumented fact` class (reverse coverage — joins the
   existing `mimus coverage` command's direction).
5. Semantic tier rescoped to fuzzy-candidate adjudication.
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
