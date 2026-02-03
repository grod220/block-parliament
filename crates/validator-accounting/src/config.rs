//! Configuration for the validator financial tracker

use anyhow::{Context, Result};
use chrono::Datelike;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::str::FromStr;

use crate::constants;

// =============================================================================
// File-based Configuration (config.toml)
// =============================================================================

/// Configuration loaded from config.toml
#[derive(Debug, Deserialize)]
pub struct FileConfig {
    pub validator: ValidatorConfig,
    pub api_keys: ApiKeys,
    #[serde(default)]
    pub notion: Option<NotionConfig>,
    #[serde(default)]
    pub bam: Option<BamConfig>,
    #[serde(default)]
    pub doublezero: Option<DoubleZeroConfig>,
}

/// Jito BAM (Block Assembly Marketplace) configuration
#[derive(Debug, Clone, Deserialize)]
pub struct BamConfig {
    /// Enable BAM reward tracking (default: true if section present)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// First epoch to check for BAM rewards (default: 912)
    #[serde(default = "default_bam_first_epoch")]
    pub first_epoch: u64,
    /// jitoSOL to SOL exchange rate for valuation (default: 1.0)
    /// jitoSOL typically trades at ~1.05-1.15x SOL. Set to 1.0 for conservative
    /// accounting, or a realistic value like 1.10 for more accurate reporting.
    #[serde(default = "default_jitosol_rate")]
    pub jitosol_rate: f64,
}

/// DoubleZero fee configuration (block reward sharing)
#[derive(Debug, Clone, Deserialize)]
pub struct DoubleZeroConfig {
    /// Enable DoubleZero fee tracking (default: true if section present)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Fee rate (default: 0.05 = 5%)
    #[serde(default = "default_doublezero_fee_rate")]
    pub fee_rate: f64,
    /// First epoch with DoubleZero fees (default: 859)
    #[serde(default = "default_doublezero_first_epoch")]
    pub first_epoch: u64,
    /// Deposit account PDA (optional, used for payment categorization)
    #[serde(default)]
    pub deposit_account: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_bam_first_epoch() -> u64 {
    crate::constants::BAM_FIRST_EPOCH
}

fn default_jitosol_rate() -> f64 {
    1.0 // Conservative default
}

fn default_doublezero_fee_rate() -> f64 {
    constants::DOUBLEZERO_FEE_RATE
}

fn default_doublezero_first_epoch() -> u64 {
    constants::DOUBLEZERO_FIRST_EPOCH
}

/// Validator-specific configuration
#[derive(Debug, Deserialize)]
pub struct ValidatorConfig {
    /// Vote account address
    pub vote_account: String,
    /// Identity account address
    pub identity: String,
    /// Withdraw authority address
    pub withdraw_authority: String,
    /// Personal wallet address (for categorizing seeding/withdrawals)
    pub personal_wallet: String,
    /// Commission percentage (0-100)
    pub commission_percent: u8,
    /// First epoch with staking rewards
    pub first_reward_epoch: u64,
    /// Bootstrap date (when validator was set up)
    pub bootstrap_date: String,
    /// SFDP acceptance date (optional - only if in SFDP program)
    #[serde(default)]
    pub sfdp_acceptance_date: Option<String>,
}

/// API keys section
#[derive(Debug, Deserialize)]
pub struct ApiKeys {
    pub helius: String,
    pub coingecko: String,
    #[serde(default)]
    pub dune: Option<String>,
}

/// Notion integration configuration
#[derive(Debug, Clone, Deserialize)]
pub struct NotionConfig {
    pub api_token: String,
    pub hours_database_id: String,
}

impl FileConfig {
    /// Load configuration from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("Failed to read config file: {}", path.display()))?;

        toml::from_str(&content).with_context(|| {
            "Failed to parse config.toml. Check for:\n\
             - Missing required fields (validator.vote_account, validator.identity, etc.)\n\
             - Invalid TOML syntax (missing quotes, brackets, etc.)\n\
             - Incorrect data types (strings vs numbers)\n\n\
             See config.toml.example for the expected format."
        })
    }
}

// =============================================================================
// Runtime Configuration
// =============================================================================

