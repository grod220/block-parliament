//! Background scheduler that runs the data ingestion job periodically.
//! Uses a simple tokio::time::interval — no external cron dependency needed.

#[cfg(feature = "ssr")]
mod ssr {
    use crate::ingestion;
    use std::time::Duration;

    const DEFAULT_INTERVAL_HOURS: u64 = 6;

    /// Spawn the background ingestion scheduler.
    /// Runs immediately on startup, then every `interval_hours` hours.
    pub fn spawn_scheduler() {
        let interval_hours = std::env::var("INGESTION_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INTERVAL_HOURS);

        println!(
            "[scheduler] Starting background ingestion every {} hours",
            interval_hours
        );

        tokio::spawn(async move {
            // Run immediately on startup
            run_once().await;

            // Then loop on the interval
            let mut interval = tokio::time::interval(Duration::from_secs(interval_hours * 3600));
            interval.tick().await; // skip the first (immediate) tick
            loop {
                interval.tick().await;
                run_once().await;
            }
        });
    }

    async fn run_once() {
        match ingestion::run_ingestion().await {
            Ok(true) => println!("[scheduler] Ingestion completed successfully"),
            Ok(false) => eprintln!("[scheduler] Ingestion skipped (no data available)"),
            Err(e) => eprintln!("[scheduler] Ingestion failed: {}", e),
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr::*;
