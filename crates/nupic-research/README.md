# nupic-research

Workspace member for **research experiments** that back the essays under
[`docs/research/`](../../docs/research/).

## What lives here

- `examples/<topic>_<n>_<slug>.rs` — one runnable per essay claim.
  Output goes to stdout / json / png artefacts under `target/research-out/`
  so anyone can re-run and diff.
- `src/lib.rs` — shared helpers (loaders, scoring, metric harnesses).
  Stays small; pull a helper out only when ≥2 experiments share it.

## Rules

- This crate is **not part of nupic's public API**. `publish = false`.
- Experiments may depend on anything they need to take a measurement
  (academic crates, oracle libs, scratch C bindings). Do not pull those
  deps into `nupic-core`.
- An experiment is "done" when its essay quotes its number and the
  command to re-run it is in the essay.
- An approach that *wins* graduates into `nupic-core` (cement-layer
  improvement) or into a fresh `nupic-<name>` stone crate (e.g.
  `nupic-bits`, `nupic-deflate`). After it graduates, the experiment
  stays here as a regression baseline.

## Running

```bash
cargo run --release -p nupic-research --example <name>
```

(currently empty — experiments land alongside `docs/research/png/01-*`)
