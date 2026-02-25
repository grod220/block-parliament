//! Dynamic financial report generation for `/financials`.
//!
//! Queries `cache.sqlite` at request time, builds operating + tax timelines,
//! and injects them into the self-contained HTML template.

pub mod categorize;
pub mod config;
pub mod db;
pub mod timeline;
pub mod types;

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};

use self::config::ValidatorConfig;
use self::types::*;

/// The HTML template with `__TIMELINE_JSON__`, `__TAX_TIMELINE_JSON__`,
/// and `__TAX_YEAR__` placeholders (embedded at compile time).
static TEMPLATE: &str = include_str!("template.html");

/// Fallback HTML when cache.sqlite doesn't exist yet.
static FALLBACK: &str = concat!(
    "<!DOCTYPE html><html><body style='font-family:monospace;padding:2em'>",
    "<h1>Financial data not yet available</h1>",
    "<p>The cache database has not been populated yet. ",
    "Run <code>validator-accounting</code> to fetch financial data.</p>",
    "</body></html>"
);

/// Generate the full HTML report dynamically from cache.sqlite.
///
/// Returns the rendered HTML string or the fallback if the DB isn't available.
pub async fn generate_report(data_dir: &str) -> String {
    match try_generate(data_dir).await {
        Ok(html) => html,
        Err(e) => {
            eprintln!("[financials] Error generating report: {:#}", e);
            FALLBACK.to_string()
        }
    }
}

fn within_actual_window(date: &str, cutoff: NaiveDate, today: NaiveDate) -> bool {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| d >= cutoff && d <= today)
        .unwrap_or(false)
}

fn month_key_from_date(date: &str) -> Option<String> {
    if date.len() >= 7 && date.as_bytes()[4] == b'-' {
        Some(date[..7].to_string())
    } else {
        None
    }
}

async fn try_generate(data_dir: &str) -> Result<String> {
    // ── Load config ─────────────────────────────────────────────────────
    let config_path = std::path::Path::new(data_dir).join("config.toml");
    let config = ValidatorConfig::load(&config_path)?;

    // ── Open cache.sqlite (read-only) ───────────────────────────────────
    let pool = db::init_cache(data_dir).await?;

    // ── Fetch all data concurrently ─────────────────────────────────────
    let (
        mut rewards,
        mut leader_fees,
        mut mev_claims,
        mut bam_claims,
        mut vote_costs,
        mut doublezero_fees,
        mut one_time_expenses,
        recurring_expenses,
        prices,
        mut transfers,
    ) = tokio::try_join!(
        db::get_epoch_rewards(pool),
        db::get_leader_fees(pool),
        db::get_mev_claims(pool),
        db::get_bam_claims(pool),
        db::get_vote_costs(pool),
        db::get_doublezero_fees(pool),
        db::get_expenses(pool),
        db::get_recurring_expenses(pool),
        db::get_prices(pool),
        db::get_sol_transfers(pool),
    )
    .context("Failed to query cache.sqlite")?;

    // ── Enforce business start cutoff (first day of bootstrap month) ───
    let cutoff = config.business_start_date();
    let today = Utc::now().date_naive();
    rewards.retain(|r| r.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));
    leader_fees.retain(|f| f.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));
    mev_claims.retain(|m| m.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));
    bam_claims.retain(|b| b.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));
    vote_costs.retain(|v| v.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));
    doublezero_fees.retain(|f| f.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));
    one_time_expenses.retain(|e| within_actual_window(&e.date, cutoff, today));
    transfers.retain(|t| t.date.as_deref().is_some_and(|d| within_actual_window(d, cutoff, today)));

    // ── Expand recurring expenses ───────────────────────────────────────
    let mut all_expenses = one_time_expenses;

    if !recurring_expenses.is_empty() {
        // Match validator-accounting behavior:
        // prefer reward date range, else derive from recurring rules.
        let reward_months: Vec<String> = rewards
            .iter()
            .filter_map(|r| r.date.as_deref())
            .filter_map(month_key_from_date)
            .collect();

        let bootstrap_month = config.business_start_month();
        let mut start_month = reward_months.iter().min().cloned();
        let mut end_month = reward_months.iter().max().cloned();

        if start_month.is_none() || end_month.is_none() {
            let current_month = today.format("%Y-%m").to_string();
            start_month = recurring_expenses
                .iter()
                .filter_map(|r| month_key_from_date(&r.start_date))
                .min();
            end_month = recurring_expenses
                .iter()
                .filter_map(|r| {
                    r.end_date
                        .as_deref()
                        .and_then(month_key_from_date)
                        .or_else(|| Some(current_month.clone()))
                })
                .max();
        }

        if let (Some(mut start_month), Some(mut end_month)) = (start_month, end_month) {
            if start_month < bootstrap_month {
                start_month = bootstrap_month;
            }
            if end_month < start_month {
                end_month = start_month.clone();
            }
            let expanded = timeline::expand_recurring_expenses(&recurring_expenses, &start_month, &end_month);
            all_expenses.extend(expanded);
        }
    }

    // Guardrail: recurring expansion can create entries later in the current month.
    all_expenses.retain(|e| within_actual_window(&e.date, cutoff, today));

    // ── Categorize transfers ────────────────────────────────────────────
    let categorized = categorize::categorize_transfers(&transfers, &config);

    // ── Build report data bundle ────────────────────────────────────────
    let report_data = ReportData {
        rewards: &rewards,
        categorized: &categorized,
        mev_claims: &mev_claims,
        bam_claims: &bam_claims,
        leader_fees: &leader_fees,
        doublezero_fees: &doublezero_fees,
        vote_costs: &vote_costs,
        expenses: &all_expenses,
        prices: &prices,
        sfdp_acceptance_date: config.sfdp_acceptance_date.clone(),
    };

    // ── Build timelines ─────────────────────────────────────────────────
    let operating = timeline::build_timeline(&report_data);
    let tax = timeline::build_tax_timeline(&report_data, &config);

    // ── Serialize & inject into template ────────────────────────────────
    let timeline_json = serde_json::to_string(&operating)?;
    let tax_timeline_json = serde_json::to_string(&tax)?;

    // Escape "</script>" inside JSON strings to prevent premature script close
    let timeline_json = timeline_json.replace("</", r"<\/");
    let tax_timeline_json = tax_timeline_json.replace("</", r"<\/");

    let html = TEMPLATE
        .replacen("__TIMELINE_JSON__", &timeline_json, 1)
        .replacen("__TAX_TIMELINE_JSON__", &tax_timeline_json, 1)
        .replacen("__TAX_YEAR__", "null", 1);

    Ok(html)
}
