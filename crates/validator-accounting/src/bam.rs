//! Jito BAM (Block Assembly Marketplace) reward tracking via Jito API
//!
//! BAM rewards are jitoSOL tokens claimed via the JIP-31 program. Unlike MEV tips
//! (which go to the vote account as SOL), BAM rewards go to the validator's
//! identity's associated jitoSOL token account.
//!
//! Per JIP-31:
//! - Rewards accumulate per epoch based on "effective stake"
//! - 10-epoch claim window before expiration
//! - First available starting epoch 912-913

use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

use crate::config::Config;
use crate::constants;
use crate::transactions::epoch_to_date;

/// BAM claim for a single epoch
///
/// Uses u64 lamports for precision (jitoSOL has 9 decimals like SOL).
/// The SOL equivalent is computed using the jitoSOL/SOL exchange rate.
#[derive(Debug, Clone)]
pub struct BamClaim {
    /// Epoch when rewards were earned
    pub epoch: u64,
    /// Amount in jitoSOL lamports (raw, 9 decimals)
    pub amount_jitosol_lamports: u64,
    /// SOL equivalent value (computed from rate)
    pub amount_sol_equivalent: f64,
    /// jitoSOL to SOL exchange rate used for conversion (for audit trail)
    pub jitosol_sol_rate: Option<f64>,
    /// When the claim transaction occurred (ISO8601)
    pub claimed_at: Option<String>,
    /// Transaction signature (unique identifier for claim)
    pub tx_signature: String,
    /// Epoch end date (for accrual-basis reporting)
    pub date: Option<String>,
}

/// Raw API response from Jito BAM claim endpoint
///
/// The API returns claim eligibility data per epoch, not historical claims.
/// We determine if claimed by checking if claim_status_address exists on-chain.
#[derive(Debug, Deserialize)]
struct JitoBamApiResponse {
    /// Amount in jitoSOL lamports available/claimed
    amount: u64,
    /// Validator identity (claimant)
    claimant: String,
    /// Merkle proof for claiming (empty array if no rewards)
    /// Kept for potential on-chain verification use
    #[serde(default)]
    #[allow(dead_code)]
    proof: Vec<Vec<u8>>,
    /// Distributor PDA address
    /// Kept for potential on-chain verification use
    #[serde(default)]
    #[allow(dead_code)]
    distributor_address: String,
    /// Claim status PDA - if this exists on-chain, rewards were claimed
    #[serde(default)]
    claim_status_address: String,
}

/// Fetch BAM claims from Jito API for a range of epochs
///
/// Important: The `identity` in config must be the validator IDENTITY pubkey,
/// not the vote account. Using the wrong value will return empty results.
///
/// Only returns actually claimed rewards (where claim_status_address exists).
/// Unclaimed eligibility is skipped to prevent double-counting.
pub async fn fetch_bam_claims(config: &Config, start_epoch: u64, end_epoch: u64) -> Result<Vec<BamClaim>> {
    // Configure client with explicit timeout to prevent hangs
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;
    let mut all_claims = Vec::new();
    let mut failed_epochs = Vec::new();

    // Use config's bam_first_epoch (allows user override), with BAM_FIRST_EPOCH as floor
    let effective_start = start_epoch.max(config.bam_first_epoch).max(constants::BAM_FIRST_EPOCH);

    if effective_start > end_epoch {
        return Ok(all_claims);
    }

    println!(
        "    Querying Jito BAM API for epochs {}-{}...",
        effective_start, end_epoch
    );

    // Query each epoch (API is per-epoch based on plan)
    for epoch in effective_start..=end_epoch {
        match fetch_bam_claim_for_epoch(&client, config, epoch).await {
            Ok(Some(claim)) => {
                println!(
                    "      Epoch {}: {:.6} jitoSOL (~{:.4} SOL)",
                    claim.epoch,
                    claim.amount_jitosol_lamports as f64 / 1e9,
                    claim.amount_sol_equivalent
                );
                all_claims.push(claim);
            }
            Ok(None) => {
                // No claim for this epoch (normal - not all epochs have rewards,
                // or rewards exist but haven't been claimed yet)
            }
            Err(e) => {
                // Track failures for summary reporting
                failed_epochs.push((epoch, e.to_string()));
            }
        }

        // Rate limiting between epoch queries
        sleep(Duration::from_millis(100)).await;
    }

    // Report failures as a summary (not per-epoch spam)
    if !failed_epochs.is_empty() {
        eprintln!(
            "    Warning: Failed to fetch BAM data for {} epochs: {:?}",
            failed_epochs.len(),
            failed_epochs.iter().map(|(e, _)| e).collect::<Vec<_>>()
        );
    }

    // Sort by epoch
    all_claims.sort_by_key(|c| c.epoch);

    Ok(all_claims)
}

