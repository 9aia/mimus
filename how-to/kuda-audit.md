# How-to: audit kuda memory

`mimus audit` is the falsifier for the memory layer: it verifies
every `.kuda/modules/*.yaml` card against source — memory
audited, not just generated.

## Run

```sh
kuda sync        # (re)generate the index first
mimus audit
```

```
fresh        kuda-store
fresh        kuda-core
drifted      kuda-cli
20 fresh, 1 drifted, 0 contradicted
```

## Scores

| score          | meaning                                              |
| -------------- | ---------------------------------------------------- |
| `fresh`        | every `provenance.files` hash still matches          |
| `drifted`      | a verify file's content changed since sync           |
| `contradicted` | a provenance file no longer exists                   |

Audit recomputes git `hash-object` per `provenance.files` entry
and compares with what the card recorded at sync time — the same
provenance model `kuda status` uses, checked from the outside.

## Exit semantics

Audit is report-tier: exit `0` even with contradictions. It is
diagnosis, not a gate — pair it with `mimus check` if you want
drift to fail CI, or keep it in a scheduled report job. The fix
is upstream anyway: `kuda sync`, not a hand-edit.

## What it does not check (yet)

Audit verifies *freshness* (provenance hashes), not *accuracy*
of the card's prose fields — `api`/`deps` drift beyond their
recorded inputs, or a hand-written `notes:` that has gone stale,
is the semantic tier's job when it lands.
