---
url: /mimus/explanation/boundaries.md
---
# Boundaries

mimus owns **verification** — claim extraction, checkers, rules,
violations. It does not own:

* **Memory** — `kuda` builds and stores the index; mimus reads it
  (symbol checker, `audit`, graph edges) and never writes cards.
* **Fixing** — `reconcile` (planned) consumes mimus's JSON
  violation contract to auto-fix; mimus reports, never rewrites.
* **Distribution** — the `mimus` command ships as a tamias pack
  (`libraries/mimus`); tamias owns install/transport.
* **Secrets** — `mimus-secret` ports resolve credentials; values
  never enter findings or logs.
* **Style** — style opinions are never emitted at all; mimus
  checks truth, not taste.
