//! Lightweight financial data types for dynamic rendering.
//!
//! These mirror the validator-accounting types but use `String` for addresses
//! instead of `solana_sdk::Pubkey`, keeping bp-web free of Solana SDK dependencies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Revenue types ───────────────────────────────────────────────────────────

/// Staking commission earned per epoch.
#[derive(Debug, Clone)]
pub struct EpochReward {
    pub epoch: u64,
    pub amount_sol: f64,
    pub commission: u8,
    pub date: Option<String>,
}

/// Block production fees earned per epoch.
#[derive(Debug, Clone)]
pub struct EpochLeaderFees {
    pub epoch: u64,
    pub total_fees_sol: f64,
    pub blocks_produced: u64,
    pub skipped_slots: u64,
    pub date: Option<String>,
}

/// Jito MEV tips commission per epoch.
#[derive(Debug, Clone)]
pub struct MevClaim {
    pub epoch: u64,
    pub amount_sol: f64,
    pub total_tips_lamports: u64,
    pub commission_lamports: u64,
    pub date: Option<String>,
}

/// Jito BAM reward (jitoSOL) per epoch.
#[derive(Debug, Clone)]
pub struct BamClaim {
    pub epoch: u64,
    pub amount_sol_equivalent: f64,
    pub amount_jitosol_lamports: u64,
    pub jitosol_sol_rate: Option<f64>,
    pub tx_signature: String,
    pub date: Option<String>,
}

// ── Expense types ───────────────────────────────────────────────────────────

/// On-chain vote transaction costs per epoch.
#[derive(Debug, Clone)]
pub struct EpochVoteCost {
    pub epoch: u64,
    pub vote_count: u64,
    pub total_fee_sol: f64,
    pub source: String,
    pub date: Option<String>,
}

/// DoubleZero block-reward-sharing fee per epoch.
#[derive(Debug, Clone)]
pub struct DoubleZeroFee {
    pub epoch: u64,
    pub liability_sol: f64,
    pub fee_base_lamports: u64,
    pub fee_rate_bps: u64,
    pub date: Option<String>,
    pub is_estimate: bool,
}

/// Off-chain expense category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExpenseCategory {
    Hosting,
    Contractor,
    Hardware,
    Software,
    VoteFees,
    Other,
}

impl std::fmt::Display for ExpenseCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hosting => write!(f, "Hosting"),
            Self::Contractor => write!(f, "Contractor"),
            Self::Hardware => write!(f, "Hardware"),
            Self::Software => write!(f, "Software"),
            Self::VoteFees => write!(f, "Vote Fees"),
            Self::Other => write!(f, "Other"),
        }
    }
}

impl ExpenseCategory {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hosting" => Self::Hosting,
            "contractor" => Self::Contractor,
            "hardware" => Self::Hardware,
            "software" => Self::Software,
            "votefees" | "vote fees" => Self::VoteFees,
            _ => Self::Other,
        }
    }
}

/// Off-chain expense entry (hosting, contractors, etc.).
#[derive(Debug, Clone)]
pub struct Expense {
    pub date: String,
    pub vendor: String,
    pub category: ExpenseCategory,
    pub description: String,
    pub amount_usd: f64,
    pub paid_with: String,
    pub invoice_id: Option<String>,
}

/// Recurring expense template that expands into monthly `Expense` entries.
#[derive(Debug, Clone)]
pub struct RecurringExpense {
    pub vendor: String,
    pub category: ExpenseCategory,
    pub description: String,
    pub amount_usd: f64,
    pub paid_with: String,
    pub start_date: String,
    pub end_date: Option<String>,
}

// ── Transfer types ──────────────────────────────────────────────────────────

/// SOL transfer between addresses (read from cache.sqlite sol_transfers table).
#[derive(Debug, Clone)]
pub struct SolTransfer {
    pub signature: String,
    pub date: Option<String>,
    pub from_address: String,
    pub to_address: String,
    pub amount_sol: f64,
    pub from_label: String,
    pub to_label: String,
}

/// Transfers bucketed by purpose.
#[derive(Debug, Default)]
pub struct CategorizedTransfers {
    pub seeding: Vec<SolTransfer>,
    pub sfdp_reimbursements: Vec<SolTransfer>,
    pub mev_deposits: Vec<SolTransfer>,
    pub doublezero_payments: Vec<SolTransfer>,
    pub vote_funding: Vec<SolTransfer>,
    pub withdrawals: Vec<SolTransfer>,
    pub other: Vec<SolTransfer>,
}

// ── Timeline event (matches html_report_template.html contract) ─────────────

/// One atomic financial event in the timeline.
///
/// The JS frontend expects this exact shape via `__TIMELINE_JSON__`.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub date: String,
    pub epoch: Option<u64>,
    pub event_type: &'static str,
    pub label: String,
    pub sublabel: Option<String>,
    pub amount_sol: f64,
    pub amount_usd: f64,
    pub cumulative_profit_usd: f64,
    pub cumulative_revenue_usd: f64,
    pub cumulative_expenses_usd: f64,
    pub is_pnl: bool,
}

// ── Tax row (intermediate for tax timeline) ─────────────────────────────────

/// A single row in the tax computation before timeline conversion.
#[derive(Debug, Clone)]
pub struct TaxRow {
    pub date: String,
    pub entry_type: String, // "Revenue", "Expense", "Return of Capital", "Reimbursement"
    pub category: String,
    pub description: String,
    pub sol_amount: Option<f64>,
    pub sol_price_usd: Option<f64>,
    pub usd_value: f64,
    pub destination: String,
    pub tx_signature: String,
}

// ── Prices ──────────────────────────────────────────────────────────────────

/// Daily SOL/USD prices keyed by ISO date string.
pub type PriceMap = HashMap<String, f64>;

/// Fallback price when date is missing from the cache.
const FALLBACK_PRICE: f64 = 170.0;

/// Look up the SOL/USD price for a date, falling back gracefully.
pub fn get_price(prices: &PriceMap, date: &str) -> f64 {
    if let Some(&p) = prices.get(date) {
        return p;
    }

    // Match validator-accounting behavior exactly:
    // use the closest available cached date (no fixed +/- window).
    if let Ok(target) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        let mut closest_price = FALLBACK_PRICE;
        let mut closest_diff = i64::MAX;

        for (d, p) in prices {
            if let Ok(cached_date) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                let diff = (target - cached_date).num_days().abs();
                if diff < closest_diff {
                    closest_diff = diff;
                    closest_price = *p;
                }
            }
        }

        return closest_price;
    }

    FALLBACK_PRICE
}

// ── Report data bundle ──────────────────────────────────────────────────────

/// Everything needed to build both timelines, passed by reference.
pub struct ReportData<'a> {
    pub rewards: &'a [EpochReward],
    pub categorized: &'a CategorizedTransfers,
    pub mev_claims: &'a [MevClaim],
    pub bam_claims: &'a [BamClaim],
    pub leader_fees: &'a [EpochLeaderFees],
    pub doublezero_fees: &'a [DoubleZeroFee],
    pub vote_costs: &'a [EpochVoteCost],
    pub expenses: &'a [Expense],
    pub prices: &'a PriceMap,
    pub sfdp_acceptance_date: Option<String>,
}
