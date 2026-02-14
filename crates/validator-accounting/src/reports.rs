//! Report generation (CSV outputs and console summary)

use anyhow::Result;
use csv::Writer;
use std::collections::HashMap;
use std::path::Path;

use crate::bam::BamClaim;
use crate::config::Config;
use crate::constants;
use crate::doublezero::DoubleZeroFee;
use crate::expenses::{Expense, ExpenseCategory};
use crate::jito::MevClaim;
use crate::leader_fees::EpochLeaderFees;
use crate::prices::{PriceCache, get_price};
use crate::transactions::{CategorizedTransfers, EpochReward};
use crate::vote_costs::EpochVoteCost;

/// Bundled report data to reduce function argument counts
pub struct ReportData<'a> {
    pub rewards: &'a [EpochReward],
    pub categorized: &'a CategorizedTransfers,
    pub mev_claims: &'a [MevClaim],
    pub bam_claims: &'a [BamClaim],
    pub leader_fees: &'a [EpochLeaderFees],
    pub doublezero_fees: &'a [DoubleZeroFee],
    pub vote_costs: &'a [EpochVoteCost],
    pub expenses: &'a [Expense],
    pub prices: &'a PriceCache,
    pub config: &'a Config,
}

/// Generate all CSV reports
pub fn generate_all_reports(output_dir: &Path, data: &ReportData, year_filter: Option<i32>) -> Result<()> {
    generate_income_ledger(
        output_dir,
        data.rewards,
        data.categorized,
        data.mev_claims,
        data.bam_claims,
        data.leader_fees,
        data.prices,
    )?;
    generate_expense_ledger(
        output_dir,
        data.expenses,
        data.vote_costs,
        data.doublezero_fees,
        data.prices,
        data.config,
    )?;
    generate_treasury_ledger(output_dir, data.categorized, data.prices)?;
    generate_summary(output_dir, data, year_filter)?;
    generate_glossary(output_dir)?;

    // Older versions generated a separate glossary/data-dictionary CSV. Remove it to
    // avoid accidentally sharing stale context alongside the ledgers.
    let _ = std::fs::remove_file(output_dir.join("report_context.csv"));

    Ok(())
}

