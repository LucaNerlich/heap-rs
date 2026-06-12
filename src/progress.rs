//! Live progress reporting shared across analysis phases.
//!
//! A [`ProgressGroup`](crate::progress::ProgressGroup) represents a multi-step
//! phase (for example "Building object graph" with 4 steps). Each step is a
//! [`PhaseProgress`](crate::progress::PhaseProgress) spinner that accumulates
//! [`Counters`](crate::progress::Counters) (objects, edges, nodes, …) and
//! refreshes on a timer. In `quiet` mode every spinner is inert, so the same
//! calling code works for both interactive and CI/log output.

use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};

const UPDATE_INTERVAL: Duration = Duration::from_secs(2);
const UPDATE_EVERY: u64 = 5_000_000;

/// Running tallies displayed by a [`PhaseProgress`] spinner.
#[derive(Default, Clone)]
pub struct Counters {
    /// Heap sub-records processed.
    pub sub_records: u64,
    /// Heap dump segments processed.
    pub segments: u64,
    /// Objects (instances and arrays) seen.
    pub objects: u64,
    /// Class dumps seen.
    pub classes: u64,
    /// GC roots seen.
    pub roots: u64,
    /// Graph edges processed.
    pub edges: u64,
    /// Generic work units (e.g. nodes finalized).
    pub nodes: u64,
}

/// A multi-step analysis phase that hands out per-step [`PhaseProgress`] spinners.
pub struct ProgressGroup {
    phase: String,
    total_steps: u32,
    quiet: bool,
}

impl ProgressGroup {
    /// Create a progress group for a named `phase` with `total_steps` steps.
    ///
    /// When `quiet` is `true`, all spinners produced by this group are inert.
    pub fn new(phase: impl Into<String>, total_steps: u32, quiet: bool) -> Self {
        Self {
            phase: phase.into(),
            total_steps,
            quiet,
        }
    }

    /// Begin step `step` of this group, labelled `name`, returning its spinner.
    pub fn begin(&self, step: u32, name: impl Into<String>) -> PhaseProgress {
        PhaseProgress::subtask(
            self.phase.clone(),
            step,
            self.total_steps,
            name,
            self.quiet,
        )
    }
}

/// A single-step progress spinner that accumulates [`Counters`] and refreshes
/// on a timer. Obtain one from [`ProgressGroup::begin`].
pub struct PhaseProgress {
    bar: ProgressBar,
    group_phase: String,
    step: u32,
    total_steps: u32,
    subtask: String,
    started: Instant,
    last_update: Instant,
    counters: Counters,
    quiet: bool,
}

