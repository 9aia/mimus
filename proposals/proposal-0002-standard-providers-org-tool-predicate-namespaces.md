+++
id = "P-002"
title = "Standard providers — org-tool predicate namespaces for spec packs"
status = "open"
created = "2026-11-24"
owner = "devin"
priority = "med"
+++

# Standard providers — org-tool predicate namespaces

## Problem

P-001's spec format has a `providers` seam (`extra` catch-all today):
each 9aia tool owns a predicate namespace (`lasius.*`, `bupha.*`,
`cypera.*`, `kuda.*`, `myco.*`, `tamias.*`) so specs declare intent
("CI gate must be the bupha context") and the owning tool implements
the check. Without it the standard stays file-set-only — it can say
*which files* must exist, not *what they must mean*.

## Proposal

`StandardProvider` port in `mimus-spec` (or `mimus-core` if it must
stay I/O-free):

```rust
pub trait StandardProvider {
    /// Namespace prefix — "bupha", "lasius", …
    fn ns(&self) -> &str;
    /// Evaluate one predicate table against the repo.
    /// `claims` = the `[providers.<ns>]` table from the spec.
    fn check(&self, repo: &Path, claims: &Value) -> Result<Vec<SpecViolation>, ProviderError>;
}
```

Spec side (composes via existing `extra` → promote to typed):

```toml
[providers.bupha]
context = "ci/bupha"        # required status context on the repo

[providers.lasius]
clone = "present"           # lasius-managed clone metadata sane
```

Semantics:

- **Namespace dispatch** — spec keys under `providers.<ns>` route to
  the provider whose `ns()` matches. Unknown namespace → spec-load
  error (typo ≠ silent pass, same rule as path tables).
- **Degrade to warn** — provider crate not installed/registered →
  `provider-unavailable` warn violation, never a hard fail. The
  file-set tier always runs.
- **In-process trait v1** — providers are Rust impls linked into the
  mimus binary (or loaded via the existing tamias driver surface
  later). Driver-surface dispatch is the phase-2 question, not v1.
- **First providers** — `lasius` (repo is lasius-managed, remotes
  sane) and `bupha` (required CI contexts) are the highest-value;
  `cypera`/`kuda`/`myco`/`tamias` follow the same shape.

## Non-goals

- No LLM/semantic claims — mechanical predicates only (acp tier
  stays advisory per ADR-0003).
- No remote/provider-over-network in v1 — in-process or local CLI
  probes (`bupha status --json`-style) only.
- Spec distribution via tamius stays P-001's deferred item.

## Open questions

- Provider ABI: trait in mimus-spec vs a `mimus-provider` port crate
  (keeps mimus-spec pure)? Lean: port crate, mirroring
  mimus-check/mimus-check-* layout.
- How do provider crates register — feature flags on the mimus bin,
  or tamias pack composition?
