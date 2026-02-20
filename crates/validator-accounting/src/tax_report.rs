//! Withdrawal-based tax report generation
//!
//! Generates a tax report where:
//! - **Revenue** = all outgoing external transfers from validator business
//!   accounts (to exchanges, personal wallet, or any non-internal address),
//!   valued at SOL/USD price on withdrawal date
//! - **Expenses** = period costs (vote fees, DoubleZero, hosting, etc.),
//!   deductible in the period incurred
//! - **Internal transfers** (vote account ↔ identity) are ignored
//!
//! This is a parallel, non-destructive feature that does not modify existing reports.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use csv::Writer;
use std::path::Path;

use crate::config::Config;
use crate::doublezero::DoubleZeroFee;
use crate::expenses::Expense;
use crate::prices::{PriceCache, get_price};
use crate::transactions::{CategorizedTransfers, SolTransfer};
use crate::vote_costs::EpochVoteCost;

/// Tax report output filename
const TAX_REPORT_FILENAME: &str = "tax_report.csv";

/// All data needed to generate the tax report.
pub struct TaxReportData<'a> {
    pub config: &'a Config,
    pub categorized: &'a CategorizedTransfers,
    pub doublezero_fees: &'a [DoubleZeroFee],
    pub vote_costs: &'a [EpochVoteCost],
    pub expenses: &'a [Expense],
    pub prices: &'a PriceCache,
}

/// A single row in the tax report CSV.
struct TaxRow {
    date: String,
    entry_type: String, // "Revenue", "Expense", "Return of Capital", or "Reimbursement"
    category: String,   // e.g. "Withdrawal", "Vote Fees", "DoubleZero", "Hosting"
    description: String,
    sol_amount: Option<f64>,
    sol_price_usd: Option<f64>,
    usd_value: f64,
    destination: String,  // for withdrawals
    tx_signature: String, // for on-chain events
}

