//! HTML financial report generator
//!
//! Produces a self-contained `report.html` alongside the CSV files — a banking-style
//! scrollable timeline where a sticky header shows Net P/L, Revenue, and Expenses,
//! and those numbers "rewind" to what they were at any point in history as the user scrolls.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use crate::constants;
use crate::prices::get_price;
use crate::reports::ReportData;

/// One atomic financial event in the timeline.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub date: String,
    pub epoch: Option<u64>,
    pub event_type: &'static str,
    pub label: String,
    pub sublabel: Option<String>,
    pub amount_sol: f64,
    pub amount_usd: f64,
    /// Running total AFTER this event (chronological order, pre-computed)
    pub cumulative_profit_usd: f64,
    pub cumulative_revenue_usd: f64,
    pub cumulative_expenses_usd: f64,
    /// false for seeding/withdrawals (balance-sheet only; don't affect P/L)
    pub is_pnl: bool,
}

/// Map "unknown" to a sentinel that sorts before all real ISO dates.
fn sort_date(d: &str) -> &str {
    if d == "unknown" { "0000-00-00" } else { d }
}

/// Sort key for stable ordering within the same date:
/// income sources first, then expenses, then balance-sheet items.
fn type_order(event_type: &str) -> u8 {
    match event_type {
        "commission" => 0,
        "leader_fees" => 1,
        "mev" => 2,
        "bam" => 3,
        "vote_cost" => 4,
        "doublezero" => 5,
        "expense" => 6,
        "seeding" => 7,
        "withdrawal" => 8,
        "doublezero_payment" => 9,
        _ => 10,
    }
}