/// Main configuration struct with parsed values
pub struct Config {
    /// Vote account pubkey
    pub vote_account: Pubkey,
    /// Validator identity pubkey
    pub identity: Pubkey,
    /// Withdraw authority pubkey
    pub withdraw_authority: Pubkey,
    /// Personal wallet (for detecting seeding transactions)
    pub personal_wallet: Pubkey,
    /// RPC URL
    pub rpc_url: String,
    /// CoinGecko API key
    pub coingecko_api_key: String,
    /// Dune Analytics API key (optional, for backfilling pruned data)
    #[allow(dead_code)]
    pub dune_api_key: Option<String>,
    /// Commission percentage
    pub commission_percent: u8,
    /// First epoch with rewards
    pub first_reward_epoch: u64,
    /// SFDP acceptance date (for calculating coverage schedule)
    pub sfdp_acceptance_date: Option<String>,
    /// Bootstrap date (for finding initial seeding)
    pub bootstrap_date: String,
    /// BAM reward tracking enabled
    pub bam_enabled: bool,
    /// First epoch to check for BAM rewards
    pub bam_first_epoch: u64,
    /// jitoSOL to SOL exchange rate for BAM reward valuation
    pub bam_jitosol_rate: f64,
    /// DoubleZero fee tracking enabled
    pub doublezero_enabled: bool,
    /// DoubleZero fee rate (e.g., 0.05 = 5%)
    pub doublezero_fee_rate: f64,
    /// First epoch with DoubleZero fees
    pub doublezero_first_epoch: u64,
    /// DoubleZero deposit account PDA (optional)
    pub doublezero_deposit_account: Option<Pubkey>,
}

impl Config {
    /// Create config from file config and optional RPC URL override
    pub fn from_file(file_config: &FileConfig, rpc_url: Option<String>) -> Result<Self> {
        let validator = &file_config.validator;

        // BAM config defaults
        let (bam_enabled, bam_first_epoch, bam_jitosol_rate) = match &file_config.bam {
            Some(bam) => (bam.enabled, bam.first_epoch, bam.jitosol_rate),
            None => (true, constants::BAM_FIRST_EPOCH, 1.0), // Enabled by default, conservative rate
        };

        // DoubleZero config defaults (disabled unless section present)
        let (doublezero_enabled, doublezero_fee_rate, doublezero_first_epoch, doublezero_deposit_account) =
            match &file_config.doublezero {
                Some(dz) => {
                    let deposit = match dz.deposit_account.as_deref() {
                        Some(addr) => Some(
                            Pubkey::from_str(addr).with_context(|| "Invalid doublezero.deposit_account address")?,
                        ),
                        None => None,
                    };
                    (dz.enabled, dz.fee_rate, dz.first_epoch, deposit)
                }
                None => (false, constants::DOUBLEZERO_FEE_RATE, constants::DOUBLEZERO_FIRST_EPOCH, None),
            };

        Ok(Self {
            // Parse validator addresses from config
            vote_account: Pubkey::from_str(&validator.vote_account).with_context(|| "Invalid vote_account address")?,
            identity: Pubkey::from_str(&validator.identity).with_context(|| "Invalid identity address")?,
            withdraw_authority: Pubkey::from_str(&validator.withdraw_authority)
                .with_context(|| "Invalid withdraw_authority address")?,
            personal_wallet: Pubkey::from_str(&validator.personal_wallet)
                .with_context(|| "Invalid personal_wallet address")?,

            // Helius RPC endpoint (has historical transaction data)
            rpc_url: rpc_url
                .unwrap_or_else(|| format!("{}{}", constants::HELIUS_RPC_BASE, &file_config.api_keys.helius)),

            // CoinGecko API key for price lookups
            coingecko_api_key: file_config.api_keys.coingecko.clone(),

            // Dune API key for backfilling pruned data
            dune_api_key: file_config.api_keys.dune.clone(),

            // Commission rate from config
            commission_percent: validator.commission_percent,

            // First epoch where validator earned rewards
            first_reward_epoch: validator.first_reward_epoch,

            // SFDP acceptance date (optional)
            sfdp_acceptance_date: validator.sfdp_acceptance_date.clone(),

            // Bootstrap date (when validator was first set up)
            bootstrap_date: validator.bootstrap_date.clone(),

            // BAM reward tracking
            bam_enabled,
            bam_first_epoch,
            bam_jitosol_rate,

            // DoubleZero fee tracking
            doublezero_enabled,
            doublezero_fee_rate,
            doublezero_first_epoch,
            doublezero_deposit_account,
        })
    }

    /// Check if a pubkey is one of our validator accounts
    pub fn is_our_account(&self, pubkey: &Pubkey) -> bool {
        *pubkey == self.vote_account || *pubkey == self.identity || *pubkey == self.withdraw_authority
    }

    /// Check if a pubkey is any account we care about (including personal wallet)
    pub fn is_relevant_account(&self, pubkey: &Pubkey) -> bool {
        self.is_our_account(pubkey) || *pubkey == self.personal_wallet
    }

