//! Background scheduler that runs the data ingestion job periodically.
//! Uses a simple tokio::time::interval — no external cron dependency needed.

#[cfg(feature = "ssr")]
mod ssr {
    use crate::ingestion;
    use std::time::Duration;
    use tokio::process::Command;

    const DEFAULT_INTERVAL_HOURS: u64 = 6;
    const DEFAULT_REFRESH_FINANCIALS: bool = true;

    /// Spawn the background ingestion scheduler.
    /// Runs immediately on startup, then every `interval_hours` hours.
    pub fn spawn_scheduler() {
        let interval_hours = std::env::var("INGESTION_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INTERVAL_HOURS);
        let refresh_financials = parse_bool_env("FINANCIALS_REFRESH_ENABLED").unwrap_or(DEFAULT_REFRESH_FINANCIALS);

        println!(
            "[scheduler] Starting background ingestion every {} hours",
            interval_hours
        );
        println!(
            "[scheduler] Financial cache refresh is {}",
            if refresh_financials { "enabled" } else { "disabled" }
        );

        tokio::spawn(async move {
            // Run immediately on startup
            run_once(refresh_financials).await;

            // Then loop on the interval
            let mut interval = tokio::time::interval(Duration::from_secs(interval_hours * 3600));
            interval.tick().await; // skip the first (immediate) tick
            loop {
                interval.tick().await;
                run_once(refresh_financials).await;
            }
        });
    }

    async fn run_once(refresh_financials: bool) {
        match ingestion::run_ingestion().await {
            Ok(true) => println!("[scheduler] Ingestion completed successfully"),
            Ok(false) => eprintln!("[scheduler] Ingestion skipped (no data available)"),
            Err(e) => eprintln!("[scheduler] Ingestion failed: {}", e),
        }

        if refresh_financials {
            if let Err(e) = refresh_financial_cache().await {
                eprintln!("[scheduler] Financial refresh failed: {}", e);
            } else {
                println!("[scheduler] Financial cache refresh completed successfully");
            }
        }
    }

    async fn refresh_financial_cache() -> Result<(), String> {
        let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "/data".to_string());
        let data_dir = data_dir.trim_end_matches('/').to_string();
        let config_path = format!("{}/config.toml", data_dir);
        let output_dir = format!("{}/output", data_dir);

        let output = Command::new("/app/validator-accounting")
            .arg("--config")
            .arg(&config_path)
            .arg("--data-dir")
            .arg(&data_dir)
            .arg("--output-dir")
            .arg(&output_dir)
            .output()
            .await
            .map_err(|e| format!("failed to spawn validator-accounting: {}", e))?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "exit status {} (stderr: {}; stdout: {})",
            output.status,
            truncate_for_log(&stderr),
            truncate_for_log(&stdout)
        ))
    }

    fn parse_bool_env(name: &str) -> Option<bool> {
        let raw = std::env::var(name).ok()?;
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    fn truncate_for_log(s: &str) -> String {
        const MAX_CHARS: usize = 500;
        if s.len() <= MAX_CHARS {
            return s.trim().to_string();
        }
        format!("{}...[truncated]", s[..MAX_CHARS].trim())
    }
}

#[cfg(feature = "ssr")]
pub use ssr::*;