/// Flatten all data sources into a timeline and compute running totals.
pub fn build_timeline(data: &ReportData) -> Vec<TimelineEvent> {
    let mut events: Vec<TimelineEvent> = Vec::new();

    // ── Commission rewards ─────────────────────────────────────────────────
    for reward in data.rewards {
        let date = reward.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = reward.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(reward.epoch),
            event_type: "commission",
            label: "Staking commission".to_string(),
            sublabel: Some(format!("Epoch {}", reward.epoch)),
            amount_sol: reward.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Leader fees ────────────────────────────────────────────────────────
    for fees in data.leader_fees {
        let date = fees.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = fees.total_fees_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(fees.epoch),
            event_type: "leader_fees",
            label: "Leader fees".to_string(),
            sublabel: Some(format!("Epoch {} · {} blocks", fees.epoch, fees.blocks_produced)),
            amount_sol: fees.total_fees_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── MEV claims ─────────────────────────────────────────────────────────
    // Use Jito API claims as source of truth; fall back to transfer detection.
    if data.mev_claims.is_empty() {
        for transfer in &data.categorized.mev_deposits {
            let date = transfer.date.clone().unwrap_or_else(|| "unknown".to_string());
            let price = get_price(data.prices, &date);
            let usd = transfer.amount_sol * price;
            events.push(TimelineEvent {
                date,
                epoch: None,
                event_type: "mev",
                label: "MEV tips (Jito)".to_string(),
                sublabel: None,
                amount_sol: transfer.amount_sol,
                amount_usd: usd,
                cumulative_profit_usd: 0.0,
                cumulative_revenue_usd: 0.0,
                cumulative_expenses_usd: 0.0,
                is_pnl: true,
            });
        }
    } else {
        for claim in data.mev_claims {
            let date = claim.date.clone().unwrap_or_else(|| "unknown".to_string());
            let price = get_price(data.prices, &date);
            let usd = claim.amount_sol * price;
            events.push(TimelineEvent {
                date,
                epoch: Some(claim.epoch),
                event_type: "mev",
                label: "MEV tips (Jito)".to_string(),
                sublabel: Some(format!("Epoch {}", claim.epoch)),
                amount_sol: claim.amount_sol,
                amount_usd: usd,
                cumulative_profit_usd: 0.0,
                cumulative_revenue_usd: 0.0,
                cumulative_expenses_usd: 0.0,
                is_pnl: true,
            });
        }
    }

    // ── BAM claims ─────────────────────────────────────────────────────────
    for claim in data.bam_claims {
        let date = claim.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = claim.amount_sol_equivalent * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(claim.epoch),
            event_type: "bam",
            label: "BAM incentives (Jito)".to_string(),
            sublabel: Some(format!("Epoch {} · jitoSOL reward", claim.epoch)),
            amount_sol: claim.amount_sol_equivalent,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Vote costs ─────────────────────────────────────────────────────────
    for cost in data.vote_costs {
        let date = cost.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let gross_usd = cost.total_fee_sol * price;

        let parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .unwrap_or_else(|_| {
                chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap()
            });
        let coverage = data.config.sfdp_coverage_percent(&parsed);
        let net_usd = gross_usd * (1.0 - coverage);
        let net_sol = cost.total_fee_sol * (1.0 - coverage);

        let sublabel = if coverage > 0.0 {
            Some(format!(
                "Epoch {} · SFDP {:.0}% offset",
                cost.epoch,
                coverage * 100.0
            ))
        } else {
            Some(format!("Epoch {}", cost.epoch))
        };

        events.push(TimelineEvent {
            date,
            epoch: Some(cost.epoch),
            event_type: "vote_cost",
            label: "Vote costs".to_string(),
            sublabel,
            amount_sol: -net_sol,
            amount_usd: -net_usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── DoubleZero fees ────────────────────────────────────────────────────
    for fee in data.doublezero_fees {
        let date = fee.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = fee.liability_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(fee.epoch),
            event_type: "doublezero",
            label: "DoubleZero fees".to_string(),
            sublabel: Some(format!("Epoch {}", fee.epoch)),
            amount_sol: -fee.liability_sol,
            amount_usd: -usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Off-chain expenses ─────────────────────────────────────────────────
    for expense in data.expenses {
        events.push(TimelineEvent {
            date: expense.date.clone(),
            epoch: None,
            event_type: "expense",
            label: format!("{} — {}", expense.vendor, expense.category),
            sublabel: Some(expense.description.clone()),
            amount_sol: 0.0,
            amount_usd: -expense.amount_usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Balance-sheet: seeding ─────────────────────────────────────────────
    for transfer in &data.categorized.seeding {
        let date = transfer.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = transfer.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: None,
            event_type: "seeding",
            label: "Capital contribution".to_string(),
            sublabel: Some(format!("{} → {}", transfer.from_label, transfer.to_label)),
            amount_sol: transfer.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: false,
        });
    }

    // ── Balance-sheet: withdrawals ─────────────────────────────────────────
    for transfer in &data.categorized.withdrawals {
        let date = transfer.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = transfer.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: None,
            event_type: "withdrawal",
            label: "Withdrawal".to_string(),
            sublabel: Some(format!("→ {}", transfer.to_label)),
            amount_sol: transfer.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: false,
        });
    }

    // ── Balance-sheet: DoubleZero prepayments ─────────────────────────────
    for transfer in &data.categorized.doublezero_payments {
        let date = transfer.date.clone().unwrap_or_else(|| "unknown".to_string());
        let price = get_price(data.prices, &date);
        let usd = transfer.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: None,
            event_type: "doublezero_payment",
            label: "DoubleZero prepayment".to_string(),
            sublabel: Some("Deposit to DoubleZero PDA".to_string()),
            amount_sol: transfer.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: false,
        });
    }

    // ── Sort: ascending date, stable type order within same date ───────────
    // "unknown" dates sort before all real ISO dates so they appear at the
    // beginning of the timeline rather than floating to the end.
    events.sort_by(|a, b| {
        sort_date(&a.date)
            .cmp(sort_date(&b.date))
            .then_with(|| type_order(a.event_type).cmp(&type_order(b.event_type)))
    });

    // ── Walk forward accumulating running totals ───────────────────────────
    let mut cum_profit = 0.0_f64;
    let mut cum_revenue = 0.0_f64;
    let mut cum_expenses = 0.0_f64;

    for ev in &mut events {
        if ev.is_pnl {
            if ev.amount_usd >= 0.0 {
                cum_revenue += ev.amount_usd;
            } else {
                cum_expenses += ev.amount_usd.abs();
            }
            cum_profit += ev.amount_usd;
        }
        ev.cumulative_profit_usd = cum_profit;
        ev.cumulative_revenue_usd = cum_revenue;
        ev.cumulative_expenses_usd = cum_expenses;
    }

    events
}

/// Write a self-contained `report.html` to `output_dir`.
pub fn generate_html_report(output_dir: &Path, data: &ReportData) -> Result<()> {
    let timeline = build_timeline(data);
    let timeline_json = serde_json::to_string(&timeline)?;

    // Prevent "</script>" in string values (vendor names, descriptions, labels)
    // from closing the inline <script> block prematurely.
    // Escaping the forward slash (\/) is valid JSON and parsed transparently.
    let timeline_json = timeline_json.replace("</", r"<\/");

    let html = build_html(&timeline_json);
    let path = output_dir.join("report.html");
    std::fs::write(&path, html)?;
    println!("  Generated: {}", path.display());
    Ok(())
}

fn build_html(timeline_json: &str) -> String {
    // The HTML template is a raw string literal embedded at compile time.
    // The JSON data is injected at a single marker so the template stays readable.
    let template = include_str!("html_report_template.html");
    template.replacen("__TIMELINE_JSON__", timeline_json, 1)
}
