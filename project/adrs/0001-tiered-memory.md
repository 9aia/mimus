# ADR-0001: Tiered memory artifacts + ports/adapters topology

Status: proposed

## Context

Agent sessions re-explore repos from scratch: no overall
comprehension, expensive planning, and doc/comment sweeps degrade
to undirected greps. A repo "memory" layer must (a) fit small
context budgets, (b) stay provably fresh, (c) span languages and
ecosystems, (d) never become a second source of truth that
contradicts the code.

## Options

**A. One generated summary doc** — too coarse to plan with, or too
big to keep in context. Rejected.

**B. Embedding/RAG store** — language-agnostic and good at fuzzy
lookup, but weak on structure (dep graphs, provenance, exact
symbol resolution) and opaque about staleness. Deferred — viable
later as a `malurus-store` impl.

**C. Tiered index + query (recommended)** — L0 repo card
(always-load), L1 module cards (on-demand), L2 symbol/dep graph
(queried). Everything carries provenance; memory is an index into
code, not a copy.

## Proposal

- Every concern is a port + impl crates (bupha topology):
  extract (`manifest` | `treesitter` | `lsp`), summarize
  (`template` | `llm`), store (`sqlite` first), vcs (`git`),
  serve (`cli` | `mcp`).
- Per-language knowledge = data (`data/langs/*`: grammar refs,
  symbol queries, manifest kinds), never extractor code.
  New language = data change, not release.
- `template` summarizer (zero-model, deterministic) is the
  default; LLM prose is an impl behind the port.
- Committed artifacts: `.malurus/index.yaml`, `.malurus/modules/*.yaml`.
  Gitignored: `.malurus/index.sqlite` (rebuildable).
- Provenance per card `{files, content_hash, commit_sha}` →
  `malurus status` = sha diff vs HEAD. Cards end with `verify:`
  file lists — agents must check memory against source.

## Consequences

- Staleness is a property of the data, not a promise in prose.
- Incremental regen (content-hash → affected cards only) keeps
  `sync` cheap on large workspaces.
- Malurus (the critic, 9aia/malurus) builds on the same index:
  claim extraction + verification reuse the query layer.