    /// Get DoubleZero fee rate as basis points (0-10000)
    pub fn doublezero_fee_rate_bps(&self) -> u64 {
        let rate = self.doublezero_fee_rate.clamp(0.0, 1.0);
        (rate * 10_000.0).round() as u64
    }

    /// Calculate SFDP vote cost coverage percentage for a given date
    /// Schedule from acceptance date:
    /// - Months 1-3: 100% coverage
    /// - Months 4-6: 75% coverage
    /// - Months 7-9: 50% coverage
    /// - Months 10-12: 25% coverage
    /// - After 12 months: 0%
    pub fn sfdp_coverage_percent(&self, date: &chrono::NaiveDate) -> f64 {
        use chrono::NaiveDate;

        let Some(ref acceptance_str) = self.sfdp_acceptance_date else {
            return 0.0; // Not in SFDP program
        };

        let Ok(acceptance) = NaiveDate::parse_from_str(acceptance_str, "%Y-%m-%d") else {
            return 0.0; // Invalid date
        };

        let months_diff = (date.year() - acceptance.year()) * 12 + (date.month() as i32 - acceptance.month() as i32);

        if months_diff < 0 {
            0.0
        } else if months_diff < 3 {
            1.0 // 100%
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
    use solana_sdk::pubkey::Pubkey;

    /// Create a minimal Config for testing SFDP calculations
    fn test_config(sfdp_date: Option<&str>) -> Config {
        Config {
            vote_account: Pubkey::new_unique(),
            identity: Pubkey::new_unique(),
            withdraw_authority: Pubkey::new_unique(),
            personal_wallet: Pubkey::new_unique(),
            rpc_url: String::new(),
            coingecko_api_key: String::new(),
            dune_api_key: None,
            commission_percent: 10,
            first_reward_epoch: 900,
            sfdp_acceptance_date: sfdp_date.map(|s| s.to_string()),
            bootstrap_date: "2025-11-01".to_string(),
            bam_enabled: true,
            bam_first_epoch: 912,
            bam_jitosol_rate: 1.0,
        }
    }

    #[test]
    fn test_sfdp_no_acceptance_date() {
        let config = test_config(None);
        let date = NaiveDate::from_ymd_opt(2025, 12, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&date), 0.0);
    }

    #[test]
    fn test_sfdp_before_acceptance() {
        let config = test_config(Some("2025-12-01"));
        let date = NaiveDate::from_ymd_opt(2025, 11, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&date), 0.0);
    }

    #[test]
    fn test_sfdp_month_1_to_3_full_coverage() {
        let config = test_config(Some("2025-12-01"));

        // Month 1 (same month as acceptance)
        let m1 = NaiveDate::from_ymd_opt(2025, 12, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m1), 1.0);

        // Month 2
        let m2 = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m2), 1.0);

        // Month 3
        let m3 = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m3), 1.0);
    }

    #[test]
    fn test_sfdp_month_4_to_6_75_percent() {
        let config = test_config(Some("2025-12-01"));

        // Month 4
        let m4 = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m4), 0.75);

        // Month 5
        let m5 = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m5), 0.75);

        // Month 6
        let m6 = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m6), 0.75);
    }

    #[test]
    fn test_sfdp_month_7_to_9_50_percent() {
        let config = test_config(Some("2025-12-01"));

        // Month 7
        let m7 = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m7), 0.50);

        // Month 9
        let m9 = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m9), 0.50);
    }

    #[test]
    fn test_sfdp_month_10_to_12_25_percent() {
        let config = test_config(Some("2025-12-01"));

        // Month 10
        let m10 = NaiveDate::from_ymd_opt(2026, 9, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m10), 0.25);

        // Month 12
        let m12 = NaiveDate::from_ymd_opt(2026, 11, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m12), 0.25);
    }

    #[test]
    fn test_sfdp_after_12_months_no_coverage() {
        let config = test_config(Some("2025-12-01"));

        // Month 13 (12 months after December 2025 = December 2026)
        let m13 = NaiveDate::from_ymd_opt(2026, 12, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&m13), 0.0);

        // Well after program ends
        let later = NaiveDate::from_ymd_opt(2027, 6, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&later), 0.0);
    }

    #[test]
    fn test_sfdp_invalid_acceptance_date() {
        let config = test_config(Some("invalid-date"));
        let date = NaiveDate::from_ymd_opt(2025, 12, 15).unwrap();
        assert_eq!(config.sfdp_coverage_percent(&date), 0.0);
    }
}