impl PhaseProgress {
    fn subtask(
        group_phase: String,
        step: u32,
        total_steps: u32,
        name: impl Into<String>,
        quiet: bool,
    ) -> Self {
        let subtask = name.into();
        if quiet {
            return Self {
                bar: ProgressBar::hidden(),
                group_phase,
                step,
                total_steps,
                subtask,
                started: Instant::now(),
                last_update: Instant::now(),
                counters: Counters::default(),
                quiet: true,
            };
        }

        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner:.green} {msg}")
                .unwrap()
                .tick_strings(&[
                    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
                ]),
        );
        bar.enable_steady_tick(Duration::from_millis(100));

        let progress = Self {
            bar,
            group_phase,
            step,
            total_steps,
            subtask,
            started: Instant::now(),
            last_update: Instant::now(),
            counters: Counters::default(),
            quiet: false,
        };
        progress.refresh();
        progress
    }

    /// Increment the sub-record counter and refresh the display if due.
    pub fn tick_sub_record(&mut self) {
        self.counters.sub_records += 1;
        self.maybe_refresh();
    }

    /// Increment the segment counter.
    pub fn tick_segment(&mut self) {
        self.counters.segments += 1;
    }

    /// Increment the object counter and refresh the display if due.
    pub fn add_object(&mut self) {
        self.counters.objects += 1;
        self.maybe_refresh();
    }

    /// Increment the class counter.
    pub fn add_class(&mut self) {
        self.counters.classes += 1;
    }

    /// Increment the GC-root counter.
    pub fn add_root(&mut self) {
        self.counters.roots += 1;
    }

    /// Add `n` to the edge counter and refresh the display if due.
    pub fn add_edges(&mut self, n: u64) {
        self.counters.edges += n;
        self.maybe_refresh();
    }

    /// Add `n` to the generic node/work counter and refresh the display if due.
    pub fn add_nodes(&mut self, n: u64) {
        self.counters.nodes += n;
        self.maybe_refresh();
    }

    fn maybe_refresh(&mut self) {
        if self.quiet {
            return;
        }
        let due = self.last_update.elapsed() >= UPDATE_INTERVAL
            || (self.counters.sub_records > 0
                && self.counters.sub_records % UPDATE_EVERY == 0);
        if due {
            self.refresh();
            self.last_update = Instant::now();
        }
    }

    fn refresh(&self) {
        self.bar.set_message(self.format_line(None));
    }

    fn format_line(&self, summary: Option<&str>) -> String {
        if let Some(summary) = summary {
            return summary.to_string();
        }

        let c = &self.counters;
        let elapsed = self.started.elapsed();
        let secs = elapsed.as_secs_f64().max(0.001);

        let mut parts = Vec::new();
        if !self.group_phase.is_empty() && self.total_steps > 1 {
            parts.push(format!(
                "{} [{}/{}] {}",
                self.group_phase, self.step, self.total_steps, self.subtask
            ));
        } else if !self.subtask.is_empty() {
            parts.push(self.subtask.clone());
        }

        if c.segments > 0 {
            parts.push(format!("{} segs", format_count(c.segments)));
        }
        if c.sub_records > 0 {
            parts.push(format!("{} subs", format_count(c.sub_records)));
            parts.push(format!("{:.0}/s", c.sub_records as f64 / secs));
        }
        if c.objects > 0 {
            parts.push(format!("{} objs", format_count(c.objects)));
        }
        if c.classes > 0 {
            parts.push(format!("{} classes", format_count(c.classes)));
        }
        if c.roots > 0 {
            parts.push(format!("{} roots", format_count(c.roots)));
        }
        if c.edges > 0 {
            parts.push(format!("{} edges", format_count(c.edges)));
            parts.push(format!("{:.0}/s", c.edges as f64 / secs));
        }
        if c.nodes > 0 {
            parts.push(format!("{} nodes", format_count(c.nodes)));
        }
        parts.push(format!("{elapsed:.1?}"));

        parts.join(" | ")
    }

    /// Finish this step, replacing the spinner with a one-line `summary`.
    ///
    /// No-op in quiet mode.
    pub fn finish(&self, summary: impl Into<String>) {
        if self.quiet {
            return;
        }
        let summary = summary.into();
        let line = if summary.is_empty() {
            self.format_line(None)
        } else if self.total_steps > 1 {
            format!(
                "{} [{}/{}] {} — {}",
                self.group_phase, self.step, self.total_steps, self.subtask, summary
            )
        } else {
            format!("{} — {}", self.subtask, summary)
        };
        self.bar.finish_with_message(line);
    }
}

pub(crate) fn format_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_count_scales_units() {
        assert_eq!(format_count(42), "42");
        assert_eq!(format_count(1_500), "1.50K");
        assert_eq!(format_count(2_500_000), "2.50M");
        assert_eq!(format_count(3_500_000_000), "3.50B");
    }

    #[test]
    fn progress_group_quiet_mode_is_inert() {
        let group = ProgressGroup::new("phase", 2, true);
        let mut progress = group.begin(1, "step");
        progress.tick_sub_record();
        progress.add_object();
        progress.add_edges(10);
        progress.finish("done");
    }

    #[test]
    fn progress_group_tracks_counters_without_panic() {
        let group = ProgressGroup::new("Indexing", 2, true);
        let mut progress = group.begin(1, "scan");
        progress.tick_segment();
        progress.tick_sub_record();
        progress.add_object();
        progress.add_class();
        progress.add_root();
        progress.add_edges(3);
        progress.add_nodes(5);
        progress.finish("5 nodes indexed");
    }
}
