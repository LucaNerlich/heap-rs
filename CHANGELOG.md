# Changelog

All notable changes to this project will be documented in this file.

## [1.2.0] - 2026-06-14

### Fixed

- Replaced recursive `compress()` in the Lengauer–Tarjan dominator computation with an iterative version — deep heap chains (linked lists, long inheritance hierarchies) no longer cause a stack overflow
- Fixed O(n × max_depth) retained-size accumulation that caused the tool to hang indefinitely on heaps with long reference chains (e.g. linked lists of 1M+ nodes)
- Fixed silent `u32` truncation of shallow sizes — arrays larger than ~4 GB (e.g. `long[]` at maximum Java array size) now report correct retained sizes; `ObjectMeta.shallow` and `ObjectGraph.shallow` are now `u64`
- Fixed `truncate()` panic on class names containing non-ASCII characters (Unicode package/class names such as Chinese or Japanese identifiers)
- Fixed redundant third condition in `class_matches` that was flagged as a logic bug by clippy

### Changed

- Dominator DFS no longer allocates a `Vec` per visited node — children are now borrowed directly from the graph's edge slices, reducing allocation pressure on large heaps
- Added `CLAUDE.md` at the repo root to orient AI coding agents with build commands and project layout

## [1.1.1] - 2026-06-12

### Changed

- Added extensive rustdoc across the public API and modules so the generated docs.rs documentation is fully populated

## [1.1.0] - 2026-06-11

### Added

- `--jobs` / `-j` flag to control worker thread count for parallel analysis phases

### Changed

- Parallel analysis with Rayon across class layout finalization, object graph construction, retained-size aggregation, and `--explain-class`
- Object graph build uses a single heap scan instead of two when collecting edges
- README documents parallelism, `--jobs`, and a full example command with CSV export and `byte[]` analysis

## [1.0.2] - 2026-06-11

### Changed

- Upgraded to Rust 2024 edition (requires Rust 1.85+)
- CI workflow pins Rust 1.85 for edition 2024 support

## [1.0.1] - 2026-06-11

### Added

- `--explain-class` to show who references instances of a class (largest instances and top retainer classes)
- `--class` filter for the object retained-size table
- Comprehensive test suite with synthetic HPROF fixtures and CLI integration tests
- GitHub Actions CI workflow for Rust builds and tests
- Crates.io publish metadata and MIT license

### Fixed

- Dominator tree computation so retained sizes propagate correctly through object chains (Lengauer–Tarjan link step and 1-based vertex indexing)

### Changed

- README documentation for retainer analysis, leaf-type retained-size behavior, and project layout
