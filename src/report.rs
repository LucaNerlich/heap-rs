use crate::graph::ObjectGraph;
use crate::retained::{ClassRetainedRow, ObjectRetainedRow, RetainedAnalysis};
use std::io;
use std::path::Path;

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

pub fn print_object_table(rows: &[ObjectRetainedRow], top: usize) {
    println!("=== Top {top} Objects by Retained Size ===");
    println!(
        "{:<6} {:>18} {:<50} {:>10} {:>14}",
        "Rank", "Address", "Class", "Shallow", "Retained"
    );
    println!("{}", "-".repeat(104));
    for (i, row) in rows.iter().take(top).enumerate() {
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