/// Fetch BAM claim for a single epoch with retry logic
///
/// Returns None if:
/// - No rewards available for this epoch (amount = 0)
/// - API returns empty claimant
/// - Rewards exist but haven't been claimed yet (empty claim_status_address)
///
/// CRITICAL: Only returns actually claimed rewards to prevent double-counting.
/// The API returns eligibility data even for unclaimed rewards. We only record
/// claims where claim_status_address is present (indicating the claim PDA exists).
async fn fetch_bam_claim_for_epoch(client: &reqwest::Client, config: &Config, epoch: u64) -> Result<Option<BamClaim>> {
    let url = format!("{}/{}/{}", constants::JITO_BAM_API_BASE, epoch, config.identity);

    // Retry with exponential backoff
    let max_retries = 3;
    let mut last_error = None;

    for attempt in 0..max_retries {
        if attempt > 0 {
            let delay = Duration::from_secs(2u64.pow(attempt as u32));
            sleep(delay).await;
        }

        match client.get(&url).header("Accept", "application/json").send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let text = response.text().await?;

                    // Handle empty response
                    if text.is_empty() || text == "null" {
                        return Ok(None);
                    }

                    // Parse the API response
                    match serde_json::from_str::<JitoBamApiResponse>(&text) {
                        Ok(api_response) => {
                            // Skip if no rewards (amount = 0 or empty claimant)
                            if api_response.amount == 0 || api_response.claimant.is_empty() {
                                return Ok(None);
                            }

                            // CRITICAL: Skip unclaimed rewards to prevent double-counting
                            // The API returns eligibility even for unclaimed epochs.
                            // Only record when claim_status_address exists (claim PDA created).
                            // This ensures cash-basis accounting and prevents duplicate entries.
                            if api_response.claim_status_address.is_empty() {
                                return Ok(None);
                            }

                            // Convert to BamClaim - claim_status_address is guaranteed non-empty
                            let claim = process_bam_api_response(epoch, api_response, config);
                            return Ok(Some(claim));
                        }
                        Err(e) => {
                            // Parse errors are not retryable (schema mismatch won't fix itself)
                            return Err(anyhow::anyhow!(
                                "Parse error: {} (response: {})",
                                e,
                                &text[..text.len().min(100)]
                            ));
                        }
                    }
                } else if response.status().as_u16() == 404 {
                    return Ok(None);
                } else if response.status().as_u16() == 429 {
                    last_error = Some(anyhow::anyhow!("Rate limited (429)"));
                    sleep(Duration::from_secs(10)).await;
                    continue;
                } else {
                    last_error = Some(anyhow::anyhow!("BAM API returned status: {}", response.status()));
                }
            }
            Err(e) => {
                last_error = Some(anyhow::anyhow!("Request failed: {}", e));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} retries", max_retries)))
}

/// Process API response into a BamClaim
///
/// Uses the jitoSOL/SOL exchange rate from config (default 1.0).
/// jitoSOL typically trades at ~1.05-1.15x SOL due to accumulated staking rewards.
/// Users can override the rate in config.toml [bam] section.
///
/// Note: claim_status_address is guaranteed to be non-empty when this is called
/// (we skip unclaimed rewards in fetch_bam_claim_for_epoch).
fn process_bam_api_response(epoch: u64, data: JitoBamApiResponse, config: &Config) -> BamClaim {
    let jitosol_amount = data.amount as f64 / 1e9;

    // Use configured rate (allows user to set realistic rate like 1.10)
    let rate = config.bam_jitosol_rate;
    let amount_sol_equivalent = jitosol_amount * rate;

    let date = epoch_to_date(epoch);

    // Use claim_status_address as unique identifier (PDA is unique per epoch per validator)
    // This is guaranteed non-empty since we skip unclaimed rewards
    let tx_signature = data.claim_status_address;

    BamClaim {
        epoch,
        amount_jitosol_lamports: data.amount,
        amount_sol_equivalent,
        jitosol_sol_rate: Some(rate),
        claimed_at: None, // API doesn't provide this
        tx_signature,
        date: Some(date),
    }
}

