# Changelog

All notable changes to this project will be documented in this file.

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
