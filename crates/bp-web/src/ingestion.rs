//! Data ingestion: fetches external API data and writes metrics snapshots to SQLite.
//! Called by the scheduler (daily) or manually via CLI.

#[cfg(feature = "ssr")]
mod ssr {
    use crate::api::{get_jito_mev_history, get_network_comparison, get_sfdp_status, get_validator_data};
    use crate::components::metrics::MetricsData;
    use crate::db;

    /// Run one ingestion cycle: fetch all APIs, write snapshot to DB.
    /// Returns Ok(true) if data was written, Ok(false) if no data available.
    pub async fn run_ingestion() -> Result<bool, Box<dyn std::error::Error>> {
        println!("[ingestion] Starting metrics fetch...");

        // Fetch Stakewiz data first (required for other calculations)
        let Some(validator) = get_validator_data().await else {
            eprintln!("[ingestion] Failed to fetch Stakewiz validator data — skipping this cycle");
            return Ok(false);
        };

        println!(
            "[ingestion] Stakewiz OK: rank #{}, stake {:.0} SOL, APY {:.2}%",
            validator.rank, validator.activated_stake, validator.total_apy
        );

        // Fetch remaining data in parallel — each can fail independently
        let (mev_result, sfdp_result, network_result) = futures::join!(
            get_jito_mev_history(5),
            get_sfdp_status(),
            get_network_comparison(validator.skip_rate, validator.activated_stake),
        );

        if mev_result.is_some() {
            println!("[ingestion] Jito MEV OK");
        } else {
            eprintln!("[ingestion] Jito MEV fetch failed (non-fatal)");
        }
        if sfdp_result.is_some() {
            println!("[ingestion] SFDP OK");
        } else {
            eprintln!("[ingestion] SFDP fetch failed (non-fatal)");
        }
        if network_result.is_some() {
            println!("[ingestion] Network comparison OK");
        } else {
            eprintln!("[ingestion] Network comparison fetch failed (non-fatal)");
        }

        let data = MetricsData {
            validator,
            mev_history: mev_result,
            network_comp: network_result,
            sfdp_status: sfdp_result,
        };

        let json = serde_json::to_string(&data)?;
        db::save_metrics_snapshot(&json).await.map_err(|e| {
            eprintln!("[ingestion] Failed to save snapshot: {}", e);
            e
        })?;

        let now = chrono::Utc::now().to_rfc3339();
        db::set_metadata("last_ingestion", &now).await.ok();

        println!("[ingestion] Snapshot saved at {}", now);
        Ok(true)
    }
}

#[cfg(feature = "ssr")]
pub use ssr::*;