/// Get total BAM rewards in jitoSOL
pub fn total_bam_jitosol(claims: &[BamClaim]) -> f64 {
    claims.iter().map(|c| c.amount_jitosol_lamports as f64 / 1e9).sum()
}

/// Get total BAM rewards in SOL equivalent
pub fn total_bam_sol_equivalent(claims: &[BamClaim]) -> f64 {
    claims.iter().map(|c| c.amount_sol_equivalent).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a minimal test config for BAM tests
    fn test_config() -> Config {
        use solana_sdk::pubkey::Pubkey;
        Config {
            vote_account: Pubkey::new_unique(),
            identity: Pubkey::new_unique(),
            withdraw_authority: Pubkey::new_unique(),
            personal_wallet: Pubkey::new_unique(),
            rpc_url: "https://test.rpc".to_string(),
            coingecko_api_key: "test".to_string(),
            dune_api_key: None,
            commission_percent: 5,
            first_reward_epoch: 900,
            sfdp_acceptance_date: None,
            bootstrap_date: "2025-01-01".to_string(),
            bam_enabled: true,
            bam_first_epoch: 912,
            bam_jitosol_rate: 1.0, // Default rate
        }
    }

    #[test]
    fn test_process_bam_api_response() {
        let config = test_config();
        let response = JitoBamApiResponse {
            amount: 1_304_802_961, // ~1.3 jitoSOL
            claimant: "mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e".to_string(),
            proof: vec![],
            distributor_address: "2hMorPktS1tD6997BXkjwqQKRMY2oEfeEjvFoBpfTbL3".to_string(),
            claim_status_address: "9H1JAihZbL1UixdLraraL6nuvhJyqF5hxStQTQgYa7mz".to_string(),
        };

        let claim = process_bam_api_response(913, response, &config);

        assert_eq!(claim.epoch, 913);
        assert_eq!(claim.amount_jitosol_lamports, 1_304_802_961);
        // Uses configured 1.0 rate
        assert!((claim.amount_sol_equivalent - 1.304802961).abs() < 0.001);
        assert_eq!(claim.tx_signature, "9H1JAihZbL1UixdLraraL6nuvhJyqF5hxStQTQgYa7mz");
        assert_eq!(claim.jitosol_sol_rate, Some(1.0));
    }

    #[test]
    fn test_process_bam_api_response_custom_rate() {
        let mut config = test_config();
        config.bam_jitosol_rate = 1.10; // Realistic rate

        let response = JitoBamApiResponse {
            amount: 1_000_000_000, // 1.0 jitoSOL
            claimant: "mD1afZhSisoXfJLT8nYwSFANqjr1KPoDUEpYTEfFX1e".to_string(),
            proof: vec![],
            distributor_address: "".to_string(),
            claim_status_address: "ClaimPDA123".to_string(),
        };

        let claim = process_bam_api_response(914, response, &config);

        // Should use custom 1.10 rate
        assert!((claim.amount_sol_equivalent - 1.10).abs() < 0.001);
        assert_eq!(claim.jitosol_sol_rate, Some(1.10));
    }

    #[test]
    fn test_total_bam_jitosol() {
        let claims = vec![
            BamClaim {
                epoch: 913,
                amount_jitosol_lamports: 1_000_000_000,
                amount_sol_equivalent: 1.0,
                jitosol_sol_rate: Some(1.0),
                claimed_at: None,
                tx_signature: "a".to_string(),
                date: Some("2025-01-15".to_string()),
            },
            BamClaim {
                epoch: 914,
                amount_jitosol_lamports: 500_000_000,
                amount_sol_equivalent: 0.5,
                jitosol_sol_rate: Some(1.0),
                claimed_at: None,
                tx_signature: "b".to_string(),
                date: Some("2025-01-17".to_string()),
            },
        ];

        assert!((total_bam_jitosol(&claims) - 1.5).abs() < 0.001);
        assert!((total_bam_sol_equivalent(&claims) - 1.5).abs() < 0.001);
    }
}
