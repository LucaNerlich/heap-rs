# heap-rs — Agent context

## Build & test commands

| Command | Purpose |
|---------|---------|
| `cargo build` | Debug build |
| `cargo build --release` | Release build (required for large heap dumps) |
| `cargo test` | Run all unit + integration tests |
| `cargo clippy` | Lint (should be clean before commits) |
| `cargo run --release -- --file heap.hprof --top 30` | Run against a dump |

All commands run from the repo root.

## Project structure

```
src/
  main.rs        CLI: parses args, drives the analysis pipeline
  lib.rs         Public library API; re-exports all modules
  index.rs       Pass 1 — parse dump, collect objects, class layouts, GC roots
  graph.rs       Pass 2 — build CSR object reference graph + shallow histogram
  dominators.rs  Pass 3 — Lengauer–Tarjan dominator tree (iterative DFS)
  parallel.rs    One-time Rayon thread-pool setup
  progress.rs    indicatif-based progress spinners (quiet-mode safe)
  retained.rs    Pass 4 — retained sizes via dominator tree; --explain-class
  report.rs      Terminal tables + CSV export
  testutil/      Synthetic HPROF byte fixtures for unit tests
tests/
  analysis.rs    Integration tests (library API)
  cli.rs         CLI integration tests (assert_cmd)
  common/        Shared fixture helpers for integration tests
plans/           Advisor-generated implementation plans (see plans/README.md)
```

## Key types

- `HeapIndex` (`src/index.rs`) — result of pass 1: object list, class field layouts, GC roots
- `ObjectGraph` (`src/graph.rs`) — CSR graph; nodes are dense u32 ids (sorted by address)
- `RetainedAnalysis` (`src/retained.rs`) — per-class and per-object retained sizes + idom array
- `ProgressGroup` / `PhaseProgress` (`src/progress.rs`) — pass `quiet = true` in tests

## Conventions

- Error handling: functions return `Result<_, String>` at system/parse boundaries; pure computation is infallible
- Parallelism: use `rayon::prelude::*`; configure thread pool once via `parallel::configure`
- No `unsafe` except the single `Mmap::map` call in `main.rs` (required by `memmap2`)
- Tests use synthetic HPROF bytes from `testutil/hprof.rs` — no real `.hprof` file needed
- The `qa.hprof` file at the repo root is a local test file; it is gitignored and not required for tests

## Running a real analysis

```bash
cargo run --release -- --file path/to/heap.hprof --top 50 --quiet
```

For large dumps (> 1 GB), always use `--release`.
