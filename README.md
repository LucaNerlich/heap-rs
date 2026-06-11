# heap-rs

A fast CLI tool for analyzing Java `.hprof` heap dumps. It reports **per-class memory usage** down to the actual class name, and (in full mode) **retained sizes** via a dominator tree — similar to what Eclipse MAT provides.

## Requirements

- [Rust](https://rustup.rs/) 1.70 or newer (2021 edition)
- Enough RAM to hold the parsed object graph in memory (see [Memory](#memory) below)

Supported dump format: **JAVA PROFILE 1.0.2** (standard HotSpot / OpenJDK output, including segmented `HEAP_DUMP_SEGMENT` records).

## Build & run with Cargo

From the project root:

**Debug build** (faster compile, slower runtime):

```bash
cargo build
```

**Release build** (recommended for large dumps):

```bash
cargo build --release
```

The binary lands at `target/release/heap-rs` (or `target/debug/heap-rs` for debug).

### Run without installing

Pass CLI flags after `--` so Cargo does not consume them:

```bash
# release
cargo run --release -- --file heap.hprof --top 50

# debug
cargo run -- --file heap.hprof --shallow-only --top 20
```

Or invoke the built binary directly:

```bash
./target/release/heap-rs --file heap.hprof --top 50
```

### Install onto your PATH

```bash
cargo install --path .
```

Then run from anywhere:

```bash
heap-rs --file heap.hprof --csv classes.csv
```

To reinstall after pulling changes:

```bash
cargo install --path . --force
```

## Installation

If you do not have Rust yet, install it from [rustup.rs](https://rustup.rs/), then clone and build:

```bash
git clone <repo-url>
cd heap-rs
cargo build --release
```

See [Build & run with Cargo](#build--run-with-cargo) for `cargo run`, `cargo install`, and other workflows.

## Capturing a heap dump

From a running JVM:

```bash
jcmd <pid> GC.heap_dump /path/to/heap.hprof
```

Or start the JVM with:

```bash
java -XX:+HeapDumpOnOutOfMemoryError -XX:HeapDumpPath=/path/to/heap.hprof ...
```

## Usage

```bash
heap-rs [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `-f`, `--file <PATH>` | Path to the `.hprof` file (default: `qa.hprof`) |
| `-n`, `--top <N>` | Number of rows to print in each terminal table (default: `30`) |
| `--csv <PATH>` | Write a per-class CSV report |
| `--csv-objects <PATH>` | Write a per-object CSV report (full mode only) |
| `--shallow-only` | Skip dominator computation; output shallow histogram only |

### Examples

All examples below use the release binary; with Cargo, replace `./target/release/heap-rs` with `cargo run --release --`.

**Full retained-size analysis** (recommended when you have enough RAM and time):

```bash
cargo run --release -- --file heap.hprof --top 50
# or: ./target/release/heap-rs --file heap.hprof --top 50
```

**Export class breakdown to CSV:**

```bash
cargo run --release -- --file heap.hprof --csv classes.csv --csv-objects top-objects.csv
```

**Quick shallow histogram** (faster; no dominator tree):

```bash
cargo run --release -- --file heap.hprof --shallow-only --top 20
```

## Output

### Terminal

The tool prints phase timings, then:

1. **Shallow histogram** — instance count and shallow bytes per class (always shown).
2. **Heap summary** — object count, GC roots, reachable vs unreachable objects, total shallow size (full mode).
3. **Top classes by retained size** — instance count, shallow bytes, retained bytes (full mode).
4. **Top objects by retained size** — address, class, shallow and retained bytes (full mode).

**Shallow size** = memory stored in the object itself (fields, array payload, headers).

**Retained size** = memory that would become unreachable if this object were removed (includes everything dominated by it in the heap). This is the most useful metric for finding what is actually holding memory.

### CSV

**`--csv`** writes one row per class:

| Column | Description |
|--------|-------------|
| `class` | Fully qualified class name (e.g. `java.util.HashMap`) |
| `instances` | Number of live instances |
| `shallow_bytes` | Sum of shallow sizes |
| `retained_bytes` | Sum of retained sizes (same as shallow in `--shallow-only` mode) |

**`--csv-objects`** writes one row per object (sorted by retained size, full list):

| Column | Description |
|--------|-------------|
| `address` | Object address in the dump (hex) |
| `class` | Class name |
| `shallow_bytes` | Shallow size of this object |
| `retained_bytes` | Retained size of this object |

## How it works

1. **Parse** — the dump is memory-mapped and parsed with [`jvm-hprof`](https://crates.io/crates/jvm-hprof).
2. **Index** — classes, instances, arrays, and GC roots are collected; class field layouts are merged across the inheritance chain.
3. **Graph** — object references are extracted into a compact CSR edge list.
4. **Dominators** — Lengauer–Tarjan computes the dominator tree from a synthetic super-root over all GC roots.
5. **Retained sizes** — accumulated up the dominator tree and aggregated by class.

Use `--shallow-only` to stop after step 3 and get a fast sanity check that parsing is correct.

## Memory

Memory use scales with the number of objects and references in the dump, not just file size. As a rough guide:

| Dump scale | Objects (approx.) | RAM needed (approx.) |
|------------|-------------------|----------------------|
| Small | &lt; 10 M | 2–4 GB |
| Medium | 10–50 M | 8–16 GB |
| Large | 50–150 M | 16–32 GB+ |

Full retained analysis on multi-gigabyte production dumps can take tens of minutes. Start with `--shallow-only` on a new dump to validate parsing before running the full analysis.

## Project layout

```
src/
  main.rs        CLI entry point
  index.rs       Pass 1: parse heap, build class layouts and object list
  graph.rs       Object graph (CSR edges) and shallow histogram
  dominators.rs  Lengauer–Tarjan dominator computation
  retained.rs    Retained-size aggregation
  report.rs      Terminal tables and CSV export
```

## License

See repository license file if present.
