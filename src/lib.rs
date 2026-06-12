//! # heap-rs
//!
//! A fast library and CLI for analyzing Java `.hprof` heap dumps. It computes
//! **per-class memory usage** down to the concrete class name and, in full mode,
//! **retained sizes** via a dominator tree — comparable to what Eclipse MAT
//! provides.
//!
//! The supported dump format is **JAVA PROFILE 1.0.2** (standard HotSpot /
//! OpenJDK output, including segmented `HEAP_DUMP_SEGMENT` records).
//!
//! ## Pipeline overview
//!
//! Analysis runs as a sequence of phases, each implemented by its own module:
//!
//! 1. [`index`] — parse the dump and build a flat list of objects, class field
//!    layouts (merged across the inheritance chain), and GC roots.
//! 2. [`graph`] — turn object references into a compact
//!    [CSR](https://en.wikipedia.org/wiki/Sparse_matrix#Compressed_sparse_row_(CSR,_CRS_or_Yale_format))
//!    edge list ([`graph::ObjectGraph`]).
//! 3. [`dominators`] — compute the dominator tree from a synthetic super-root
//!    over all GC roots using Lengauer–Tarjan ([`dominators::compute_dominators`]).
//! 4. [`retained`] — accumulate retained sizes up the dominator tree and
//!    aggregate them by class ([`retained::compute_retained`]).
//! 5. [`report`] — render terminal tables and CSV exports.
//!
//! The [`progress`] module provides live progress reporting shared across
//! phases, and [`parallel`] configures the global [Rayon](https://docs.rs/rayon)
//! thread pool used to parallelize the heavier phases.
//!
//! ## Example
//!
//! ```no_run
//! use heap_rs::{graph, index, retained};
//! use jvm_hprof::parse_hprof;
//! use memmap2::Mmap;
//! use std::fs::File;
//!
//! # fn main() -> Result<(), String> {
//! // Memory-map the dump and parse its header/record stream.
//! let file = File::open("heap.hprof").map_err(|e| e.to_string())?;
//! let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };
//! let hprof = parse_hprof(&mmap[..]).map_err(|e| format!("{e:?}"))?;
//!
//! // Phase 1 & 2: index the heap and build the object graph.
//! let index = index::HeapIndex::build(&hprof, true)?;
//! let graph = graph::ObjectGraph::build(&hprof, &index, true)?;
//!
//! // Phase 3 & 4: dominators + retained sizes, aggregated per class.
//! let analysis = retained::compute_retained(&graph, true);
//! for row in analysis.class_rows.iter().take(10) {
//!     println!("{:>14} {}", row.retained_bytes, row.class_name);
//! }
//! # Ok(())
//! # }
//! ```

/// Lengauer–Tarjan dominator-tree computation (see [`dominators::compute_dominators`]).
pub mod dominators;
/// The object reference graph in compressed-sparse-row form (see [`graph::ObjectGraph`]).
pub mod graph;
/// First-pass heap indexing: objects, class layouts, and GC roots (see [`index::HeapIndex`]).
pub mod index;
/// Rayon thread-pool configuration (see [`parallel::configure`]).
pub mod parallel;
/// Live progress reporting shared across analysis phases (see [`progress::ProgressGroup`]).
pub mod progress;
/// Terminal tables and CSV export helpers (see [`report::print_class_table`]).
pub mod report;
/// Retained-size aggregation and class retainer analysis (see [`retained::compute_retained`]).
pub mod retained;

#[cfg(test)]
pub mod testutil;
