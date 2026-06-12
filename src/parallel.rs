//! Thread-pool configuration for the parallel analysis phases.
//!
//! The heavier phases (graph construction, retained-size accumulation, class
//! aggregation, and `--explain-class`) use [Rayon](https://docs.rs/rayon). This
//! module sets the size of Rayon's global pool once, up front.

/// Configure the global Rayon thread pool before any parallel work runs.
///
/// When `jobs` is `None`, Rayon's default (the number of logical CPUs, or the
/// `RAYON_NUM_THREADS` environment variable) is left in place. When it is
/// `Some(n)`, the global pool is sized to `n` threads.
///
/// Call this at most once, before any parallel iterator runs.
///
/// # Errors
///
/// Returns an `Err(String)` if `jobs` is `Some(0)`, or if the global pool has
/// already been initialized.
pub fn configure(jobs: Option<usize>) -> Result<(), String> {
    if let Some(n) = jobs {
        if n == 0 {
            return Err("--jobs must be at least 1".to_string());
        }
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .map_err(|e| format!("failed to configure thread pool: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_jobs() {
        assert!(configure(Some(0)).is_err());
    }
}
