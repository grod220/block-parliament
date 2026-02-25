//! Read config.toml for validator addresses & SFDP schedule.
//!
//! This is a lightweight mirror of `validator-accounting/src/config.rs`:
//! it reads the same config.toml but keeps addresses as strings (no Solana SDK).

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

// ── TOML shape ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct FileConfig {
    validator: ValidatorSection,
    #[serde(default)]
    doublezero: Option<DoubleZeroSection>,
}

#[derive(Debug, Deserialize)]
struct ValidatorSection {
    vote_account: String,
    identity: String,
    withdraw_authority: String,
    personal_wallet: String,
    bootstrap_date: String,
    #[serde(default)]
    sfdp_acceptance_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoubleZeroSection {
    #[serde(default)]
    deposit_account: Option<String>,
}

// ── Public config ─────────────────────────────────────────────────────────────

/// Lightweight validator config for bp-web (string addresses, no Solana SDK).
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    pub vote_account: String,
    pub identity: String,
    pub withdraw_authority: String,
    pub personal_wallet: String,
    pub bootstrap_date: String,
    pub sfdp_acceptance_date: Option<String>,
    pub doublezero_deposit_account: Option<String>,

    /// All "our" accounts for quick membership checks.
    our_accounts: HashSet<String>,
}

impl ValidatorConfig {
    /// Load config from a TOML file (typically `$DATA_DIR/config.toml`).
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("Failed to read config: {}", path.display()))?;
        let file: FileConfig =
            toml::from_str(&content).with_context(|| format!("Failed to parse config: {}", path.display()))?;

        let v = file.validator;
        let dz_deposit = file.doublezero.and_then(|dz| dz.deposit_account);

        let mut our_accounts = HashSet::new();
        our_accounts.insert(v.vote_account.clone());
        our_accounts.insert(v.identity.clone());
        our_accounts.insert(v.withdraw_authority.clone());
        // Note: personal_wallet intentionally NOT in our_accounts —
        // it's "ours" but transfers from/to it get special categorization
        // (seeding vs withdrawal) rather than "vote_funding".

        Ok(Self {
            vote_account: v.vote_account,
            identity: v.identity,
            withdraw_authority: v.withdraw_authority,
            personal_wallet: v.personal_wallet,
            bootstrap_date: v.bootstrap_date,
            sfdp_acceptance_date: v.sfdp_acceptance_date,
            doublezero_deposit_account: dz_deposit,
            our_accounts,
        })
    }

    /// Is this one of our validator operational accounts (vote, identity, withdraw)?
    pub fn is_our_account(&self, address: &str) -> bool {
        self.our_accounts.contains(address)
    }

    /// First day of the bootstrap month.
    ///
    /// If `bootstrap_date` is invalid, falls back to `2025-11-01`.
    pub fn business_start_date(&self) -> NaiveDate {
        let parsed = NaiveDate::parse_from_str(&self.bootstrap_date, "%Y-%m-%d")
            .unwrap_or_else(|_| NaiveDate::from_ymd_opt(2025, 11, 1).unwrap());
        NaiveDate::from_ymd_opt(parsed.year(), parsed.month(), 1).unwrap()
    }

    /// Bootstrap month in `YYYY-MM` format.
    pub fn business_start_month(&self) -> String {
        self.business_start_date().format("%Y-%m").to_string()
    }

    /// SFDP vote-cost coverage percentage for a given date.
    ///
    /// Schedule from acceptance date:
    /// - Months 0-2:  100% coverage
    /// - Months 3-5:   75% coverage
    /// - Months 6-8:   50% coverage
    /// - Months 9-11:  25% coverage
    /// - Month 12+:     0%
    pub fn sfdp_coverage_percent(&self, date: &chrono::NaiveDate) -> f64 {
        let Some(ref acceptance_str) = self.sfdp_acceptance_date else {
            return 0.0;
        };
        let Ok(acceptance) = chrono::NaiveDate::parse_from_str(acceptance_str, "%Y-%m-%d") else {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn cfg(sfdp: Option<&str>) -> ValidatorConfig {
        ValidatorConfig {
            vote_account: "VOTE".into(),
            identity: "ID".into(),
            withdraw_authority: "WA".into(),
            personal_wallet: "PW".into(),
            bootstrap_date: "2025-11-19".into(),
            sfdp_acceptance_date: sfdp.map(|s| s.into()),
            doublezero_deposit_account: None,
            our_accounts: ["VOTE", "ID", "WA"].iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn no_sfdp() {
        assert_eq!(
            cfg(None).sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
            0.0
        );
    }

    #[test]
    fn full_schedule() {
        let c = cfg(Some("2025-12-01"));
        // Month 0 (Dec 2025) → 100%
        assert_eq!(
            c.sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2025, 12, 15).unwrap()),
            1.0
        );
        // Month 2 (Feb 2026) → 100%
        assert_eq!(
            c.sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2026, 2, 15).unwrap()),
            1.0
        );
        // Month 3 (Mar 2026) → 75%
        assert_eq!(
            c.sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
            0.75
        );
        // Month 6 (Jun 2026) → 50%
        assert_eq!(
            c.sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()),
            0.50
        );
        // Month 9 (Sep 2026) → 25%
        assert_eq!(
            c.sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()),
            0.25
        );
        // Month 12 (Dec 2026) → 0%
        assert_eq!(
            c.sfdp_coverage_percent(&NaiveDate::from_ymd_opt(2026, 12, 15).unwrap()),
            0.0
        );
    }

    #[test]
    fn our_accounts() {
        let c = cfg(None);
        assert!(c.is_our_account("VOTE"));
        assert!(c.is_our_account("ID"));
        assert!(!c.is_our_account("PW")); // personal wallet is special
        assert!(!c.is_our_account("random"));
    }

    #[test]
    fn business_start_month_uses_bootstrap_month() {
        let c = cfg(None);
        assert_eq!(c.business_start_date(), NaiveDate::from_ymd_opt(2025, 11, 1).unwrap());
        assert_eq!(c.business_start_month(), "2025-11");
    }
}
