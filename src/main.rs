//! Command-line entry point for `heap-rs`.
//!
//! Parses CLI arguments, memory-maps the dump, and drives the analysis pipeline
//! exposed by the [`heap_rs`] library. See the library crate docs for the API
//! used here.

use clap::Parser;
use heap_rs::{graph, index, report, retained};
use jvm_hprof::parse_hprof;
use memmap2::Mmap;
use std::fs::File;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "heap-rs", about = "Analyze Java hprof heap dumps for retained memory")]
struct Args {
    /// Path to the .hprof file
    #[arg(short, long, default_value = "qa.hprof")]
    file: PathBuf,

    /// Number of top entries to print to the terminal
    #[arg(short = 'n', long, default_value_t = 30)]
    top: usize,

    /// Write per-class CSV to this path
    #[arg(long)]
    csv: Option<PathBuf>,

    /// Write per-object CSV to this path
    #[arg(long)]
    csv_objects: Option<PathBuf>,

    /// Filter object table to instances of this class (e.g. `byte[]`)
    #[arg(long)]
    class: Option<String>,

    /// Explain what retains instances of a class (who keeps them alive)
    #[arg(long)]
    explain_class: Option<String>,

    /// Skip dominator computation (shallow histogram only)
    #[arg(long)]
    shallow_only: bool,

    /// Disable progress spinners (for CI/log files)
    #[arg(long)]
    quiet: bool,

    /// Number of worker threads for parallel phases (default: logical CPU count)
    #[arg(long = "jobs", short = 'j')]
    jobs: Option<usize>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    heap_rs::parallel::configure(args.jobs)?;

    let total_start = Instant::now();

    println!("Opening {} …", args.file.display());
    let file = File::open(&args.file).map_err(|e| e.to_string())?;
    let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };

    let hprof = parse_hprof(&mmap[..]).map_err(|e| format!("{e:?}"))?;
    println!(
        "HPROF id_size={:?} timestamp={}",
        hprof.header().id_size(),
        hprof.header().timestamp_millis()
    );

    let t0 = Instant::now();
    let index = index::HeapIndex::build(&hprof, args.quiet)?;
    let pass1 = t0.elapsed();
    if args.quiet {
        println!(
            "Pass 1: {} objects, {} classes, {} roots ({pass1:.1?})",
            index.objects.len(),
            index.classes.len(),
            index.roots.len()
        );
    }

    let t1 = Instant::now();
    let graph = graph::ObjectGraph::build(&hprof, &index, args.quiet)?;
    let graph_time = t1.elapsed();
    if args.quiet {
        println!("Object graph: {} edges ({graph_time:.1?})", graph.targets.len());
    }

    report::print_shallow_histogram(&graph, args.top);

    if args.shallow_only {
        if let Some(path) = &args.csv {
            let rows: Vec<retained::ClassRetainedRow> = graph
                .shallow_histogram()
                .into_iter()
                .map(|(name, count, bytes)| retained::ClassRetainedRow {
                    class_name: name,
                    instance_count: count,
                    shallow_bytes: bytes,
                    retained_bytes: bytes,
                })
                .collect();
            report::write_class_csv(path, &rows).map_err(|e| e.to_string())?;
            println!("Wrote class CSV to {}", path.display());
        }
        println!("Done in {:.1?}", total_start.elapsed());
        return Ok(());
    }

    let t2 = Instant::now();
    let analysis = retained::compute_retained(&graph, args.quiet);
    let retained_time = t2.elapsed();
    if args.quiet {
        println!("Retained analysis ({retained_time:.1?})");
    }

    report::print_summary(&analysis, &graph);
    report::print_class_table(&analysis.class_rows, args.top);
    report::print_object_table(&analysis.top_objects, args.top, args.class.as_deref());

    if let Some(ref explain) = args.explain_class {
        if let Some(explanation) = retained::explain_class(&graph, explain, args.top, args.top) {
            report::print_class_explanation(&explanation, args.top);
        } else {
            eprintln!("No instances found for class filter `{explain}`");
        }
    }

    if let Some(path) = &args.csv {
        report::write_class_csv(path, &analysis.class_rows).map_err(|e| e.to_string())?;
        println!("Wrote class CSV to {}", path.display());
    }
    if let Some(path) = &args.csv_objects {
        report::write_object_csv(path, &analysis.top_objects, None).map_err(|e| e.to_string())?;
        println!("Wrote object CSV to {}", path.display());
    }

    println!("Done in {:.1?}", total_start.elapsed());
    Ok(())
}
