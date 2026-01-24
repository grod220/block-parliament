use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::http::post_json_cached;

#[cfg(feature = "ssr")]
const RPC_ENDPOINT: &str = "https://api.mainnet-beta.solana.com";

/// Network comparison stats for a validator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkComparison {
    pub total_validators: usize,
    pub skip_rate_percentile: u8,
    pub stake_percentile: u8,
}

#[cfg(feature = "ssr")]
#[derive(Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    id: u32,
    method: &'static str,
    params: Vec<serde_json::Value>,
}

#[cfg(feature = "ssr")]
#[derive(Deserialize)]
struct RpcResponse {
    result: Option<VoteAccountsResult>,
}

#[cfg(feature = "ssr")]
#[derive(Deserialize)]
struct VoteAccountsResult {
    current: Vec<VoteAccount>,
    #[serde(default)]
    delinquent: Vec<VoteAccount>,
}

#[cfg(feature = "ssr")]
#[derive(Deserialize)]
struct VoteAccount {
    #[serde(rename = "activatedStake")]
    activated_stake: u64,
}

/// Fetch network comparison data using getVoteAccounts
/// Note: Skip rate percentile is estimated using a heuristic based on typical network average
#[cfg(feature = "ssr")]
pub async fn get_network_comparison(current_skip_rate: f64, current_stake: f64) -> Option<NetworkComparison> {
    let request = RpcRequest {
        jsonrpc: "2.0",
        id: 1,
        method: "getVoteAccounts",
        params: vec![serde_json::json!({"commitment": "confirmed"})],
    };

    let body = serde_json::to_string(&request).ok()?;

    // Use cached POST for RPC calls (5 minute TTL)
    let data: RpcResponse = post_json_cached(RPC_ENDPOINT, &body).await?;
    let result = data.result?;

    // Include both current and delinquent validators for accurate network stats
    let mut all_stakes: Vec<u64> = result
        .current
        .iter()
        .chain(result.delinquent.iter())
        .map(|v| v.activated_stake)
        .collect();

    let total_validators = all_stakes.len();

    if total_validators == 0 {
        return None;
    }

    // Sort descending for percentile calculation
    all_stakes.sort_by(|a, b| b.cmp(a));

    let current_stake_lamports = (current_stake * 1_000_000_000.0) as u64;

    // Find rank: position of first stake <= ours, or 0 if we have highest stake
    // Rank is 1-indexed: rank 1 = top validator
    let stake_rank = all_stakes
        .iter()
        .position(|&s| s <= current_stake_lamports)
        .map(|pos| pos + 1) // Convert 0-indexed to 1-indexed
        .unwrap_or(1); // If not found (we have highest), we're rank 1

    let stake_percentile = ((stake_rank as f64 / total_validators as f64) * 100.0).round() as u8;

    // Estimate skip rate percentile based on typical network average
    // NOTE: This is a heuristic - actual percentile would require per-validator skip rate data
    const NETWORK_AVG_SKIP_RATE: f64 = 0.2; // ~20% typical network skip rate
    let skip_rate_percentile = if current_skip_rate <= NETWORK_AVG_SKIP_RATE {
        // Better than average: 1-50 percentile (lower skip = better = lower percentile)
        ((current_skip_rate / NETWORK_AVG_SKIP_RATE) * 50.0).round() as u8
    } else {
        // Worse than average: 50-100 percentile
        (50.0 + ((current_skip_rate - NETWORK_AVG_SKIP_RATE) / NETWORK_AVG_SKIP_RATE) * 50.0).round() as u8
    };

    Some(NetworkComparison {
        total_validators,
        skip_rate_percentile: skip_rate_percentile.clamp(1, 100),
        stake_percentile: stake_percentile.clamp(1, 100),
    })
}