/// Generate glossary.csv (accountant-oriented data dictionary)
fn generate_glossary(output_dir: &Path) -> Result<()> {
    let path = output_dir.join(constants::GLOSSARY_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    wtr.write_record([
        "field",
        "display_name",
        "type",
        "unit",
        "short_definition",
        "why_it_matters",
        "source_of_truth",
        "notes_for_accountant",
    ])?;

    let mut row = |field: &str,
                   display_name: &str,
                   ty: &str,
                   unit: &str,
                   short_definition: &str,
                   why_it_matters: &str,
                   source_of_truth: &str,
                   notes_for_accountant: &str|
     -> Result<()> {
        wtr.write_record([
            field,
            display_name,
            ty,
            unit,
            short_definition,
            why_it_matters,
            source_of_truth,
            notes_for_accountant,
        ])?;
        Ok(())
    };

    // Core revenue
    row(
        "commission_sol",
        "Staking commission",
        "revenue",
        "SOL",
        "Validator's share of staking rewards (commission) from delegated stake.",
        "Primary validator service revenue stream.",
        "Solana protocol inflation rewards (RPC `getInflationReward` via vote account; cached locally).",
        "Recognize per your revenue policy. Report also provides USD valuation using a daily SOL price; confirm pricing methodology required for tax reporting.",
    )?;
    row(
        "commission_usd",
        "Staking commission (USD valuation)",
        "revenue",
        "USD",
        "USD valuation of staking commission at the selected daily SOL price.",
        "Used for USD books/tax reporting when SOL-denominated income is received.",
        "Computed by this tool from commission SOL and daily SOL USD price (CoinGecko, cached).",
        "Confirm pricing policy (daily close vs spot at receipt time; UTC vs local). Use a consistent method across all crypto activity.",
    )?;
    row(
        "leader_fees_sol",
        "Leader fees",
        "revenue",
        "SOL",
        "Transaction fees earned when the validator produces blocks (is the leader).",
        "Often the largest non-staking revenue component; varies with network activity.",
        "Solana on-chain block/fee data (derived leader slot fees; cached locally; may be imported/backfilled).",
        "Represents fees earned by block production. May include base and priority fees depending on data source and network rules.",
    )?;
    row(
        "leader_fees_usd",
        "Leader fees (USD valuation)",
        "revenue",
        "USD",
        "USD valuation of leader fees at the selected daily SOL price.",
        "Used for USD books/tax reporting of SOL-denominated fee income.",
        "Computed by this tool from leader fees SOL and daily SOL USD price (CoinGecko, cached).",
        "Confirm pricing policy and ensure it matches how other SOL income is valued.",
    )?;
    row(
        "mev_tips_sol",
        "MEV tips (Jito)",
        "revenue",
        "SOL",
        "Optional tips paid via Jito distribution (often for transaction priority/ordering).",
        "Can be meaningful and volatile; may need separate disclosure/classification.",
        "Jito claim API (primary) and/or on-chain receipts (fallback).",
        "MEV tips are separate from standard Solana transaction fees. If you prefer, classify separately from leader fees.",
    )?;
    row(
        "mev_tips_usd",
        "MEV tips (Jito, USD valuation)",
        "revenue",
        "USD",
        "USD valuation of MEV tips at the selected daily SOL price.",
        "Used for USD books/tax reporting of MEV-related income.",
        "Computed by this tool from MEV tips SOL and daily SOL USD price (CoinGecko, cached).",
        "If you use a different pricing source for tax lots, you may want to revalue these externally and treat the report as a reconciliation aid.",
    )?;
    row(
        "bam_sol",
        "BAM incentives (Jito)",
        "revenue",
        "SOL-equivalent",
        "Validator incentive rewards paid in jitoSOL (a liquid staking token) valued at a SOL-equivalent amount for reporting.",
        "Additional validator revenue; different asset form (token) may affect tracking/valuation.",
        "Jito BAM claim history + on-chain token receipt; valued using configured jitoSOL-to-SOL rate and SOL USD price.",
        "These rows are paid in jitoSOL; the report converts to SOL-equivalent for valuation. For tax, you may need fair market value at receipt and asset-specific lots.",
    )?;
    row(
        "bam_usd",
        "BAM incentives (Jito, USD valuation)",
        "revenue",
        "USD",
        "USD valuation of BAM incentives at the selected daily SOL price (after converting to SOL-equivalent).",
        "Used for USD books/tax reporting of token-denominated income.",
        "Computed by this tool from BAM SOL-equivalent and daily SOL USD price (CoinGecko, cached).",
        "Because BAM is paid in jitoSOL, confirm whether you need the jitoSOL spot USD price at receipt instead of a SOL-equivalent proxy.",
    )?;

    // Pricing/valuation mechanics used throughout the CSVs
    row(
        "usd_price",
        "SOL USD price (daily)",
        "assumption",
        "USD per SOL",
        "Daily SOL USD price used to value SOL-denominated amounts.",
        "Drives USD revenue/expense totals and tax reporting values if you rely on this output.",
        "CoinGecko daily price (UTC), cached locally; falls back to a fixed price if API fails.",
        "If you must use a different pricing policy (spot at receipt time, different provider, local timezone), revalue externally and use this report for SOL-denominated quantities and traceability.",
    )?;
    row(
        "usd_value",
        "USD value (valuation)",
        "derived",
        "USD",
        "USD valuation of a SOL-denominated amount using the tool's daily SOL USD price.",
        "Provides a consistent USD view for bookkeeping.",
        "Computed by this tool: Amount_SOL * USD_Price (or equivalent).",
        "This is a valuation, not necessarily cash received/spent in USD.",
    )?;

    // Core operating expenses
    row(
        "vote_costs_sol",
        "Vote costs",
        "expense",
        "SOL",
        "On-chain transaction fees paid to submit validator vote transactions.",
        "Core operating cost required to participate in consensus and earn rewards.",
        "Vote fee dataset (import/backfill/estimate) + on-chain fee economics; cached locally.",
        "These are on-chain network fees. The report also provides gross USD valuation and net USD after SFDP coverage.",
    )?;
    row(
        "vote_costs_gross_usd",
        "Vote costs (gross, USD valuation)",
        "expense",
        "USD",
        "Gross USD valuation of vote transaction fees before SFDP coverage is applied.",
        "Supports an expense view in USD and an explicit SFDP offset calculation.",
        "Computed by this tool from vote costs SOL and daily SOL USD price (CoinGecko, cached).",
        "Gross vs net is driven by the SFDP coverage schedule modeled by this tool.",
    )?;
    row(
        "vote_costs_net_usd",
        "Vote costs (net after SFDP, USD valuation)",
        "expense",
        "USD",
        "Net USD vote fee cost after applying SFDP coverage percent.",
        "Represents out-of-pocket vote fee expense after modeled program reimbursement.",
        "Computed by this tool: Vote_Costs_Gross_USD * (1 - SFDP_Coverage).",
        "This is a modeled net. Actual SFDP transfers may differ; consider reconciling to actual receipts if needed for your books/tax approach.",
    )?;
    row(
        "sfdp_coverage_percent",
        "SFDP coverage percent",
        "assumption",
        "percent",
        "The percent of vote transaction fees covered/reimbursed under Solana Foundation Delegation Program (SFDP).",
        "Directly reduces out-of-pocket vote fee expense.",
        "Config setting (`validator.sfdp_acceptance_date`) + coverage schedule implemented by this tool.",
        "This tool treats SFDP as a contra-expense (offset to vote fees). You may prefer alternative presentation; decide and apply consistently.",
    )?;
    row(
        "sfdp_offset_usd",
        "SFDP vote reimbursement",
        "contra-expense",
        "USD",
        "Portion of vote costs treated as covered by SFDP (gross vote costs minus net vote costs).",
        "Reduces out-of-pocket vote expense; affects operating profit.",
        "Computed: Vote_Costs_Gross_USD - Vote_Costs_Net_USD (per month in summary).",
        "Depending on your accounting/tax policy, this could be presented as contra-expense or as other income. This report currently models it as contra-expense.",
    )?;
    row(
        "sfdp_reimbursements_actual",
        "SFDP reimbursements (actual transfers)",
        "info",
        "SOL/USD",
        "Actual on-chain transfers received from a Solana Foundation SFDP reimbursement address.",
        "Useful to reconcile modeled SFDP offsets to actual receipts and to support audit trail.",
        "On-chain receipts (incoming SOL transfers from known SFDP reimbursement wallet).",
        "This tool currently models SFDP as a coverage schedule and does not output a dedicated SFDP receipts ledger. If you want, we can add `sfdp_ledger.csv` or include these transfers in `treasury_ledger.csv` with clear labeling.",
    )?;

    // DoubleZero: accrued vs paid vs outstanding
    row(
        "doublezero_fees_usd",
        "DoubleZero fees (accrued)",
        "expense",
        "USD",
        "Networking service fees incurred (accrued) related to DoubleZero program.",
        "Infrastructure cost supporting validator performance; may be material.",
        "Derived accrual from configured fee rate applied to relevant on-chain activity; cached locally.",
        "This tool tracks accrued fees separate from payments. If you receive invoices/contract statements, use them as source-of-truth and reconcile.",
    )?;
    row(
        "doublezero_fees_sol",
        "DoubleZero fees (accrued, SOL)",
        "expense",
        "SOL",
        "Accrued DoubleZero fee amount denominated in SOL.",
        "Supports reconciliation and audit trail back to SOL quantities.",
        "Derived by this tool from configured fee rules; cached locally.",
        "If you book DoubleZero on an invoice basis, you may ignore this accrual and treat it as an estimate/reconciliation aid.",
    )?;
    row(
        "doublezero_paid_usd",
        "DoubleZero paid",
        "cash_movement",
        "USD",
        "Payments made to DoubleZero during the period.",
        "Used for cash reconciliation; not necessarily equal to incurred expense in the month.",
        "On-chain transfers categorized as DoubleZero deposits/prepayments.",
        "Payments can be prepayments; expense should follow your accrual policy. Reconcile wallet outflows to this line.",
    )?;
    row(
        "doublezero_paid_sol",
        "DoubleZero paid (SOL)",
        "cash_movement",
        "SOL",
        "SOL-denominated amount transferred as a DoubleZero deposit/prepayment.",
        "Used for on-chain reconciliation to wallet activity.",
        "On-chain transfers categorized as DoubleZero deposits/prepayments.",
        "Not necessarily equal to incurred expense in the month.",
    )?;
    row(
        "doublezero_ap_usd",
        "DoubleZero outstanding (A/P)",
        "balance_tracking",
        "USD",
        "Accrued minus paid balance (liability) owed to DoubleZero.",
        "Supports reconciliation and period-to-period roll-forward.",
        "Computed: DoubleZero_Fees_USD - DoubleZero_Paid_USD (monthly/annual in summary).",
        "Treat as payable/contra-prepayment depending on how DoubleZero billing works for you.",
    )?;
    row(
        "doublezero_outstanding_sol",
        "DoubleZero outstanding (SOL)",
        "balance_tracking",
        "SOL",
        "Accrued minus paid DoubleZero amount in SOL.",
        "Supports reconciliation when liabilities are tracked in SOL.",
        "Computed by this tool from accrued SOL and paid SOL.",
        "If you track A/P in USD only, use the USD version and treat SOL as supporting detail.",
    )?;

    // Other operating expenses (off-chain)
    row(
        "other_expenses_usd",
        "Other opex",
        "expense",
        "USD",
        "Non-on-chain operating costs (hosting, tools, contractors, hardware/software, etc.).",
        "Day-to-day operating costs; needs supporting detail for deductibility and categorization.",
        "Local expense tracker (manual entries, recurring schedule, and optional Notion-based contractor hours).",
        "Keep vendor-level support (invoices/receipts). This report is only as accurate as the expense inputs.",
    )?;

    // Summary-level totals (monthly)
    row(
        "total_revenue_usd",
        "Total revenue (USD)",
        "derived",
        "USD",
        "Sum of revenue streams for the period in USD.",
        "Top-line measure for P&L reporting.",
        "Computed by this tool (commission + leader fees + MEV tips + BAM), valued at daily SOL USD prices.",
        "If you revalue items with a different pricing policy, recompute totals externally.",
    )?;
    row(
        "total_expenses_usd",
        "Total expenses (USD)",
        "derived",
        "USD",
        "Sum of expenses for the period in USD (vote costs net + DoubleZero accrued + other opex).",
        "Used to compute net profit and track operating costs.",
        "Computed by this tool from underlying expense components.",
        "Ensure SFDP treatment aligns with your reporting policy (contra-expense vs other income).",
    )?;
    row(
        "net_profit_usd",
        "Net profit (USD)",
        "derived",
        "USD",
        "Total revenue minus total expenses for the period in USD.",
        "High-level performance metric; drives tax planning discussions.",
        "Computed by this tool from totals.",
        "Does not include capital gains/losses from selling/swapping crypto unless those are separately modeled.",
    )?;

    // Helpful meta fields that appear in ledgers
    row(
        "epoch",
        "Epoch (Solana)",
        "metadata",
        "",
        "Solana epoch identifier (roughly a ~2-day network period).",
        "Used to group staking rewards and some fee calculations.",
        "Solana protocol epoch numbering.",
        "Dates for epoch-based rows can be approximate if derived from epoch number rather than on-chain timestamp.",
    )?;
    row(
        "tx_signature",
        "Transaction signature / id",
        "metadata",
        "",
        "Unique identifier for a Solana transaction; some rows use a synthetic id like `epoch-N` for epoch-based items.",
        "Helps trace back to on-chain evidence for audits/reconciliation.",
        "Solana transaction signatures; synthetic ids generated by this tool for certain epoch-based rows.",
        "If you need strict auditability, prefer rows with real transaction signatures and reconcile epoch-based synthetic ids to on-chain reward records.",
    )?;

    row(
        "accounting_treatment",
        "Accounting treatment label",
        "metadata",
        "",
        "Classification label in ledgers indicating whether a row is Income (Revenue), Expense, or Balance Sheet movement.",
        "Helps map lines to chart-of-accounts buckets and prevents treating transfers as income/expense.",
        "Generated by this tool based on transaction categorization and report type.",
        "You may override classifications for your specific entity/tax approach, but this provides a sane default.",
    )?;

    // Scope and categorization assumptions (these are usually the #1 source of confusion)
    row(
        "wallet_scope",
        "Wallet/account scope",
        "assumption",
        "",
        "Which on-chain accounts are considered 'in scope' for this validator's books (vote/identity/withdraw authority and any configured personal wallet used for seeding/flows).",
        "Determines whether transfers are treated as internal movements vs external (potential distributions, contributions, etc.).",
        "config.toml validator addresses (vote_account, identity, withdraw_authority, personal_wallet) plus derived token accounts (ATAs) where applicable.",
        "Confirm which wallets legally belong to the reporting entity. If a personal wallet is mixed-use, treasury transfers may require manual classification (owner distribution vs business transfer).",
    )?;
    row(
        "treasury_transfer_types",
        "Treasury transfer types",
        "metadata",
        "",
        "High-level labels used in treasury_ledger.csv: Capital Contribution, Internal Transfer, Prepayment, Withdrawal, Other.",
        "Prevents treating balance sheet movements as revenue/expense.",
        "Generated by this tool based on known addresses and transfer direction.",
        "Withdrawals are not automatically expenses; they may represent owner distributions or moving funds to an exchange. Review and reclassify as needed.",
    )?;
    row(
        "address_labels",
        "Address labels (counterparty identification)",
        "info",
        "",
        "Human-friendly labels for blockchain addresses (e.g., Solana Foundation, Jito, exchanges).",
        "Improves auditability and reduces time spent matching addresses to counterparties.",
        "This tool's built-in address label map + configured addresses in config.toml.",
        "Labels are best-effort and may be incomplete. For audit support, keep your own mapping for any recurring counterparties.",
    )?;
    row(
        "tx_signature_truncation",
        "Transaction signature truncation",
        "assumption",
        "",
        "Some CSVs show only the first 16 characters of a Solana transaction signature for readability.",
        "A truncated id may not be sufficient to independently verify a transaction without looking up the full signature.",
        "Generated by this tool in the CSV output formatting.",
        "If you need full signatures for audit workpapers, we can add a 'Tx_Signature_Full' column or stop truncating signatures in the ledgers.",
    )?;
    row(
        "year_filter_behavior",
        "Year filter behavior (--year)",
        "assumption",
        "",
        "The --year flag filters summary.csv and the printed console summary; the ledgers are not currently year-filtered.",
        "If you hand the accountant only a single year's summary but the ledgers include multiple years, it can look inconsistent.",
        "validator-accounting CLI behavior.",
        "If you want year-filtered ledgers, we can implement it so all CSVs align to the same period.",
    )?;

    // Off-chain expense metadata commonly needed for substantiation
    row(
        "paid_with",
        "Paid with",
        "metadata",
        "",
        "Payment method for an off-chain expense (USD, SOL, credit card, etc.).",
        "Supports cash/bank/credit card reconciliation and helps determine whether an expense is crypto-denominated.",
        "User-entered expense tracker fields (manual/recurring/Notion-derived).",
        "If paid in crypto, you may have an associated disposition (capital gain/loss) when the crypto was spent.",
    )?;
    row(
        "invoice_id",
        "Invoice/receipt id",
        "metadata",
        "",
        "Optional invoice, receipt, or reference id for off-chain expenses.",
        "Helps tie ledger lines to supporting documentation.",
        "User-entered expense tracker fields.",
        "If blank, consider adding invoice ids for material expenses to simplify substantiation.",
    )?;

    // Data sources (useful to explain confidence/limitations)
    row(
        "data_sources",
        "Data sources used",
        "info",
        "",
        "External sources used by this tool: Solana RPC (Helius endpoint), CoinGecko pricing, Jito APIs, optional Dune backfills, optional Notion hours logs.",
        "Explains where numbers come from and where gaps/estimates might exist.",
        "Runtime configuration + cached datasets in ./data.",
        "If a source is missing or rate-limited, the tool may fall back to cached data or estimates; review the console output for warnings.",
    )?;

    // Explicitly call out what's missing from these reports (so expectations are clear)
    row(
        "out_of_scope_capital_gains",
        "Capital gains/losses (out of scope)",
        "out_of_scope",
        "",
        "Gains/losses from selling, swapping, or spending crypto (dispositions) are not computed here.",
        "These are often the largest tax complexity for crypto activity.",
        "Dedicated tax lot software / exchange statements / detailed transaction history.",
        "Use these CSVs for validator operations income/expense and wallet movement context, but compute dispositions separately (cost basis, proceeds, lots, wash sale rules if applicable).",
    )?;

    wtr.flush()?;
    println!("  Generated: {}", path.display());
    Ok(())
}

/// Generate income_ledger.csv
fn generate_income_ledger(
    output_dir: &Path,
    rewards: &[EpochReward],
    categorized: &CategorizedTransfers,
    mev_claims: &[MevClaim],
    bam_claims: &[BamClaim],
    leader_fees: &[EpochLeaderFees],
    prices: &PriceCache,
) -> Result<()> {
    let path = output_dir.join(constants::INCOME_LEDGER_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Header
    wtr.write_record([
        "Date (YYYY-MM-DD)",
        "Epoch (Solana ~2-day period)",
        "Accounting_Treatment (Income/Expense/Balance Sheet)",
        "Source (plain English)",
        "From_Address (blockchain address or label)",
        "From_Label (who/what is it?)",
        "Amount_SOL (SOL, Solana cryptocurrency)",
        "USD_Price (USD per 1 SOL)",
        "USD_Value (Amount_SOL * USD_Price)",
        "Tx_Signature (tx id or epoch-N)",
        "Notes (plain English)",
    ])?;

    // Commission rewards
    for reward in rewards {
        let date = reward.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = reward.amount_sol * price;

        wtr.write_record([
            date,
            &reward.epoch.to_string(),
            "Income (Revenue)",
            "Staking commission (Solana inflation rewards)",
            "Solana protocol",
            "Staking inflation reward (to validator vote account)",
            &format!("{:.6}", reward.amount_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &format!("epoch-{}", reward.epoch),
            &format!(
                "Staking reward payout. Validator keeps {}% commission from delegated stake rewards.",
                reward.commission
            ),
        ])?;
    }

    // Note: SFDP reimbursements are NOT included in income - they are expense offsets

    // MEV: Use Jito API claims as source of truth to avoid double-counting.
    // mev_deposits (transfers) and mev_claims (API) represent the same income.
    // Only use mev_deposits as fallback when mev_claims is empty.

    // MEV deposits (from transfer detection) - only when no Jito API data
    // These are fallback data when Jito API doesn't have epoch info
    for transfer in &categorized.mev_deposits {
        // Skip if we have Jito API data for this epoch (avoid double-counting)
        // Note: transfers don't have epoch directly, so we include them only if
        // mev_claims is empty (no API data at all)
        if !mev_claims.is_empty() {
            continue;
        }

        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "",
            "Income (Revenue)",
            "MEV tips (Jito)",
            &transfer.from.to_string(),
            &transfer.from_label,
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Extra validator income from optional 'tips' paid via Jito (often for transaction priority). Fallback row: inferred from on-chain transfer (no per-epoch API claim data).",
        ])?;
    }

    // MEV claims from Jito API (primary source)
    for claim in mev_claims {
        let date = claim.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = claim.amount_sol * price;

        wtr.write_record([
            date,
            &claim.epoch.to_string(),
            "Income (Revenue)",
            "MEV tips (Jito)",
            "Jito tip distribution",
            "MEV tip payout (to validator vote account)",
            &format!("{:.6}", claim.amount_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &format!("epoch-{}", claim.epoch),
            &format!(
                "Extra validator income from optional 'tips' paid via Jito (often for transaction priority). Validator received ~{}% of {:.4} SOL of tips for this epoch.",
                if claim.total_tips_lamports > 0 {
                    (claim.commission_lamports as f64 / claim.total_tips_lamports as f64 * 100.0).round() as u64
                } else {
                    0
                },
                claim.total_tips_lamports as f64 / 1e9
            ),
        ])?;
    }

    // Leader slot fees (block production rewards)
    for fees in leader_fees {
        let date = fees.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = fees.total_fees_sol * price;

        wtr.write_record([
            date,
            &fees.epoch.to_string(),
            "Income (Revenue)",
            "Block production fees (Solana)",
            "Solana protocol",
            "Transaction fees earned for producing blocks",
            &format!("{:.6}", fees.total_fees_sol),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &format!("epoch-{}", fees.epoch),
            &format!(
                "Validator produced {} blocks ({} skipped slots) during this epoch.",
                fees.blocks_produced, fees.skipped_slots
            ),
        ])?;
    }

    // BAM claims (jitoSOL rewards per JIP-31)
    for claim in bam_claims {
        let date = claim.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        // Use the SOL-equivalent value for USD calculation
        let usd_value = claim.amount_sol_equivalent * price;
        let jitosol_amount = claim.amount_jitosol_lamports as f64 / 1e9;

        wtr.write_record([
            date,
            &claim.epoch.to_string(),
            "Income (Revenue)",
            "Validator incentives (Jito BAM, paid in jitoSOL)",
            "Jito BAM Boost program",
            "jitoSOL reward payout (to validator token account)",
            &format!("{:.6}", claim.amount_sol_equivalent),
            &format!("{:.2}", price),
            &format!("{:.2}", usd_value),
            &claim.tx_signature[..claim.tx_signature.len().min(16)],
            &format!(
                "{:.6} jitoSOL (a liquid staking token representing staked SOL). Valued at {:.4} SOL per jitoSOL.",
                jitosol_amount,
                claim.jitosol_sol_rate.unwrap_or(1.0)
            ),
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

/// Generate expense_ledger.csv
fn generate_expense_ledger(
    output_dir: &Path,
    expenses: &[Expense],
    vote_costs: &[EpochVoteCost],
    doublezero_fees: &[DoubleZeroFee],
    prices: &PriceCache,
    config: &Config,
) -> Result<()> {
    let path = output_dir.join(constants::EXPENSE_LEDGER_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Header
    wtr.write_record([
        "Date (YYYY-MM-DD)",
        "Epoch (Solana ~2-day period)",
        "Vendor",
        "Accounting_Treatment (Income/Expense/Balance Sheet)",
        "Category (plain English)",
        "Description (plain English)",
        "Amount_SOL (SOL, Solana cryptocurrency)",
        "Amount_USD (gross valuation on Date)",
        "Paid_With (asset)",
        "SFDP_Coverage (% of vote fees reimbursed by Solana Foundation program)",
        "Net_Amount_USD (gross * (1 - coverage))",
        "Invoice_ID",
    ])?;

    // Vote costs per epoch (actual on-chain data)
    for cost in vote_costs {
        let date = cost.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let gross_usd = cost.total_fee_sol * price;

        // Calculate SFDP coverage for this epoch's date
        let parsed_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap());
        let coverage = config.sfdp_coverage_percent(&parsed_date);
        let net_usd = gross_usd * (1.0 - coverage);

        wtr.write_record([
            date,
            &cost.epoch.to_string(),
            "Solana Network",
            "Expense",
            "On-chain vote transaction fees",
            &format!(
                "Transaction fees for {} validator vote transactions (source: {}). SFDP = Solana Foundation Delegation Program; SFDP_Coverage indicates the % reimbursed, and Net_Amount_USD is the remaining cost.",
                cost.vote_count, cost.source
            ),
            &format!("{:.6}", cost.total_fee_sol),
            &format!("{:.2}", gross_usd),
            "SOL",
            &format!("{:.0}%", coverage * 100.0),
            &format!("{:.2}", net_usd),
            "",
        ])?;
    }

    // DoubleZero fees (block reward sharing)
    for fee in doublezero_fees {
        let date = fee.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = fee.liability_sol * price;
        let fee_base_sol = fee.fee_base_lamports as f64 / 1e9;
        let rate_percent = fee.fee_rate_bps as f64 / 100.0;
        let status = if fee.is_estimate { "estimated" } else { "final" };

        wtr.write_record([
            date,
            &fee.epoch.to_string(),
            "DoubleZero",
            "Expense",
            "Network fees (DoubleZero)",
            &format!(
                "Block reward sharing fee owed to DoubleZero (base {:.4} SOL, {:.2}% {}, paid separately when deposited).",
                fee_base_sol, rate_percent, status
            ),
            &format!("{:.6}", fee.liability_sol),
            &format!("{:.2}", usd_value),
            "SOL",
            "",
            &format!("{:.2}", usd_value),
            "",
        ])?;
    }

    // Off-chain expenses (hosting, contractors, etc.)
    for expense in expenses {
        let expense_usd = expense.amount_usd;
        wtr.write_record([
            &expense.date,
            "", // No epoch for off-chain expenses
            &expense.vendor,
            "Expense",
            &expense.category.to_string(),
            &expense.description,
            "", // No SOL amount
            &format!("{:.2}", expense_usd),
            &expense.paid_with,
            "", // No SFDP coverage for off-chain expenses
            &format!("{:.2}", expense_usd),
            expense.invoice_id.as_deref().unwrap_or(""),
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

/// Generate treasury_ledger.csv (transfers, seeding, withdrawals)
fn generate_treasury_ledger(output_dir: &Path, categorized: &CategorizedTransfers, prices: &PriceCache) -> Result<()> {
    let path = output_dir.join(constants::TREASURY_LEDGER_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Header
    wtr.write_record([
        "Date (YYYY-MM-DD)",
        "Type (plain English)",
        "From_Address (blockchain address)",
        "From_Label (who/what is it?)",
        "To_Address (blockchain address)",
        "To_Label (who/what is it?)",
        "Accounting_Treatment (Income/Expense/Balance Sheet)",
        "Amount_SOL (SOL, Solana cryptocurrency)",
        "USD_Value (valuation on Date)",
        "Tx_Signature (tx id)",
        "Notes (plain English)",
    ])?;

    // Initial seeding
    for transfer in &categorized.seeding {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Capital Contribution",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            "Balance Sheet (Owner contribution)",
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Owner capital contribution to fund validator operations (balance sheet movement, not income).",
        ])?;
    }

    // Vote funding (internal transfers)
    for transfer in &categorized.vote_funding {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Internal Transfer",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            "Balance Sheet (Internal transfer)",
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Move funds between internal validator wallets to pay on-chain transaction fees (not income).",
        ])?;
    }

    // DoubleZero payments (prepaid network fees)
    for transfer in &categorized.doublezero_payments {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Prepayment",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            "Balance Sheet (Prepayment/deposit)",
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Deposit to DoubleZero to prepay network fee obligations (balance sheet movement; expense recorded as fees accrue).",
        ])?;
    }

    // Withdrawals
    for transfer in &categorized.withdrawals {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Withdrawal",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            "Balance Sheet (Transfer out)",
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Transfer out to exchange/personal wallet (owner distribution or asset movement; not automatically income/expense).",
        ])?;
    }

    // Other transfers
    for transfer in &categorized.other {
        let date = transfer.date.as_deref().unwrap_or("unknown");
        let price = get_price(prices, date);
        let usd_value = transfer.amount_sol * price;

        wtr.write_record([
            date,
            "Other",
            &transfer.from.to_string(),
            &transfer.from_label,
            &transfer.to.to_string(),
            &transfer.to_label,
            "Balance Sheet (Transfer)",
            &format!("{:.6}", transfer.amount_sol),
            &format!("{:.2}", usd_value),
            &transfer.signature[..16],
            "Uncategorized transfer (typically a balance sheet movement, not P&L).",
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

/// Generate summary.csv (monthly P&L with annual summaries)
fn generate_summary(output_dir: &Path, data: &ReportData, year_filter: Option<i32>) -> Result<()> {
    let path = output_dir.join(constants::SUMMARY_FILENAME);
    let mut wtr = Writer::from_path(&path)?;

    // Aggregate by month
    let mut monthly: HashMap<String, MonthlyData> = HashMap::new();

    // Commission
    for reward in data.rewards {
        if let Some(date) = &reward.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.commission_sol += reward.amount_sol;
            entry.commission_usd += reward.amount_sol * price;
        }
    }

    // SFDP reimbursements
    for transfer in &data.categorized.sfdp_reimbursements {
        if let Some(date) = &transfer.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.sfdp_sol += transfer.amount_sol;
            entry.sfdp_usd += transfer.amount_sol * price;
        }
    }

    // MEV: Use Jito API claims as source of truth to avoid double-counting.
    // Only use mev_deposits as fallback when mev_claims is empty.
    if data.mev_claims.is_empty() {
        // Fallback: use transfer detection when no Jito API data
        for transfer in &data.categorized.mev_deposits {
            if let Some(date) = &transfer.date {
                let month = &date[..7];
                let price = get_price(data.prices, date);
                let entry = monthly.entry(month.to_string()).or_default();
                entry.mev_sol += transfer.amount_sol;
                entry.mev_usd += transfer.amount_sol * price;
            }
        }
    } else {
        // Primary: use Jito API data (per-epoch, accurate)
        for claim in data.mev_claims {
            if let Some(date) = &claim.date {
                let month = &date[..7];
                let price = get_price(data.prices, date);
                let entry = monthly.entry(month.to_string()).or_default();
                entry.mev_sol += claim.amount_sol;
                entry.mev_usd += claim.amount_sol * price;
            }
        }
    }

    // BAM rewards (jitoSOL, tracked in SOL equivalent)
    for claim in data.bam_claims {
        if let Some(date) = &claim.date {
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.bam_sol += claim.amount_sol_equivalent;
            entry.bam_usd += claim.amount_sol_equivalent * price;
        }
    }

    // Leader fees from block production
    for fees in data.leader_fees {
        if let Some(date) = &fees.date {
            if date.len() < 7 {
                continue;
            }
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.leader_fees_sol += fees.total_fees_sol;
            entry.leader_fees_usd += fees.total_fees_sol * price;
        }
    }

    // Vote costs by month (with SFDP coverage calculation)
    for cost in data.vote_costs {
        if let Some(date) = &cost.date {
            if date.len() < 7 {
                continue;
            }
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let gross_usd = cost.total_fee_sol * price;

            // Calculate SFDP coverage for net cost
            let parsed_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .unwrap_or_else(|_| chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap());
            let coverage = data.config.sfdp_coverage_percent(&parsed_date);
            let net_usd = gross_usd * (1.0 - coverage);

            let entry = monthly.entry(month.to_string()).or_default();
            entry.vote_costs_sol += cost.total_fee_sol;
            entry.vote_costs_gross_usd += gross_usd;
            entry.vote_costs_net_usd += net_usd;
        }
    }

    // DoubleZero fees by month
    for fee in data.doublezero_fees {
        if let Some(date) = &fee.date {
            if date.len() < 7 {
                continue;
            }
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.doublezero_sol += fee.liability_sol;
            entry.doublezero_usd += fee.liability_sol * price;
        }
    }

    // DoubleZero payments by month (prepayments to deposit PDA)
    for payment in &data.categorized.doublezero_payments {
        if let Some(date) = &payment.date {
            if date.len() < 7 {
                continue;
            }
            let month = &date[..7];
            let price = get_price(data.prices, date);
            let entry = monthly.entry(month.to_string()).or_default();
            entry.doublezero_paid_sol += payment.amount_sol;
            entry.doublezero_paid_usd += payment.amount_sol * price;
        }
    }

    // Expenses by month
    for expense in data.expenses {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&expense.date, "%Y-%m-%d") {
            let month = date.format("%Y-%m").to_string();
            let entry = monthly.entry(month).or_default();
            entry.other_expenses_usd += expense.amount_usd;
        }
    }

    // Header
    wtr.write_record([
        "Month (YYYY-MM)",
        "Commission_SOL (staking commission, SOL)",
        "Commission_USD (staking commission, USD)",
        "Leader_Fees_SOL (block production fees, SOL)",
        "Leader_Fees_USD (block production fees, USD)",
        "MEV_SOL (Jito MEV tips, SOL)",
        "MEV_USD (Jito MEV tips, USD)",
        "BAM_SOL (Jito BAM incentives, SOL-equiv)",
        "BAM_USD (Jito BAM incentives, USD)",
        "Total_Revenue_USD (sum of revenue items)",
        "Vote_Costs_SOL (on-chain vote tx fees, SOL)",
        "Vote_Costs_Gross_USD (before SFDP reimbursement)",
        "SFDP_Offset_USD (Solana Foundation reimbursement, reduces expense)",
        "Vote_Costs_Net_USD (after SFDP reimbursement)",
        "DoubleZero_Fees_SOL (accrued, SOL)",
        "DoubleZero_Fees_USD (accrued, USD)",
        "DoubleZero_Paid_SOL (payments made, SOL)",
        "DoubleZero_Paid_USD (payments made, USD)",
        "DoubleZero_Outstanding_SOL (accrued - paid, SOL)",
        "DoubleZero_Outstanding_USD (accrued - paid, USD)",
        "Other_Expenses_USD (off-chain expenses)",
        "Total_Expenses_USD (vote net + DoubleZero + other)",
        "Net_Profit_USD (revenue - expenses)",
        "YTD_Profit_USD (resets each Jan)",
    ])?;

    let mut months: Vec<_> = monthly.keys().cloned().collect();
    months.sort();

    // Filter by year if specified
    let months: Vec<_> = if let Some(year) = year_filter {
        let year_prefix = format!("{}-", year);
        months.into_iter().filter(|m| m.starts_with(&year_prefix)).collect()
    } else {
        months
    };

    // Track annual totals for summary rows
    let mut annual_totals: HashMap<String, MonthlyData> = HashMap::new();
    let mut ytd = 0.0;
    let mut current_year: Option<String> = None;

    for month in &months {
        let year = &month[..4];
        let data = &monthly[month];
        // SFDP is expense offset, not revenue. BAM rewards are revenue.
        let total_revenue = data.commission_usd + data.leader_fees_usd + data.mev_usd + data.bam_usd;
        let total_expenses = data.vote_costs_net_usd + data.doublezero_usd + data.other_expenses_usd;
        let net_profit = total_revenue - total_expenses;

        // Reset YTD at year boundary
        if current_year.as_deref() != Some(year) {
            current_year = Some(year.to_string());
            ytd = 0.0;
        }
        ytd += net_profit;

        // Accumulate annual totals
        let annual = annual_totals.entry(year.to_string()).or_default();
        annual.commission_sol += data.commission_sol;
        annual.commission_usd += data.commission_usd;
        annual.leader_fees_sol += data.leader_fees_sol;
        annual.leader_fees_usd += data.leader_fees_usd;
        annual.mev_sol += data.mev_sol;
        annual.mev_usd += data.mev_usd;
        annual.bam_sol += data.bam_sol;
        annual.bam_usd += data.bam_usd;
        annual.sfdp_sol += data.sfdp_sol;
        annual.sfdp_usd += data.sfdp_usd;
        annual.vote_costs_sol += data.vote_costs_sol;
        annual.vote_costs_gross_usd += data.vote_costs_gross_usd;
        annual.vote_costs_net_usd += data.vote_costs_net_usd;
        annual.doublezero_sol += data.doublezero_sol;
        annual.doublezero_usd += data.doublezero_usd;
        annual.doublezero_paid_sol += data.doublezero_paid_sol;
        annual.doublezero_paid_usd += data.doublezero_paid_usd;
        annual.other_expenses_usd += data.other_expenses_usd;

        let sfdp_offset = data.vote_costs_gross_usd - data.vote_costs_net_usd;
        let dz_outstanding_sol = data.doublezero_sol - data.doublezero_paid_sol;
        let dz_outstanding_usd = data.doublezero_usd - data.doublezero_paid_usd;

        wtr.write_record([
            month,
            &format!("{:.4}", data.commission_sol),
            &format!("{:.2}", data.commission_usd),
            &format!("{:.4}", data.leader_fees_sol),
            &format!("{:.2}", data.leader_fees_usd),
            &format!("{:.4}", data.mev_sol),
            &format!("{:.2}", data.mev_usd),
            &format!("{:.4}", data.bam_sol),
            &format!("{:.2}", data.bam_usd),
            &format!("{:.2}", total_revenue),
            &format!("{:.4}", data.vote_costs_sol),
            &format!("{:.2}", data.vote_costs_gross_usd),
            &format!("{:.2}", sfdp_offset),
            &format!("{:.2}", data.vote_costs_net_usd),
            &format!("{:.4}", data.doublezero_sol),
            &format!("{:.2}", data.doublezero_usd),
            &format!("{:.4}", data.doublezero_paid_sol),
            &format!("{:.2}", data.doublezero_paid_usd),
            &format!("{:.4}", dz_outstanding_sol),
            &format!("{:.2}", dz_outstanding_usd),
            &format!("{:.2}", data.other_expenses_usd),
            &format!("{:.2}", total_expenses),
            &format!("{:.2}", net_profit),
            &format!("{:.2}", ytd),
        ])?;
    }

    // Write annual summary rows
    let mut years: Vec<_> = annual_totals.keys().cloned().collect();
    years.sort();

    for year in &years {
        let data = &annual_totals[year];
        // SFDP is expense offset, not revenue. BAM rewards are revenue.
        let total_revenue = data.commission_usd + data.leader_fees_usd + data.mev_usd + data.bam_usd;
        let total_expenses = data.vote_costs_net_usd + data.doublezero_usd + data.other_expenses_usd;
        let net_profit = total_revenue - total_expenses;

        let sfdp_offset = data.vote_costs_gross_usd - data.vote_costs_net_usd;
        let dz_outstanding_sol = data.doublezero_sol - data.doublezero_paid_sol;
        let dz_outstanding_usd = data.doublezero_usd - data.doublezero_paid_usd;

        wtr.write_record([
            &format!("{} TOTAL", year),
            &format!("{:.4}", data.commission_sol),
            &format!("{:.2}", data.commission_usd),
            &format!("{:.4}", data.leader_fees_sol),
            &format!("{:.2}", data.leader_fees_usd),
            &format!("{:.4}", data.mev_sol),
            &format!("{:.2}", data.mev_usd),
            &format!("{:.4}", data.bam_sol),
            &format!("{:.2}", data.bam_usd),
            &format!("{:.2}", total_revenue),
            &format!("{:.4}", data.vote_costs_sol),
            &format!("{:.2}", data.vote_costs_gross_usd),
            &format!("{:.2}", sfdp_offset),
            &format!("{:.2}", data.vote_costs_net_usd),
            &format!("{:.4}", data.doublezero_sol),
            &format!("{:.2}", data.doublezero_usd),
            &format!("{:.4}", data.doublezero_paid_sol),
            &format!("{:.2}", data.doublezero_paid_usd),
            &format!("{:.4}", dz_outstanding_sol),
            &format!("{:.2}", dz_outstanding_usd),
            &format!("{:.2}", data.other_expenses_usd),
            &format!("{:.2}", total_expenses),
            &format!("{:.2}", net_profit),
            "", // No YTD for annual rows
        ])?;
    }

    wtr.flush()?;
    println!("  Generated: {}", path.display());

    Ok(())
}

#[derive(Default)]
struct MonthlyData {
    commission_sol: f64,
    commission_usd: f64,
    leader_fees_sol: f64,
    leader_fees_usd: f64,
    mev_sol: f64,
    mev_usd: f64,
    bam_sol: f64,
    bam_usd: f64,
    sfdp_sol: f64,
    sfdp_usd: f64,
    vote_costs_sol: f64,
    vote_costs_gross_usd: f64,
    vote_costs_net_usd: f64,
    doublezero_sol: f64,
    doublezero_usd: f64,
    doublezero_paid_sol: f64,
    doublezero_paid_usd: f64,
    other_expenses_usd: f64,
}

/// Normalize -0.0 to 0.0 for cleaner display
fn normalize_zero(val: f64) -> f64 {
    if val == 0.0 { 0.0 } else { val }
}

/// Print summary to console
pub fn print_summary(data: &ReportData, year_filter: Option<i32>) {
    // Helper to check if a date matches the year filter
    let matches_year = |date: &str| -> bool {
        if let Some(year) = year_filter {
            date.starts_with(&format!("{}-", year))
        } else {
            true
        }
    };

    println!("\n============================================================");
    if let Some(year) = year_filter {
        println!("                FINANCIAL SUMMARY ({})", year);
    } else {
        println!("                    FINANCIAL SUMMARY");
    }
    println!("============================================================\n");

    // Calculate totals (filtered by year if specified)
    let total_commission_sol: f64 = data
        .rewards
        .iter()
        .filter(|r| r.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|r| r.amount_sol)
        .sum();
    let total_commission_usd: f64 = data
        .rewards
        .iter()
        .filter(|r| r.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|r| {
            let price = get_price(data.prices, r.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
            r.amount_sol * price
        })
        .sum();

    // MEV: Use Jito API claims as source of truth to avoid double-counting.
    // Only use mev_deposits as fallback when mev_claims is empty.
    let (total_mev_sol, total_mev_usd) = if data.mev_claims.is_empty() {
        // Fallback: use transfer detection
        let mev_sol: f64 = data
            .categorized
            .mev_deposits
            .iter()
            .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|t| t.amount_sol)
            .sum();
        let mev_usd: f64 = data
            .categorized
            .mev_deposits
            .iter()
            .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|t| {
                let price = get_price(data.prices, t.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
                t.amount_sol * price
            })
            .sum();
        (mev_sol, mev_usd)
    } else {
        // Primary: use Jito API data
        let mev_sol: f64 = data
            .mev_claims
            .iter()
            .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|c| c.amount_sol)
            .sum();
        let mev_usd: f64 = data
            .mev_claims
            .iter()
            .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
            .map(|c| {
                let price = get_price(data.prices, c.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
                c.amount_sol * price
            })
            .sum();
        (mev_sol, mev_usd)
    };

    // BAM rewards (jitoSOL converted to SOL equivalent)
    let total_bam_sol: f64 = data
        .bam_claims
        .iter()
        .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|c| c.amount_sol_equivalent)
        .sum();
    let total_bam_usd: f64 = data
        .bam_claims
        .iter()
        .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|c| {
            let price = get_price(data.prices, c.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
            c.amount_sol_equivalent * price
        })
        .sum();

    // Leader fees from block production
    let total_leader_fees_sol: f64 = data
        .leader_fees
        .iter()
        .filter(|f| f.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|f| f.total_fees_sol)
        .sum();
    let total_leader_fees_usd: f64 = data
        .leader_fees
        .iter()
        .filter(|f| f.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|f| {
            let price = get_price(data.prices, f.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
            f.total_fees_sol * price
        })
        .sum();

    // Note: SFDP is tracked as expense offset, not calculated separately for revenue

    let total_seeding_sol: f64 = data
        .categorized
        .seeding
        .iter()
        .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|t| t.amount_sol)
        .sum();

    // Vote costs (with SFDP coverage)
    let total_vote_costs_sol: f64 = data
        .vote_costs
        .iter()
        .filter(|c| c.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|c| c.total_fee_sol)
        .sum();
    let mut total_vote_costs_gross_usd = 0.0;
    let mut total_vote_costs_net_usd = 0.0;

    for cost in data.vote_costs {
        let date = cost.date.as_deref().unwrap_or(constants::FALLBACK_DATE);
        if !matches_year(date) {
            continue;
        }
        let price = get_price(data.prices, date);
        let gross_usd = cost.total_fee_sol * price;

        // Calculate SFDP coverage
        let parsed_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .unwrap_or_else(|_| chrono::NaiveDate::parse_from_str(constants::FALLBACK_DATE, "%Y-%m-%d").unwrap());
        let coverage = data.config.sfdp_coverage_percent(&parsed_date);
        let net_usd = gross_usd * (1.0 - coverage);

        total_vote_costs_gross_usd += gross_usd;
        total_vote_costs_net_usd += net_usd;
    }

    // DoubleZero fees
    let total_doublezero_sol: f64 = data
        .doublezero_fees
        .iter()
        .filter(|f| f.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|f| f.liability_sol)
        .sum();
    let total_doublezero_usd: f64 = data
        .doublezero_fees
        .iter()
        .filter(|f| f.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|f| {
            let price = get_price(data.prices, f.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
            f.liability_sol * price
        })
        .sum();
    let total_doublezero_paid_sol: f64 = data
        .categorized
        .doublezero_payments
        .iter()
        .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|t| t.amount_sol)
        .sum();
    let total_doublezero_paid_usd: f64 = data
        .categorized
        .doublezero_payments
        .iter()
        .filter(|t| t.date.as_deref().map(&matches_year).unwrap_or(false))
        .map(|t| {
            let price = get_price(data.prices, t.date.as_deref().unwrap_or(constants::FALLBACK_DATE));
            t.amount_sol * price
        })
        .sum();
    let total_doublezero_outstanding_sol = total_doublezero_sol - total_doublezero_paid_sol;
    let total_doublezero_outstanding_usd = total_doublezero_usd - total_doublezero_paid_usd;

    // Other expenses (hosting, contractors, etc.)
    let total_other_expenses: f64 = data
        .expenses
        .iter()
        .filter(|e| matches_year(&e.date))
        .map(|e| e.amount_usd)
        .sum();
    let hosting_expenses: f64 = data
        .expenses
        .iter()
        .filter(|e| e.category == ExpenseCategory::Hosting && matches_year(&e.date))
        .map(|e| e.amount_usd)
        .sum();
    let contractor_expenses: f64 = data
        .expenses
        .iter()
        .filter(|e| e.category == ExpenseCategory::Contractor && matches_year(&e.date))
        .map(|e| e.amount_usd)
        .sum();

    // SFDP is an expense offset, not revenue. BAM rewards are revenue.
    let total_revenue_usd = total_commission_usd + total_leader_fees_usd + total_mev_usd + total_bam_usd;
    let total_expenses_usd = total_vote_costs_net_usd + total_doublezero_usd + total_other_expenses;
    let net_profit = total_revenue_usd - total_expenses_usd;

    // Normalize values to avoid displaying -0.0
    let total_commission_sol = normalize_zero(total_commission_sol);
    let total_commission_usd = normalize_zero(total_commission_usd);
    let total_leader_fees_sol = normalize_zero(total_leader_fees_sol);
    let total_leader_fees_usd = normalize_zero(total_leader_fees_usd);
    let total_mev_sol = normalize_zero(total_mev_sol);
    let total_mev_usd = normalize_zero(total_mev_usd);
    let total_bam_sol = normalize_zero(total_bam_sol);
    let total_bam_usd = normalize_zero(total_bam_usd);
    let total_doublezero_sol = normalize_zero(total_doublezero_sol);
    let total_doublezero_usd = normalize_zero(total_doublezero_usd);
    let total_doublezero_paid_sol = normalize_zero(total_doublezero_paid_sol);
    let total_doublezero_paid_usd = normalize_zero(total_doublezero_paid_usd);
    let total_doublezero_outstanding_sol = normalize_zero(total_doublezero_outstanding_sol);
    let _total_doublezero_outstanding_usd = normalize_zero(total_doublezero_outstanding_usd);
    let total_seeding_sol = normalize_zero(total_seeding_sol);

    println!("REVENUE:");
    println!(
        "  Commission:         {:>10.4} SOL  ${:>10.2}",
        total_commission_sol, total_commission_usd
    );
    println!(
        "  Leader Fees:        {:>10.4} SOL  ${:>10.2}",
        total_leader_fees_sol, total_leader_fees_usd
    );
    println!(
        "  Jito MEV:           {:>10.4} SOL  ${:>10.2}",
        total_mev_sol, total_mev_usd
    );
    if total_bam_sol > 0.0 || !data.bam_claims.is_empty() {
        println!(
            "  BAM Rewards:        {:>10.4} SOL  ${:>10.2}",
            total_bam_sol, total_bam_usd
        );
    }
    println!("  ─────────────────────────────────────────────");
    println!(
        "  Total Revenue:      {:>10.4} SOL  ${:>10.2}",
        total_commission_sol + total_leader_fees_sol + total_mev_sol + total_bam_sol,
        total_revenue_usd
    );

    println!("\nEXPENSES:");
    println!(
        "  Vote Fees (gross):  {:>10.4} SOL  ${:>10.2}",
        total_vote_costs_sol, total_vote_costs_gross_usd
    );
    println!(
        "  SFDP Offset:                   -${:>10.2}",
        total_vote_costs_gross_usd - total_vote_costs_net_usd
    );
    println!("  Vote Fees (net):                ${:>10.2}", total_vote_costs_net_usd);
    let show_doublezero = total_doublezero_sol > 0.0
        || total_doublezero_paid_sol > 0.0
        || total_doublezero_outstanding_sol.abs() > 0.000001;
    if show_doublezero {
        println!(
            "  DoubleZero Fees:    {:>10.4} SOL  ${:>10.2}",
            total_doublezero_sol, total_doublezero_usd
        );
        if total_doublezero_paid_sol > 0.0 || total_doublezero_outstanding_sol.abs() > 0.000001 {
            println!(
                "  DoubleZero Paid:    {:>10.4} SOL  ${:>10.2}",
                total_doublezero_paid_sol, total_doublezero_paid_usd
            );
        }
    }
    println!("  Hosting:                        ${:>10.2}", hosting_expenses);
    println!("  Contractor:                     ${:>10.2}", contractor_expenses);
    println!("  ─────────────────────────────────────────────");
    println!("  Total Expenses:                 ${:>10.2}", total_expenses_usd);

    println!("\nPROFIT/LOSS:");
    println!("  Net Profit:                     ${:>10.2}", net_profit);

    println!("\nCAPITAL:");
    println!("  Initial Seeding:    {:>10.4} SOL", total_seeding_sol);
    println!(
        "  Transfers found:    {}",
        data.categorized.seeding.len() + data.categorized.vote_funding.len()
    );

    println!("============================================================");
}
