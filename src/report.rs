//! Terminal tables and CSV export.
//!
//! These helpers render the results of [`crate::retained::compute_retained`] and
//! [`crate::retained::explain_class`] as aligned terminal tables, and write the
//! per-class and per-object breakdowns to CSV for further analysis in a
//! spreadsheet.

use crate::graph::ObjectGraph;
use crate::retained::{ClassRetainedRow, ObjectRetainedRow, RetainedAnalysis};
use std::io;
use std::path::Path;

/// Format a byte count using binary (1024-based) units: `B`, `KB`, `MB`, `GB`.
///
/// ```
/// assert_eq!(heap_rs::report::format_bytes(2048), "2.00 KB");
/// ```
pub fn format_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.2} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.2} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

/// Print the heap summary: object count, GC roots, reachable/unreachable
/// counts, total shallow size, and the largest retained subtree.
pub fn print_summary(analysis: &RetainedAnalysis, graph: &ObjectGraph) {
    println!("=== Heap Summary ===");
    println!("Objects: {}", graph.num_nodes);
    println!("GC roots: {}", graph.roots.len());
    println!(
        "Reachable: {} | Unreachable: {}",
        analysis.reachable_objects, analysis.unreachable_objects
    );
    println!(
        "Total shallow: {} ({})",
        analysis.total_shallow,
        format_bytes(analysis.total_shallow)
    );
    println!(
        "Largest retained subtree: {} ({})",
        analysis.total_retained,
        format_bytes(analysis.total_retained)
    );
    println!();
}

/// Print the top `top` classes ranked by retained size.
pub fn print_class_table(rows: &[ClassRetainedRow], top: usize) {
    println!("=== Top {top} Classes by Retained Size ===");
    println!(
        "{:<6} {:<60} {:>12} {:>14} {:>14}",
        "Rank", "Class", "Instances", "Shallow", "Retained"
    );
    println!("{}", "-".repeat(110));
    for (i, row) in rows.iter().take(top).enumerate() {
        println!(
            "{:<6} {:<60} {:>12} {:>14} {:>14}",
            i + 1,
            truncate(&row.class_name, 60),
            row.instance_count,
            format_bytes(row.shallow_bytes),
            format_bytes(row.retained_bytes),
        );
    }
    println!();
}

/// Print the top `top` objects ranked by retained size.
///
/// When `class_filter` is `Some`, only objects whose class matches the filter
/// (see [`crate::retained::class_matches`]) are shown.
pub fn print_object_table(rows: &[ObjectRetainedRow], top: usize, class_filter: Option<&str>) {
    let filtered: Vec<_> = match class_filter {
        Some(f) => rows
            .iter()
            .filter(|r| crate::retained::class_matches(&r.class_name, f))
            .collect(),
        None => rows.iter().collect(),
    };
    let title = match class_filter {
        Some(f) => format!("Top {top} `{f}` Objects by Retained Size"),
        None => format!("Top {top} Objects by Retained Size"),
    };
    println!("=== {title} ===");
    println!(
        "{:<6} {:>18} {:<50} {:>10} {:>14}",
        "Rank", "Address", "Class", "Shallow", "Retained"
    );
    println!("{}", "-".repeat(104));
    for (i, row) in filtered.iter().take(top).enumerate() {
        println!(
            "{:<6} 0x{:016x} {:<50} {:>10} {:>14}",
            i + 1,
            row.addr,
            truncate(&row.class_name, 50),
            format_bytes(row.shallow_bytes),
            format_bytes(row.retained_bytes),
        );
    }
    println!();
}

/// Print a [`ClassExplanation`](crate::retained::ClassExplanation): the largest
/// matched instances and the classes that reference them, capped at `top` rows.
pub fn print_class_explanation(explanation: &crate::retained::ClassExplanation, top: usize) {
    use crate::retained::ClassExplanation;
    let ClassExplanation {
        class_name,
        instance_count,
        total_shallow,
        top_instances,
        top_retainers,
    } = explanation;

    println!("=== Why is `{class_name}` using memory? ===");
    println!(
        "{} instances, {} total shallow",
        instance_count,
        format_bytes(*total_shallow)
    );
    println!(
        "Note: for leaf types like arrays, retained size equals shallow size per instance."
    );
    println!(
        "Incoming references show which object types directly point at these instances."
    );
    println!();

    println!("=== Top {top} largest `{class_name}` instances ===");
    println!(
        "{:<6} {:>18} {:>12} {:<50} {:>18}",
        "Rank", "Address", "Shallow", "Referenced from (class)", "Referrer address"
    );
    println!("{}", "-".repeat(112));
    for (i, inst) in top_instances.iter().take(top).enumerate() {
        let retainer_addr = if inst.retainer_addr == 0 {
            "—".to_string()
        } else {
            format!("0x{:016x}", inst.retainer_addr)
        };
        println!(
            "{:<6} 0x{:016x} {:>12} {:<50} {:>18}",
            i + 1,
            inst.addr,
            format_bytes(inst.shallow_bytes),
            truncate(&inst.retainer_class, 50),
            retainer_addr,
        );
    }
    println!();

    println!("=== Top {top} classes with incoming refs to `{class_name}` ===");
    println!(
        "{:<6} {:<60} {:>12} {:>14}",
        "Rank", "Retainer class", "Instances", "Shallow"
    );
    println!("{}", "-".repeat(96));
    for (i, row) in top_retainers.iter().take(top).enumerate() {
        println!(
            "{:<6} {:<60} {:>12} {:>14}",
            i + 1,
            truncate(&row.retainer_class, 60),
            row.instance_count,
            format_bytes(row.shallow_bytes),
        );
    }
    println!();
}

