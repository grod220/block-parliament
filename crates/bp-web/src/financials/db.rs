//! Read-only queries against `cache.sqlite` (populated by validator-accounting).
//!
//! Opens the database lazily on the first `/financials` request.
//! Uses `?mode=ro` for read-only safety — we never write to this database.

use anyhow::{Context, Result};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};
use std::sync::OnceLock;

use super::types::*;

static CACHE_POOL: OnceLock<SqlitePool> = OnceLock::new();

/// Initialize the read-only cache.sqlite pool.
/// Safe to call multiple times — only the first call connects.
pub async fn init_cache(data_dir: &str) -> Result<&'static SqlitePool> {
    if let Some(pool) = CACHE_POOL.get() {
        return Ok(pool);
    }

    let db_path = format!("{}/cache.sqlite", data_dir);
    let url = format!("sqlite:{}?mode=ro", db_path);

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
        .with_context(|| format!("Failed to open cache.sqlite at {}", db_path))?;

    // Ignore if already set (race between concurrent requests)
    let _ = CACHE_POOL.set(pool);
    Ok(CACHE_POOL.get().unwrap())
}

// ── Query functions ───────────────────────────────────────────────────────────

pub async fn get_epoch_rewards(pool: &SqlitePool) -> Result<Vec<EpochReward>> {
    let rows = sqlx::query("SELECT epoch, amount_sol, commission, date FROM epoch_rewards ORDER BY epoch")
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| EpochReward {
            epoch: r.get::<i64, _>("epoch") as u64,
            amount_sol: r.get("amount_sol"),
            commission: r.get::<i64, _>("commission") as u8,
            date: r.get("date"),
        })
        .collect())
}

pub async fn get_leader_fees(pool: &SqlitePool) -> Result<Vec<EpochLeaderFees>> {
    let rows = sqlx::query(
        "SELECT epoch, total_fees_sol, blocks_produced, skipped_slots, date
         FROM leader_fees ORDER BY epoch",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| EpochLeaderFees {
            epoch: r.get::<i64, _>("epoch") as u64,
            total_fees_sol: r.get("total_fees_sol"),
            blocks_produced: r.get::<i64, _>("blocks_produced") as u64,
            skipped_slots: r.get::<i64, _>("skipped_slots") as u64,
            date: r.get("date"),
        })
        .collect())
}

pub async fn get_mev_claims(pool: &SqlitePool) -> Result<Vec<MevClaim>> {
    let rows = sqlx::query(
        "SELECT epoch, amount_sol, total_tips_lamports, commission_lamports, date
         FROM mev_claims ORDER BY epoch",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| MevClaim {
            epoch: r.get::<i64, _>("epoch") as u64,
            amount_sol: r.get("amount_sol"),
            total_tips_lamports: r.get::<i64, _>("total_tips_lamports") as u64,
            commission_lamports: r.get::<i64, _>("commission_lamports") as u64,
            date: r.get("date"),
        })
        .collect())
}

pub async fn get_bam_claims(pool: &SqlitePool) -> Result<Vec<BamClaim>> {
    let rows = sqlx::query(
        "SELECT epoch, amount_sol_equivalent, amount_jitosol_lamports,
                jitosol_sol_rate, tx_signature, date
         FROM bam_claims ORDER BY epoch",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| BamClaim {
            epoch: r.get::<i64, _>("epoch") as u64,
            amount_sol_equivalent: r.get("amount_sol_equivalent"),
            amount_jitosol_lamports: r.get::<i64, _>("amount_jitosol_lamports") as u64,
            jitosol_sol_rate: r.get("jitosol_sol_rate"),
            tx_signature: r.get("tx_signature"),
            date: r.get("date"),
        })
        .collect())
}

pub async fn get_vote_costs(pool: &SqlitePool) -> Result<Vec<EpochVoteCost>> {
    let rows = sqlx::query(
        "SELECT epoch, vote_count, total_fee_sol, source, date
         FROM vote_costs ORDER BY epoch",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| EpochVoteCost {
            epoch: r.get::<i64, _>("epoch") as u64,
            vote_count: r.get::<i64, _>("vote_count") as u64,
            total_fee_sol: r.get("total_fee_sol"),
            source: r.get("source"),
            date: r.get("date"),
        })
        .collect())
}

pub async fn get_doublezero_fees(pool: &SqlitePool) -> Result<Vec<DoubleZeroFee>> {
    let rows = sqlx::query(
        "SELECT epoch, liability_sol, fee_base_lamports, fee_rate_bps, date, is_estimate
         FROM doublezero_fees ORDER BY epoch",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| DoubleZeroFee {
            epoch: r.get::<i64, _>("epoch") as u64,
            liability_sol: r.get("liability_sol"),
            fee_base_lamports: r.get::<i64, _>("fee_base_lamports") as u64,
            fee_rate_bps: r.get::<i64, _>("fee_rate_bps") as u64,
            date: r.get("date"),
            is_estimate: r.get::<i64, _>("is_estimate") != 0,
        })
        .collect())
}

pub async fn get_expenses(pool: &SqlitePool) -> Result<Vec<Expense>> {
    let rows = sqlx::query(
        "SELECT date, vendor, category, description, amount_usd, paid_with, invoice_id
         FROM expenses ORDER BY date",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| Expense {
            date: r.get("date"),
            vendor: r.get("vendor"),
            category: ExpenseCategory::from_str_lossy(r.get::<&str, _>("category")),
            description: r.get("description"),
            amount_usd: r.get("amount_usd"),
            paid_with: r.get("paid_with"),
            invoice_id: r.get("invoice_id"),
        })
        .collect())
}

pub async fn get_recurring_expenses(pool: &SqlitePool) -> Result<Vec<RecurringExpense>> {
    let rows = sqlx::query(
        "SELECT vendor, category, description, amount_usd, paid_with, start_date, end_date
         FROM recurring_expenses ORDER BY start_date",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| RecurringExpense {
            vendor: r.get("vendor"),
            category: ExpenseCategory::from_str_lossy(r.get::<&str, _>("category")),
            description: r.get("description"),
            amount_usd: r.get("amount_usd"),
            paid_with: r.get("paid_with"),
            start_date: r.get("start_date"),
            end_date: r.get("end_date"),
        })
        .collect())
}

pub async fn get_prices(pool: &SqlitePool) -> Result<PriceMap> {
    let rows = sqlx::query("SELECT date, usd_price FROM prices")
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let date: String = r.get("date");
            let price: f64 = r.get("usd_price");
            (date, price)
        })
        .collect())
}

pub async fn get_sol_transfers(pool: &SqlitePool) -> Result<Vec<SolTransfer>> {
    let rows = sqlx::query(
        "SELECT signature, date, from_address, to_address, amount_sol, from_label, to_label
         FROM sol_transfers ORDER BY slot",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| SolTransfer {
            signature: r.get("signature"),
            date: r.get("date"),
            from_address: r.get("from_address"),
            to_address: r.get("to_address"),
            amount_sol: r.get("amount_sol"),
            from_label: r.get("from_label"),
            to_label: r.get("to_label"),
        })
        .collect())
}
