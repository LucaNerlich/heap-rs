use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};

const UPDATE_INTERVAL: Duration = Duration::from_secs(2);
const UPDATE_EVERY: u64 = 5_000_000;

#[derive(Default)]
pub struct Counters {
    pub sub_records: u64,
    pub segments: u64,
    pub objects: u64,
    pub classes: u64,
    pub roots: u64,
    pub edges: u64,
}

pub struct PhaseProgress {
    bar: ProgressBar,
    phase: String,
    started: Instant,
    last_update: Instant,
    pub counters: Counters,
    quiet: bool,
}

impl PhaseProgress {
    pub fn new(phase: impl Into<String>) -> Self {
        Self::new_inner(phase, false)
    }

    pub fn quiet() -> Self {
        Self::new_inner("", true)
    }

    fn new_inner(phase: impl Into<String>, quiet: bool) -> Self {
        let phase = phase.into();
        if quiet {
            return Self {
                bar: ProgressBar::hidden(),
                phase,
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
        bar.set_message(format!("{} …", phase));

        Self {
            bar,
            phase,
            started: Instant::now(),
            last_update: Instant::now(),
            counters: Counters::default(),
            quiet: false,
        }
    }

    pub fn tick_sub_record(&mut self) {
        self.counters.sub_records += 1;
        self.maybe_refresh();
    }

    pub fn tick_segment(&mut self) {
        self.counters.segments += 1;
    }

    pub fn add_object(&mut self) {
        self.counters.objects += 1;
        self.maybe_refresh();
    }

    pub fn add_class(&mut self) {
        self.counters.classes += 1;
    }

    pub fn add_root(&mut self) {
        self.counters.roots += 1;
    }

    pub fn add_edges(&mut self, n: u64) {
        self.counters.edges += n;
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
        let c = &self.counters;
        let elapsed = self.started.elapsed();
        let secs = elapsed.as_secs_f64().max(0.001);
        let sub_rate = c.sub_records as f64 / secs;

        let mut parts = vec![self.phase.clone()];
        if c.segments > 0 {
            parts.push(format!("{} segs", format_count(c.segments)));
        }
        if c.sub_records > 0 {
            parts.push(format!("{} subs", format_count(c.sub_records)));
            parts.push(format!("{:.0}/s", sub_rate));
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
        }
        parts.push(format!("{elapsed:.1?}"));

        self.bar.set_message(parts.join(" | "));
    }

    pub fn finish(&self, summary: impl Into<String>) {
        if self.quiet {
            return;
        }
        self.bar.finish_with_message(summary.into());
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