/// Generate the tax report CSV and print a console summary.
pub fn generate_tax_report(output_dir: &Path, data: &TaxReportData, year_filter: Option<i32>) -> Result<()> {
    let mut rows = Vec::new();
    let mut skipped_unknown_dates: usize = 0;

    // ── Revenue: all outgoing external transfers (offset by seeding capital)
    // Combine known withdrawals + outgoing "other" transfers (to unknown addresses)
    let mut all_outgoing: Vec<&SolTransfer> = data.categorized.withdrawals.iter().collect();
    for t in &data.categorized.other {
        if data.config.is_our_account(&t.from) && !data.config.is_our_account(&t.to) {
            all_outgoing.push(t);
        }
    }
    let total_seeded_sol: f64 = data.categorized.seeding.iter().map(|s| s.amount_sol).sum();
    add_withdrawal_rows(
        &mut rows,
        &all_outgoing,
        data.prices,
        year_filter,
        &mut skipped_unknown_dates,
        total_seeded_sol,
    );

    // ── Expenses: vote fees (SOL burned on-chain, net of SFDP) ─────────
    add_vote_cost_rows(
        &mut rows,
        data.vote_costs,
        data.prices,
        data.config,
        year_filter,
        &mut skipped_unknown_dates,
    );

    // ── Expenses: DoubleZero payments (SOL to third-party PDA) ─────────
    add_doublezero_rows(
        &mut rows,
        data.doublezero_fees,
        data.prices,
        year_filter,
        &mut skipped_unknown_dates,
    );

    // ── Expenses: off-chain costs (hosting, contractors, hardware, etc.)
    add_offchain_expense_rows(&mut rows, data.expenses, year_filter, &mut skipped_unknown_dates);

    // Sort all rows by date, then revenue before expenses
    rows.sort_by(|a, b| {
        a.date.cmp(&b.date).then_with(|| b.entry_type.cmp(&a.entry_type)) // "Revenue" > "Expense" → revenue first
    });

    // Write CSV
    let path = output_dir.join(TAX_REPORT_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    wtr.write_record([
        "Date",
        "Type",
        "Category",
        "Description",
        "SOL Amount",
        "SOL Price (USD)",
        "USD Value",
        "Destination",
        "Tx Signature",
    ])?;

    for row in &rows {
        wtr.write_record([
            &row.date,
            &row.entry_type,
            &row.category,
            &row.description,
            &row.sol_amount.map_or(String::new(), |v| format!("{:.6}", v)),
            &row.sol_price_usd.map_or(String::new(), |v| format!("{:.2}", v)),
            &format!("{:.2}", row.usd_value),
            &row.destination,
            &row.tx_signature,
        ])?;
    }

    wtr.flush()?;

    // Console summary
    print_tax_summary(&rows, year_filter);

    if skipped_unknown_dates > 0 {
        eprintln!(
            "\n  ⚠ Warning: {} row(s) with unknown/unparseable dates were excluded from the report.",
            skipped_unknown_dates
        );
    }

    println!("\nTax report written to: {}", path.display());

    Ok(())
}

// ─── Row builders ──────────────────────────────────────────────────────────

fn add_withdrawal_rows(
    rows: &mut Vec<TaxRow>,
    withdrawals: &[&SolTransfer],
    prices: &PriceCache,
    year_filter: Option<i32>,
    skipped: &mut usize,
    total_seeded_sol: f64,
) {
    // Sort withdrawals chronologically so capital is consumed in order.
    // ISO-8601 string sort is correct for YYYY-MM-DD; "unknown" sorts after
    // all real dates, so unknown-dated entries consume capital last (safest).
    let mut sorted: Vec<&&SolTransfer> = withdrawals.iter().collect();
    sorted.sort_by(|a, b| a.date.cmp(&b.date));

    let mut remaining_capital = total_seeded_sol;

    for w in sorted {
        let date = w.date.as_deref().unwrap_or("unknown");

        // Always consume capital regardless of year filter — prior-year
        // withdrawals must reduce the pool so the current year is correct.
        let capital_portion = w.amount_sol.min(remaining_capital);
        let revenue_portion = w.amount_sol - capital_portion;
        remaining_capital -= capital_portion;

        // Only emit rows for the requested year
        if !matches_year(date, year_filter, skipped) {
            continue;
        }
        let price = get_price(prices, date);

        let dest_label = if w.to_label.is_empty() {
            shorten_pubkey(&w.to.to_string())
        } else {
            w.to_label.clone()
        };

        if capital_portion > 0.0 {
            rows.push(TaxRow {
                date: date.to_string(),
                entry_type: "Return of Capital".to_string(),
                category: "Withdrawal".to_string(),
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
                entry_type: "Revenue".to_string(),
                category: "Withdrawal".to_string(),
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
    prices: &PriceCache,
    config: &Config,
    year_filter: Option<i32>,
    skipped: &mut usize,
) {
    for vc in vote_costs {
        let date = vc.date.as_deref().unwrap_or("unknown");
        if !matches_year(date, year_filter, skipped) {
            continue;
        }
        let price = get_price(prices, date);
        let gross_usd = vc.total_fee_sol * price;

        // Calculate SFDP coverage for this epoch
        let coverage = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|d| config.sfdp_coverage_percent(&d))
            .unwrap_or(0.0);
        let reimbursed_sol = vc.total_fee_sol * coverage;
        let reimbursed_usd = reimbursed_sol * price;

        // Always show gross vote fee as an expense
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
            entry_type: "Expense".to_string(),
            category: "Vote Fees".to_string(),
            description,
            sol_amount: Some(vc.total_fee_sol),
            sol_price_usd: Some(price),
            usd_value: gross_usd,
            destination: String::new(),
            tx_signature: String::new(),
        });

        // SFDP reimbursement portion (offsets the expense above)
        if reimbursed_sol > 0.0 {
            rows.push(TaxRow {
                date: date.to_string(),
                entry_type: "Reimbursement".to_string(),
                category: "SFDP Vote Fee Reimbursement".to_string(),
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

fn add_doublezero_rows(
    rows: &mut Vec<TaxRow>,
    fees: &[DoubleZeroFee],
    prices: &PriceCache,
    year_filter: Option<i32>,
    skipped: &mut usize,
) {
    for fee in fees {
        let date = fee.date.as_deref().unwrap_or("unknown");
        if !matches_year(date, year_filter, skipped) {
            continue;
        }
        let price = get_price(prices, date);
        let usd_value = fee.liability_sol * price;

        rows.push(TaxRow {
            date: date.to_string(),
            entry_type: "Expense".to_string(),
            category: "DoubleZero".to_string(),
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

fn add_offchain_expense_rows(
    rows: &mut Vec<TaxRow>,
    expenses: &[Expense],
    year_filter: Option<i32>,
    skipped: &mut usize,
) {
    for exp in expenses {
        if !matches_year(&exp.date, year_filter, skipped) {
            continue;
        }

        rows.push(TaxRow {
            date: exp.date.clone(),
            entry_type: "Expense".to_string(),
            category: exp.category.to_string(),
            description: format!("{} - {}", exp.vendor, exp.description),
            sol_amount: None, // off-chain expenses are already in USD
            sol_price_usd: None,
            usd_value: exp.amount_usd,
            destination: String::new(),
            tx_signature: String::new(),
        });
    }
}

// ─── Console summary ──────────────────────────────────────────────────────

fn print_tax_summary(rows: &[TaxRow], year_filter: Option<i32>) {
    let year_label = year_filter.map(|y| format!(" ({})", y)).unwrap_or_default();

    println!("\n══════════════════════════════════════════════════");
    println!("  TAX REPORT SUMMARY{}", year_label);
    println!("══════════════════════════════════════════════════");

    // Revenue
    let revenue_rows: Vec<&TaxRow> = rows.iter().filter(|r| r.entry_type == "Revenue").collect();
    let total_revenue_usd: f64 = revenue_rows.iter().map(|r| r.usd_value).sum();
    let total_revenue_sol: f64 = revenue_rows.iter().filter_map(|r| r.sol_amount).sum();

    println!("\n  REVENUE (External Withdrawals)");
    println!("  ─────────────────────────────────────────────");

    // Return of capital
    let roc_rows: Vec<&TaxRow> = rows.iter().filter(|r| r.entry_type == "Return of Capital").collect();
    let total_roc_sol: f64 = roc_rows.iter().filter_map(|r| r.sol_amount).sum();
    let total_roc_usd: f64 = roc_rows.iter().map(|r| r.usd_value).sum();
    if !roc_rows.is_empty() {
        println!(
            "    Return of capital:  {:.6} SOL = ${:.2} (non-taxable)",
            total_roc_sol, total_roc_usd
        );
    }

    println!(
        "    Taxable revenue:   {} withdrawal(s): {:.6} SOL = ${:.2}",
        revenue_rows.len(),
        total_revenue_sol.abs(),
        total_revenue_usd.abs()
    );

    // Reimbursements
    let reimb_rows: Vec<&TaxRow> = rows.iter().filter(|r| r.entry_type == "Reimbursement").collect();
    let total_reimb_usd: f64 = reimb_rows.iter().map(|r| r.usd_value).sum();
    let total_reimb_sol: f64 = reimb_rows.iter().filter_map(|r| r.sol_amount).sum();
    if !reimb_rows.is_empty() {
        println!("\n  REIMBURSEMENTS (SFDP)");
        println!("  ─────────────────────────────────────────────");
        println!(
            "    SFDP:              {} entries  {:.6} SOL = ${:.2}",
            reimb_rows.len(),
            total_reimb_sol,
            total_reimb_usd
        );
    }

    // Expenses by category
    let expense_rows: Vec<&TaxRow> = rows.iter().filter(|r| r.entry_type == "Expense").collect();
    let total_expense_usd: f64 = expense_rows.iter().map(|r| r.usd_value).sum();

    println!("\n  EXPENSES (Period Costs)");
    println!("  ─────────────────────────────────────────────");

    // Group by category
    let mut categories: Vec<String> = expense_rows.iter().map(|r| r.category.clone()).collect();
    categories.sort();
    categories.dedup();

    for cat in &categories {
        let cat_rows: Vec<&&TaxRow> = expense_rows.iter().filter(|r| r.category == *cat).collect();
        let cat_total: f64 = cat_rows.iter().map(|r| r.usd_value).sum();
        let cat_sol: f64 = cat_rows.iter().filter_map(|r| r.sol_amount).sum();

        if cat_sol > 0.0 {
            println!(
                "    {:<20} {:>3} entries  {:.6} SOL = ${:.2}",
                cat,
                cat_rows.len(),
                cat_sol,
                cat_total
            );
        } else {
            println!(
                "    {:<20} {:>3} entries              ${:.2}",
                cat,
                cat_rows.len(),
                cat_total
            );
        }
    }
    println!("  ─────────────────────────────────────────────");
    println!("    {:<20}              Total: ${:.2}", "", total_expense_usd);

    // Net = Revenue - (Gross Expenses - Reimbursements)
    //      = Revenue + Reimbursements - Expenses
    // Reimbursements offset gross expenses (e.g. SFDP covers vote fees),
    // so adding them back gives the true out-of-pocket expense burden.
    let net = total_revenue_usd + total_reimb_usd - total_expense_usd;
    println!("\n  ═════════════════════════════════════════════");
    println!("  NET TAXABLE INCOME:                ${:.2}", net);
    println!("  ═════════════════════════════════════════════");
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn matches_year(date: &str, year_filter: Option<i32>, skipped: &mut usize) -> bool {
    // Warn about unparseable dates regardless of year filter
    if date == "unknown" || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        *skipped += 1;
        // If no year filter, still include the row (fallback price will be used)
        return year_filter.is_none();
    }
    let Some(year) = year_filter else {
        return true;
    };
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| d.year() == year)
        .unwrap_or(false)
}

fn shorten_pubkey(addr: &str) -> String {
    if addr.len() > 12 {
        format!("{}...{}", &addr[..6], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}