/// Print the top `top` classes by shallow size.
///
/// This is the cheap checkpoint table that does not need the dominator tree; it
/// is shown in every mode, including `--shallow-only`.
pub fn print_shallow_histogram(graph: &ObjectGraph, top: usize) {
    let rows = graph.shallow_histogram();
    println!("=== Top {top} Classes by Shallow Size (checkpoint) ===");
    println!("{:<6} {:<60} {:>12} {:>14}", "Rank", "Class", "Instances", "Shallow");
    println!("{}", "-".repeat(96));
    for (i, (name, count, bytes)) in rows.iter().take(top).enumerate() {
        println!(
            "{:<6} {:<60} {:>12} {:>14}",
            i + 1,
            truncate(name, 60),
            count,
            format_bytes(*bytes),
        );
    }
    println!();
}

/// Write the per-class breakdown to a CSV file.
///
/// Columns: `class,instances,shallow_bytes,retained_bytes`.
///
/// # Errors
///
/// Returns an [`io::Error`] if the file cannot be created or written.
pub fn write_class_csv(path: &Path, rows: &[ClassRetainedRow]) -> io::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(["class", "instances", "shallow_bytes", "retained_bytes"])?;
    for row in rows {
        wtr.write_record([
            &row.class_name,
            &row.instance_count.to_string(),
            &row.shallow_bytes.to_string(),
            &row.retained_bytes.to_string(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

/// Write the per-object breakdown to a CSV file.
///
/// Columns: `address,class,shallow_bytes,retained_bytes`. When `limit` is
/// `Some(n)`, only the first `n` rows are written; otherwise all rows are.
///
/// # Errors
///
/// Returns an [`io::Error`] if the file cannot be created or written.
pub fn write_object_csv(path: &Path, rows: &[ObjectRetainedRow], limit: Option<usize>) -> io::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(["address", "class", "shallow_bytes", "retained_bytes"])?;
    let iter = rows.iter().take(limit.unwrap_or(rows.len()));
    for row in iter {
        wtr.write_record([
            &format!("0x{:x}", row.addr),
            &row.class_name,
            &row.shallow_bytes.to_string(),
            &row.retained_bytes.to_string(),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retained::{ClassRetainedRow, ObjectRetainedRow};

    #[test]
    fn format_bytes_uses_binary_prefixes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.00 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn write_class_csv_has_header_and_rows() {
        let rows = vec![
            ClassRetainedRow {
                class_name: "java/lang/String".into(),
                instance_count: 10,
                shallow_bytes: 320,
                retained_bytes: 640,
            },
            ClassRetainedRow {
                class_name: "com/example/Node".into(),
                instance_count: 3,
                shallow_bytes: 72,
                retained_bytes: 72,
            },
        ];
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("classes.csv");
        write_class_csv(&path, &rows).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.starts_with("class,instances,shallow_bytes,retained_bytes\n"));
        assert!(content.contains("java/lang/String,10,320,640"));
    }

    #[test]
    fn write_object_csv_respects_limit() {
        let rows: Vec<ObjectRetainedRow> = (0..5)
            .map(|i| ObjectRetainedRow {
                addr: 0x1000 + i,
                class_name: "X".into(),
                shallow_bytes: 16,
                retained_bytes: 16 * (i + 1),
            })
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("objects.csv");
        write_object_csv(&path, &rows, Some(2)).unwrap();

        let content = std::fs::read_to_string(path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 3); // header + 2 rows
    }

    #[test]
    fn truncate_long_class_names() {
        let long = "a".repeat(80);
        let truncated = truncate(&long, 60);
        assert!(truncated.ends_with('…'));
        assert!(truncated.chars().count() <= 60);
    }
}
