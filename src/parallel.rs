/// Configure the global Rayon thread pool before any parallel work runs.
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
