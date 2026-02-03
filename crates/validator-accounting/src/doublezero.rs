//! DoubleZero fee tracking (block reward sharing)
//!
//! DoubleZero charges a flat percentage on leader fees (base fees + priority fees).
//! This module computes per-epoch liabilities from leader fee data.

use crate::config::Config;
use crate::leader_fees::EpochLeaderFees;
use crate::transactions;
use solana_sdk::pubkey::Pubkey;
use std::process::Command;
use std::str::FromStr;

/// Source label for fee entries
pub const DOUBLEZERO_SOURCE_COMPUTED: &str = "computed";

/// DoubleZero fee liability for a single epoch
#[derive(Debug, Clone)]
pub struct DoubleZeroFee {
    pub epoch: u64,
    /// Fee base in lamports (leader fees)
    pub fee_base_lamports: u64,
    /// Liability in lamports
    pub liability_lamports: u64,
    /// Liability in SOL (for reporting)
    pub liability_sol: f64,
    /// Fee rate in basis points (e.g., 500 = 5%)
    pub fee_rate_bps: u64,
    /// Epoch end date (approx)
    pub date: Option<String>,
    /// Source of this fee entry (computed/manual/etc.)
    pub source: String,
    /// Whether this entry is estimated (e.g., current epoch)
    pub is_estimate: bool,
}

/// Compute DoubleZero fees from leader fee data for a given epoch range.
pub fn compute_fees(
    config: &Config,
    leader_fees: &[EpochLeaderFees],
    start_epoch: u64,
    end_epoch: u64,
    current_epoch: u64,
) -> Vec<DoubleZeroFee> {
    let fee_rate_bps = config.doublezero_fee_rate_bps();
    if fee_rate_bps == 0 {
        return Vec::new();
    }

    let effective_start = start_epoch.max(config.doublezero_first_epoch);

    // Map leader fees by epoch for quick lookup
    let mut fee_map = std::collections::HashMap::new();
    for fee in leader_fees {
        fee_map.insert(fee.epoch, fee.total_fees_lamports);
    }

    let mut results = Vec::new();
    for epoch in effective_start..=end_epoch {
        let fee_base_lamports = *fee_map.get(&epoch).unwrap_or(&0);
        if fee_base_lamports == 0 {
            continue;
        }

        let liability_lamports = ((fee_base_lamports as u128 * fee_rate_bps as u128) / 10_000) as u64;
        if liability_lamports == 0 {
            continue;
        }

        // Use epoch end date (approx) for accrual timing
        let end_date = transactions::epoch_to_date(epoch.saturating_add(1));

        results.push(DoubleZeroFee {
            epoch,
            fee_base_lamports,
            liability_lamports,
            liability_sol: liability_lamports as f64 / 1e9,
            fee_rate_bps,
            date: Some(end_date),
            source: DOUBLEZERO_SOURCE_COMPUTED.to_string(),
            is_estimate: epoch >= current_epoch,
        });
    }

    results
}

/// Sum total DoubleZero fees (in SOL)
pub fn total_doublezero_fees_sol(fees: &[DoubleZeroFee]) -> f64 {
    fees.iter().map(|f| f.liability_sol).sum()
}

/// Best-effort derivation of the validator deposit PDA using the DoubleZero CLI.
///
/// This avoids requiring a hardcoded deposit account when the CLI is available.
pub fn derive_deposit_account_from_cli(node_id: &Pubkey) -> Option<Pubkey> {
    let node = node_id.to_string();
    let output = Command::new("doublezero-solana")
        .args([
            "revenue-distribution",
            "fetch",
            "validator-deposits",
            "--node-id",
            &node,
            "-u",
            "mainnet-beta",
        ])
        .output()
        .ok()?;

    let mut text = String::new();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));

    // Common error format: "No deposit account found at <PDA>."
    if let Some(idx) = text.find("at ") {
        let rest = &text[idx + 3..];
        if let Some(word) = rest.split_whitespace().next() {
            let cleaned = word.trim_matches(|c: char| !c.is_ascii_alphanumeric());
            if let Ok(pubkey) = Pubkey::from_str(cleaned) {
                return Some(pubkey);
            }
        }
    }

    // Fallback: scan tokens for a pubkey that isn't the node id
    for token in text.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
        if cleaned == node {
            continue;
        }
        if let Ok(pubkey) = Pubkey::from_str(cleaned) {
            return Some(pubkey);
        }
    }

    None
}
