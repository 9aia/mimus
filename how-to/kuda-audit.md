# How-to: audit the memory layer

`mimus audit` applies the same claim machinery to `.kuda/`
cards — agent memory gets a trust score, not a free pass.

## Run

```sh
mimus audit
```

Per card it verifies:

- every `verify:` file still exists
- card `provenance.files` hashes match current content
- symbols claimed in `api:` resolve in the index
- `deps:` edges match the real dep graph

## Reading the score

Each card reports `trust`: fresh | drifted | contradicted.
Drifted cards should be regenerated (`kuda sync`); contradicted
cards need attention — either the code moved without a sync, or
the summarizer invented something (a bug worth reporting).

## Why it matters

Memory without falsification decays into confident confabulation.
The audit is what makes a kuda index *trustworthy* rather than
merely *present*.
