//! Build operating and tax timelines from financial data.
//!
//! Ported from `validator-accounting/src/html_report.rs` (build_timeline,
//! build_tax_timeline) and `tax_report.rs` (build_tax_rows).

use chrono::{Datelike, NaiveDate};

use super::config::ValidatorConfig;
use super::types::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map "unknown" → a sentinel that sorts before all real ISO dates.
fn sort_date(d: &str) -> &str {
    if d == "unknown" { "0000-00-00" } else { d }
}

/// Stable ordering within the same date: revenue first, then expenses, then balance-sheet.
fn type_order(event_type: &str) -> u8 {
    match event_type {
        // Operating timeline
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
        // Tax timeline
        "tax_revenue" => 0,
        "tax_return_capital" => 1,
        "tax_reimbursement" => 2,
        "tax_expense_vote_fees" => 3,
        "tax_expense_doublezero" => 4,
        "tax_expense_hosting" => 5,
        "tax_expense_software" => 6,
        "tax_expense_contractor" => 7,
        "tax_expense_hardware" => 8,
        "tax_expense_other" => 9,
        _ => 10,
    }
}

const FALLBACK_DATE: &str = "2025-12-15";

/// Walk forward through sorted events, accumulating running totals.
fn accumulate(events: &mut [TimelineEvent]) {
    let mut cum_profit = 0.0_f64;
    let mut cum_revenue = 0.0_f64;
    let mut cum_expenses = 0.0_f64;

    for ev in events.iter_mut() {
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
}

// ── Recurring expense expansion ───────────────────────────────────────────────

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Expand recurring expenses into individual monthly entries.
pub fn expand_recurring_expenses(
    recurring: &[RecurringExpense],
    start_month: &str, // YYYY-MM
    end_month: &str,   // YYYY-MM
) -> Vec<Expense> {
    let mut expenses = Vec::new();

    let start = NaiveDate::parse_from_str(&format!("{}-01", start_month), "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    let end = NaiveDate::parse_from_str(&format!("{}-01", end_month), "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());

    for rec in recurring {
        let rec_start = NaiveDate::parse_from_str(&rec.start_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        let rec_end = rec
            .end_date
            .as_ref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());

        let billing_day = NaiveDate::parse_from_str(&rec.start_date, "%Y-%m-%d")
            .map(|d| d.day())
            .unwrap_or(1);

        let mut current = start;
        while current <= end {
            let rec_start_month = NaiveDate::from_ymd_opt(rec_start.year(), rec_start.month(), 1).unwrap();

            if current >= rec_start_month {
                let within_end = rec_end.is_none_or(|end_date| {
                    let end_month_start = NaiveDate::from_ymd_opt(end_date.year(), end_date.month(), 1).unwrap();
                    current <= end_month_start
                });

                if within_end {
                    let dim = days_in_month(current.year(), current.month());
                    let actual_day = billing_day.min(dim);
                    let expense_date = NaiveDate::from_ymd_opt(current.year(), current.month(), actual_day).unwrap();

                    expenses.push(Expense {
                        date: expense_date.format("%Y-%m-%d").to_string(),
                        vendor: rec.vendor.clone(),
                        category: rec.category,
                        description: rec.description.clone(),
                        amount_usd: rec.amount_usd,
                        paid_with: rec.paid_with.clone(),
                        invoice_id: None,
                    });
                }
            }

            current = if current.month() == 12 {
                NaiveDate::from_ymd_opt(current.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(current.year(), current.month() + 1, 1).unwrap()
            };
        }
    }

    expenses
}

// ══════════════════════════════════════════════════════════════════════════════
// OPERATING TIMELINE
// ══════════════════════════════════════════════════════════════════════════════

/// Build the operating P/L timeline from all data sources.
pub fn build_timeline(data: &ReportData) -> Vec<TimelineEvent> {
    let mut events: Vec<TimelineEvent> = Vec::new();

    // ── Commission rewards ──────────────────────────────────────────────
    for reward in data.rewards {
        let date = reward.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = reward.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(reward.epoch),
            event_type: "commission",
            label: "Staking commission".into(),
            sublabel: Some(format!("Epoch {}", reward.epoch)),
            amount_sol: reward.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Leader fees ─────────────────────────────────────────────────────
    for fees in data.leader_fees {
        let date = fees.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = fees.total_fees_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(fees.epoch),
            event_type: "leader_fees",
            label: "Leader fees".into(),
            sublabel: Some(format!("Epoch {} \u{00b7} {} blocks", fees.epoch, fees.blocks_produced)),
            amount_sol: fees.total_fees_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── MEV claims ──────────────────────────────────────────────────────
    if data.mev_claims.is_empty() {
        for transfer in &data.categorized.mev_deposits {
            let date = transfer.date.clone().unwrap_or_else(|| "unknown".into());
            let price = get_price(data.prices, &date);
            let usd = transfer.amount_sol * price;
            events.push(TimelineEvent {
                date,
                epoch: None,
                event_type: "mev",
                label: "MEV tips (Jito)".into(),
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
            let date = claim.date.clone().unwrap_or_else(|| "unknown".into());
            let price = get_price(data.prices, &date);
            let usd = claim.amount_sol * price;
            events.push(TimelineEvent {
                date,
                epoch: Some(claim.epoch),
                event_type: "mev",
                label: "MEV tips (Jito)".into(),
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

    // ── BAM claims ──────────────────────────────────────────────────────
    for claim in data.bam_claims {
        let date = claim.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = claim.amount_sol_equivalent * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(claim.epoch),
            event_type: "bam",
            label: "BAM incentives (Jito)".into(),
            sublabel: Some(format!("Epoch {} \u{00b7} jitoSOL reward", claim.epoch)),
            amount_sol: claim.amount_sol_equivalent,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Vote costs (net of SFDP) ────────────────────────────────────────
    for cost in data.vote_costs {
        let date = cost.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let gross_usd = cost.total_fee_sol * price;

        let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::parse_from_str(FALLBACK_DATE, "%Y-%m-%d").unwrap());

        let coverage = data
            .sfdp_acceptance_date
            .as_ref()
            .map(|sfdp_date| sfdp_coverage_percent(sfdp_date, &parsed))
            .unwrap_or(0.0);

        let net_usd = gross_usd * (1.0 - coverage);
        let net_sol = cost.total_fee_sol * (1.0 - coverage);

        let sublabel = if coverage > 0.0 {
            Some(format!(
                "Epoch {} \u{00b7} SFDP {:.0}% offset",
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
            label: "Vote costs".into(),
            sublabel,
            amount_sol: -net_sol,
            amount_usd: -net_usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── DoubleZero fees ─────────────────────────────────────────────────
    for fee in data.doublezero_fees {
        let date = fee.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = fee.liability_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: Some(fee.epoch),
            event_type: "doublezero",
            label: "DoubleZero fees".into(),
            sublabel: Some(format!("Epoch {}", fee.epoch)),
            amount_sol: -fee.liability_sol,
            amount_usd: -usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Off-chain expenses ──────────────────────────────────────────────
    for expense in data.expenses {
        events.push(TimelineEvent {
            date: expense.date.clone(),
            epoch: None,
            event_type: "expense",
            label: format!("{} \u{2014} {}", expense.vendor, expense.category),
            sublabel: Some(expense.description.clone()),
            amount_sol: 0.0,
            amount_usd: -expense.amount_usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: true,
        });
    }

    // ── Balance-sheet: seeding ───────────────────────────────────────────
    for transfer in &data.categorized.seeding {
        let date = transfer.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = transfer.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: None,
            event_type: "seeding",
            label: "Capital contribution".into(),
            sublabel: Some(format!("{} \u{2192} {}", transfer.from_label, transfer.to_label)),
            amount_sol: transfer.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: false,
        });
    }

    // ── Balance-sheet: withdrawals ───────────────────────────────────────
    for transfer in &data.categorized.withdrawals {
        let date = transfer.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = transfer.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: None,
            event_type: "withdrawal",
            label: "Withdrawal".into(),
            sublabel: Some(format!("\u{2192} {}", transfer.to_label)),
            amount_sol: transfer.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: false,
        });
    }

    // ── Balance-sheet: DoubleZero prepayments ────────────────────────────
    for transfer in &data.categorized.doublezero_payments {
        let date = transfer.date.clone().unwrap_or_else(|| "unknown".into());
        let price = get_price(data.prices, &date);
        let usd = transfer.amount_sol * price;
        events.push(TimelineEvent {
            date,
            epoch: None,
            event_type: "doublezero_payment",
            label: "DoubleZero prepayment".into(),
            sublabel: Some("Deposit to DoubleZero PDA".into()),
            amount_sol: transfer.amount_sol,
            amount_usd: usd,
            cumulative_profit_usd: 0.0,
            cumulative_revenue_usd: 0.0,
            cumulative_expenses_usd: 0.0,
            is_pnl: false,
        });
    }

    // ── Sort & accumulate ───────────────────────────────────────────────
    events.sort_by(|a, b| {
        sort_date(&a.date)
            .cmp(sort_date(&b.date))
            .then_with(|| type_order(a.event_type).cmp(&type_order(b.event_type)))
    });

    accumulate(&mut events);
    events
}

// ══════════════════════════════════════════════════════════════════════════════
// TAX TIMELINE
// ══════════════════════════════════════════════════════════════════════════════

/// SFDP coverage percent (standalone, doesn't need ValidatorConfig).
fn sfdp_coverage_percent(acceptance_str: &str, date: &NaiveDate) -> f64 {
    let Ok(acceptance) = NaiveDate::parse_from_str(acceptance_str, "%Y-%m-%d") else {
        return 0.0;
    };
    let months_diff = (date.year() - acceptance.year()) * 12 + (date.month() as i32 - acceptance.month() as i32);

    if months_diff < 0 {
        0.0
    } else if months_diff < 3 {
        1.0
    } else if months_diff < 6 {
        0.75
    } else if months_diff < 9 {
        0.50
    } else if months_diff < 12 {
        0.25
    } else {
        0.0
    }
}

fn shorten_pubkey(addr: &str) -> String {
    if addr.len() > 12 {
        format!("{}...{}", &addr[..6], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

fn parse_epoch_from_description(description: &str) -> Option<u64> {
    let marker = "epoch ";
    let lower = description.to_lowercase();
    let idx = lower.find(marker)?;
    let digits = lower[idx + marker.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u64>().ok()
}

fn tax_event_type(row: &TaxRow) -> &'static str {
    match row.entry_type.as_str() {
        "Revenue" => "tax_revenue",
        "Reimbursement" => "tax_reimbursement",
        "Return of Capital" => "tax_return_capital",
        "Expense" => {
            let category = row.category.to_lowercase();
            match category.as_str() {
                "vote fees" => "tax_expense_vote_fees",
                "doublezero" => "tax_expense_doublezero",
                "hosting" => "tax_expense_hosting",
                "software" => "tax_expense_software",
                "contractor" => "tax_expense_contractor",
                "hardware" => "tax_expense_hardware",
                _ => "tax_expense_other",
            }
        }
        _ => "tax_expense_other",
    }
}

fn tax_label_and_sublabel(row: &TaxRow, event_type: &str) -> (String, Option<String>) {
    match event_type {
        "tax_revenue" => ("Taxable withdrawal".into(), Some(row.description.clone())),
        "tax_reimbursement" => ("SFDP reimbursement".into(), Some(row.description.clone())),
        "tax_return_capital" => ("Return of capital".into(), Some(row.description.clone())),
        "tax_expense_vote_fees" => ("Vote fees".into(), Some(row.description.clone())),
        "tax_expense_doublezero" => ("DoubleZero fees".into(), Some(row.description.clone())),
        _ if row.entry_type == "Expense" => {
            let parts: Vec<&str> = row.description.splitn(2, " - ").collect();
            if parts.len() == 2 {
                (
                    format!("{} \u{2014} {}", parts[0].trim(), row.category),
                    Some(parts[1].trim().to_string()),
                )
            } else {
                (format!("{} expense", row.category), Some(row.description.clone()))
            }
        }
        _ => (row.category.clone(), Some(row.description.clone())),
    }
}

fn signed_tax_amounts(row: &TaxRow, event_type: &str) -> (f64, f64, bool) {
    let sol = row.sol_amount.unwrap_or(0.0);
    let usd = row.usd_value;

    match event_type {
        "tax_revenue" | "tax_reimbursement" => (sol, usd, true),
        "tax_return_capital" => (sol, usd, false),
        "tax_expense_vote_fees"
        | "tax_expense_doublezero"
        | "tax_expense_hosting"
        | "tax_expense_software"
        | "tax_expense_contractor"
        | "tax_expense_hardware"
        | "tax_expense_other" => (-sol, -usd, true),
        _ => (0.0, 0.0, false),
    }
}

/// Build tax rows from financial data (ported from tax_report.rs).
fn build_tax_rows(data: &ReportData, config: &ValidatorConfig) -> Vec<TaxRow> {
    let mut rows = Vec::new();

    // ── Revenue: withdrawals offset by seeding capital ──────────────────
    let mut all_outgoing: Vec<&SolTransfer> = data.categorized.withdrawals.iter().collect();
    // Include outgoing "other" transfers to external addresses
    for t in &data.categorized.other {
        if config.is_our_account(&t.from_address) && !config.is_our_account(&t.to_address) {
            all_outgoing.push(t);
        }
    }
    let total_seeded_sol: f64 = data.categorized.seeding.iter().map(|s| s.amount_sol).sum();
    add_withdrawal_rows(&mut rows, &all_outgoing, data.prices, total_seeded_sol);

    // ── Expenses: vote fees (net of SFDP) ───────────────────────────────
    add_vote_cost_rows(
        &mut rows,
        data.vote_costs,
        data.prices,
        data.sfdp_acceptance_date.as_deref(),
    );

    // ── Expenses: DoubleZero ────────────────────────────────────────────
    add_doublezero_rows(&mut rows, data.doublezero_fees, data.prices);

    // ── Expenses: off-chain ─────────────────────────────────────────────
    add_offchain_expense_rows(&mut rows, data.expenses);

    rows.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| b.entry_type.cmp(&a.entry_type)));

    rows
}

fn add_withdrawal_rows(rows: &mut Vec<TaxRow>, withdrawals: &[&SolTransfer], prices: &PriceMap, total_seeded_sol: f64) {
    let mut sorted: Vec<&&SolTransfer> = withdrawals.iter().collect();
    sorted.sort_by(|a, b| a.date.cmp(&b.date));

    let mut remaining_capital = total_seeded_sol;

    for w in sorted {
        let date = w.date.as_deref().unwrap_or("unknown");
        let capital_portion = w.amount_sol.min(remaining_capital);
        let revenue_portion = w.amount_sol - capital_portion;
        remaining_capital -= capital_portion;

        let price = get_price(prices, date);
        let dest_label = if w.to_label.is_empty() {
            shorten_pubkey(&w.to_address)
        } else {
            w.to_label.clone()
        };

        if capital_portion > 0.0 {
            rows.push(TaxRow {
                date: date.to_string(),
                entry_type: "Return of Capital".into(),
                category: "Withdrawal".into(),
                description: format!("Return of seed capital to {}", dest_label),
                sol_amount: Some(capital_portion),
                sol_price_usd: Some(price),
                usd_value: capital_portion * price,
                destination: dest_label.clone(),
                tx_signature: w.signature.clone(),
            });
        }

        if revenue_portion > 0.0 {
            rows.push(TaxRow {
                date: date.to_string(),
                entry_type: "Revenue".into(),
                category: "Withdrawal".into(),
                description: format!("External withdrawal to {}", dest_label),
                sol_amount: Some(revenue_portion),
                sol_price_usd: Some(price),
                usd_value: revenue_portion * price,
                destination: dest_label,
                tx_signature: w.signature.clone(),
            });
        }
    }
}

fn add_vote_cost_rows(
    rows: &mut Vec<TaxRow>,
    vote_costs: &[EpochVoteCost],
    prices: &PriceMap,
    sfdp_acceptance_date: Option<&str>,
) {
    for vc in vote_costs {
        let date = vc.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let gross_usd = vc.total_fee_sol * price;

        let coverage = sfdp_acceptance_date
            .and_then(|sfdp| {
                NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .ok()
                    .map(|d| sfdp_coverage_percent(sfdp, &d))
            })
            .unwrap_or(0.0);

        let reimbursed_sol = vc.total_fee_sol * coverage;
        let reimbursed_usd = reimbursed_sol * price;

        let description = if coverage > 0.0 {
            format!(
                "Vote transaction fees epoch {} ({} votes, {:.0}% SFDP-reimbursed)",
                vc.epoch,
                vc.vote_count,
                coverage * 100.0
            )
        } else {
            format!("Vote transaction fees epoch {} ({} votes)", vc.epoch, vc.vote_count)
        };

        rows.push(TaxRow {
            date: date.to_string(),
            entry_type: "Expense".into(),
            category: "Vote Fees".into(),
            description,
            sol_amount: Some(vc.total_fee_sol),
            sol_price_usd: Some(price),
            usd_value: gross_usd,
            destination: String::new(),
            tx_signature: String::new(),
        });

        if reimbursed_sol > 0.0 {
            rows.push(TaxRow {
                date: date.to_string(),
                entry_type: "Reimbursement".into(),
                category: "SFDP Vote Fee Reimbursement".into(),
                description: format!(
                    "SFDP reimbursement epoch {} ({:.0}% coverage)",
                    vc.epoch,
                    coverage * 100.0
                ),
                sol_amount: Some(reimbursed_sol),
                sol_price_usd: Some(price),
                usd_value: reimbursed_usd,
                destination: String::new(),
                tx_signature: String::new(),
            });
        }
    }
}

fn add_doublezero_rows(rows: &mut Vec<TaxRow>, fees: &[DoubleZeroFee], prices: &PriceMap) {
    for fee in fees {
        let date = fee.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = fee.liability_sol * price;

        rows.push(TaxRow {
            date: date.to_string(),
            entry_type: "Expense".into(),
            category: "DoubleZero".into(),
            description: format!(
                "DoubleZero network fee epoch {} ({}bps on leader fees)",
                fee.epoch, fee.fee_rate_bps
            ),
            sol_amount: Some(fee.liability_sol),
            sol_price_usd: Some(price),
            usd_value,
            destination: String::new(),
            tx_signature: String::new(),
        });
    }
}

fn add_offchain_expense_rows(rows: &mut Vec<TaxRow>, expenses: &[Expense]) {
    for exp in expenses {
        rows.push(TaxRow {
            date: exp.date.clone(),
            entry_type: "Expense".into(),
            category: exp.category.to_string(),
            description: format!("{} - {}", exp.vendor, exp.description),
            sol_amount: None,
            sol_price_usd: None,
            usd_value: exp.amount_usd,
            destination: String::new(),
            tx_signature: String::new(),
        });
    }
}

/// Build the tax-basis timeline from financial data.
pub fn build_tax_timeline(data: &ReportData, config: &ValidatorConfig) -> Vec<TimelineEvent> {
    let rows = build_tax_rows(data, config);

    let mut events: Vec<TimelineEvent> = rows
        .into_iter()
        .map(|row| {
            let event_type = tax_event_type(&row);
            let (label, sublabel) = tax_label_and_sublabel(&row, event_type);
            let (amount_sol, amount_usd, is_pnl) = signed_tax_amounts(&row, event_type);

            TimelineEvent {
                epoch: parse_epoch_from_description(&row.description),
                date: row.date,
                event_type,
                label,
                sublabel,
                amount_sol,
                amount_usd,
                cumulative_profit_usd: 0.0,
                cumulative_revenue_usd: 0.0,
                cumulative_expenses_usd: 0.0,
                is_pnl,
            }
        })
        .collect();

    events.sort_by(|a, b| {
        sort_date(&a.date)
            .cmp(sort_date(&b.date))
            .then_with(|| type_order(a.event_type).cmp(&type_order(b.event_type)))
    });

    accumulate(&mut events);
    events
}
