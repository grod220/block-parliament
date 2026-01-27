//! Position tracking and balance snapshots
//!
//! Tracks the current state of validator accounts ("where is the money NOW?")
//! to complement the income/expense tracking ("where did the money come from?").
//!
//! Key design decisions:
//! - All amounts stored as u64 lamports to avoid f64 precision issues
//! - SOL/USD values computed at display time only
//! - Atomic balance fetching via getMultipleAccounts for snapshot consistency
//! - Explicit tracking of rent-exempt reserves (not withdrawable)
//! - Stake accounts separated into liquid vs locked categories
//! - Proper stake account state parsing using StakeStateV2

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType};
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::str::FromStr;

use crate::config::Config;
use crate::constants;

// =============================================================================
// Stake Account Deserialization Types
// =============================================================================
// These mirror the Solana stake program's account layout for parsing.
// The stake program uses bincode serialization.

type Epoch = u64;

/// Lockup configuration
/// Fields must match Solana stake program layout for bincode deserialization
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // Fields used for bincode deserialization layout
struct Lockup {
    unix_timestamp: i64,
    epoch: Epoch,
    custodian: Pubkey,
}

/// Authorized staker/withdrawer
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // Fields used for bincode deserialization layout
struct Authorized {
    staker: Pubkey,
    withdrawer: Pubkey,
}

/// Stake account metadata
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // Fields used for bincode deserialization layout
struct Meta {
    rent_exempt_reserve: u64,
    authorized: Authorized,
    lockup: Lockup,
}

/// Delegation info
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // Fields used for bincode deserialization layout
struct Delegation {
    voter_pubkey: Pubkey,
    stake: u64,
    activation_epoch: Epoch,
    deactivation_epoch: Epoch,
    warmup_cooldown_rate: f64,
}

/// Stake info (delegation + credits)
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)] // Fields used for bincode deserialization layout
struct StakeData {
    delegation: Delegation,
    credits_observed: u64,
}

// =============================================================================
// Account Types
// =============================================================================

/// Type of account being tracked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccountType {
    VoteAccount,
    Identity,
    WithdrawAuthority,
    JitosolTokenAccount,
    StakeAccount,
    PersonalWallet,
}

impl std::fmt::Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountType::VoteAccount => write!(f, "VoteAccount"),
            AccountType::Identity => write!(f, "Identity"),
            AccountType::WithdrawAuthority => write!(f, "WithdrawAuthority"),
            AccountType::JitosolTokenAccount => write!(f, "JitoSOL"),
            AccountType::StakeAccount => write!(f, "StakeAccount"),
            AccountType::PersonalWallet => write!(f, "PersonalWallet"),
        }
    }
}

impl FromStr for AccountType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "VoteAccount" => Ok(AccountType::VoteAccount),
            "Identity" => Ok(AccountType::Identity),
            "WithdrawAuthority" => Ok(AccountType::WithdrawAuthority),
            "JitoSOL" => Ok(AccountType::JitosolTokenAccount),
            "StakeAccount" => Ok(AccountType::StakeAccount),
            "PersonalWallet" => Ok(AccountType::PersonalWallet),
            _ => anyhow::bail!("Invalid account type: {}", s),
        }
    }
}

// =============================================================================
// Stake Account State
// =============================================================================

/// State of a stake account (affects liquidity)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StakeState {
    /// Warming up - not yet earning rewards
    Activating,
    /// Fully delegated and earning rewards
    Active,
    /// Cooling down - will be withdrawable after epoch
    Deactivating,
    /// Fully withdrawable
    Inactive,
    /// Uninitialized or invalid state
    Unknown,
}

impl std::fmt::Display for StakeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StakeState::Activating => write!(f, "activating"),
            StakeState::Active => write!(f, "active"),
            StakeState::Deactivating => write!(f, "deactivating"),
            StakeState::Inactive => write!(f, "inactive"),
            StakeState::Unknown => write!(f, "unknown"),
        }
    }
}

impl FromStr for StakeState {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "activating" => Ok(StakeState::Activating),
            "active" => Ok(StakeState::Active),
            "deactivating" => Ok(StakeState::Deactivating),
            "inactive" => Ok(StakeState::Inactive),
            _ => Ok(StakeState::Unknown),
        }
    }
}

// =============================================================================
// Balance Data Structures
// =============================================================================

/// Current balance snapshot for a single account
/// All amounts in lamports (u64) for precision
#[derive(Debug, Clone, Serialize)]
pub struct AccountBalance {
    pub account: Pubkey,
    pub account_type: AccountType,
    pub balance_lamports: u64,
    pub rent_exempt_reserve: u64,
    pub withdrawable_lamports: u64,
    pub snapshot_slot: u64,
    pub snapshot_time: Option<i64>,
}

impl AccountBalance {
    /// Convert balance to SOL (for display only)
    pub fn balance_sol(&self) -> f64 {
        self.balance_lamports as f64 / constants::LAMPORTS_PER_SOL_U64 as f64
    }

    /// Convert withdrawable to SOL (for display only)
    pub fn withdrawable_sol(&self) -> f64 {
        self.withdrawable_lamports as f64 / constants::LAMPORTS_PER_SOL_U64 as f64
    }
}

/// Extended info for stake accounts
#[derive(Debug, Clone, Serialize)]
pub struct StakeAccountInfo {
    pub account: Pubkey,
    pub balance_lamports: u64,
    pub state: StakeState,
    pub voter: Option<Pubkey>,
    pub lockup_epoch: Option<u64>,
    pub is_liquid: bool,
    pub snapshot_slot: u64,
}

impl StakeAccountInfo {
    /// Convert balance to SOL (for display only)
    pub fn balance_sol(&self) -> f64 {
        self.balance_lamports as f64 / constants::LAMPORTS_PER_SOL_U64 as f64
    }
}

/// Aggregated position across all validator accounts
/// All amounts in lamports for precision
#[derive(Debug, Clone, Serialize)]
pub struct ValidatorPosition {
    pub snapshot_time: i64,
    pub snapshot_slot: u64,

    // Core account balances (lamports)
    pub vote_account_lamports: u64,
    pub vote_account_withdrawable: u64,
    pub identity_lamports: u64,
    pub withdraw_authority_lamports: u64,

    // jitoSOL (BAM rewards)
    pub jitosol_lamports: u64,
    pub jitosol_sol_rate: f64,
    pub jitosol_sol_equivalent: u64,

    // Stake accounts
    pub stake_accounts_liquid: u64,
    pub stake_accounts_locked: u64,
    pub stake_accounts_total: u64,
    pub stake_account_count: usize,

    // Totals
    pub total_liquid_lamports: u64,
    pub total_locked_lamports: u64,
    pub total_assets_lamports: u64,

    // Reconciliation inputs
    pub lifetime_income_lamports: u64,
    pub lifetime_expenses_lamports: u64,
    pub lifetime_withdrawals_lamports: u64,
    pub lifetime_deposits_lamports: u64,
    pub lst_appreciation_lamports: i64,

    // Reconciliation results
    pub net_cash_flow_lamports: i64,
    pub expected_balance_lamports: i64,
    pub reconciliation_diff_lamports: i64,
}

impl ValidatorPosition {
    /// Convert total assets to SOL (for display only)
    #[allow(dead_code)]
    pub fn total_assets_sol(&self) -> f64 {
        self.total_assets_lamports as f64 / constants::LAMPORTS_PER_SOL_U64 as f64
    }

    /// Check if reconciliation is within tolerance
    #[allow(dead_code)]
    pub fn is_reconciled(&self) -> bool {
        self.reconciliation_diff_lamports.abs() < constants::RECONCILIATION_TOLERANCE_LAMPORTS
    }
}

/// Result of reconciliation check
#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationResult {
    pub net_cash_flow_lamports: i64,
    pub lst_adjustment_lamports: i64,
    pub expected_lamports: i64,
    pub actual_lamports: u64,
    pub difference_lamports: i64,
    pub status: ReconciliationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReconciliationStatus {
    Ok,
    Variance,
}

impl std::fmt::Display for ReconciliationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconciliationStatus::Ok => write!(f, "OK"),
            ReconciliationStatus::Variance => write!(f, "VARIANCE"),
        }
    }
}

// =============================================================================
// Balance Fetching Functions
// =============================================================================

/// Fetch all core validator account balances atomically at a single slot
/// Uses getMultipleAccounts to ensure consistent snapshot
pub async fn fetch_all_balances_atomic(client: &RpcClient, config: &Config) -> Result<(Vec<AccountBalance>, u64)> {
    // Collect accounts to fetch, tracking their types
    let accounts_with_types = [
        (config.vote_account, AccountType::VoteAccount),
        (config.identity, AccountType::Identity),
        (config.withdraw_authority, AccountType::WithdrawAuthority),
    ];

    // Deduplicate by pubkey (identity may == withdraw_authority)
    let mut seen = HashSet::new();
    let unique_accounts: Vec<_> = accounts_with_types
        .iter()
        .filter(|(pubkey, _)| seen.insert(*pubkey))
        .cloned()
        .collect();

    let pubkeys: Vec<Pubkey> = unique_accounts.iter().map(|(p, _)| *p).collect();

    // Fetch atomically
    let response = client
        .get_multiple_accounts_with_commitment(&pubkeys, CommitmentConfig::confirmed())
        .context("Failed to fetch account balances")?;

    let snapshot_slot = response.context.slot;
    let block_time = client.get_block_time(snapshot_slot).ok();

    let mut balances = Vec::new();

    for (i, maybe_account) in response.value.iter().enumerate() {
        let (pubkey, account_type) = &unique_accounts[i];

        if let Some(account) = maybe_account {
            let rent_exempt = get_rent_exempt_for_type(client, *account_type)?;
            let withdrawable = account.lamports.saturating_sub(rent_exempt);

            balances.push(AccountBalance {
                account: *pubkey,
                account_type: *account_type,
                balance_lamports: account.lamports,
                rent_exempt_reserve: rent_exempt,
                withdrawable_lamports: withdrawable,
                snapshot_slot,
                snapshot_time: block_time,
            });
        }
    }

    Ok((balances, snapshot_slot))
}

/// Get rent-exempt minimum for an account type
fn get_rent_exempt_for_type(client: &RpcClient, account_type: AccountType) -> Result<u64> {
    let size = match account_type {
        AccountType::VoteAccount => constants::VOTE_ACCOUNT_SIZE,
        _ => constants::SYSTEM_ACCOUNT_SIZE,
    };

    if size == 0 {
        return Ok(0);
    }

    client
        .get_minimum_balance_for_rent_exemption(size)
        .context("Failed to get rent-exempt minimum")
}

/// Static program IDs for ATA computation (parsed once)
mod ata_programs {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;
    use std::sync::LazyLock;

    pub static SPL_TOKEN_PROGRAM: LazyLock<Pubkey> = LazyLock::new(|| {
        Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").expect("Invalid SPL Token program ID")
    });

    pub static ATA_PROGRAM: LazyLock<Pubkey> = LazyLock::new(|| {
        Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").expect("Invalid ATA program ID")
    });
}

/// Compute an Associated Token Account (ATA) address for a given owner and mint
pub fn compute_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    let (ata, _bump) = Pubkey::find_program_address(
        &[owner.as_ref(), ata_programs::SPL_TOKEN_PROGRAM.as_ref(), mint.as_ref()],
        &ata_programs::ATA_PROGRAM,
    );

    ata
}

/// Common token mints for ATA computation
pub mod token_mints {
    pub const WRAPPED_SOL: &str = "So11111111111111111111111111111111111111112";
    pub const MSOL: &str = "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So";
    pub const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
    pub const JITOSOL: &str = "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn";
}

/// Compute all common token ATAs for a given owner
/// Returns a list of ATA addresses for wSOL, mSOL, USDC, and jitoSOL
pub fn compute_common_atas(owner: &Pubkey) -> Vec<String> {
    let mints = [
        token_mints::WRAPPED_SOL,
        token_mints::MSOL,
        token_mints::USDC,
        token_mints::JITOSOL,
    ];

    mints
        .iter()
        .filter_map(|mint_str| {
            Pubkey::from_str(mint_str)
                .ok()
                .map(|mint| compute_ata(owner, &mint).to_string())
        })
        .collect()
}

/// Compute the jitoSOL ATA address for a given owner
pub fn compute_jitosol_ata(owner: &Pubkey) -> Result<Pubkey> {
    let jitosol_mint = Pubkey::from_str(constants::JITOSOL_MINT).context("Invalid JITOSOL_MINT constant")?;
    Ok(compute_ata(owner, &jitosol_mint))
}

/// Fetch jitoSOL token balance with proper error handling
/// Only suppresses "account not found" errors (ATA doesn't exist yet)
/// Returns (balance, ata_pubkey) for potential inclusion in atomic fetch
pub async fn fetch_jitosol_balance(client: &RpcClient, identity: &Pubkey) -> Result<u64> {
    let ata = compute_jitosol_ata(identity)?;

    // Handle missing ATA gracefully, but propagate other errors
    match client.get_token_account_balance(&ata) {
        Ok(b) => b.amount.parse::<u64>().context("Invalid token balance format from RPC"),
        Err(e) => {
            let err_str = e.to_string();
            // Only suppress "account not found" type errors
            if err_str.contains("could not find account")
                || err_str.contains("AccountNotFound")
                || err_str.contains("Invalid param: could not find")
            {
                Ok(0) // ATA doesn't exist yet - that's fine
            } else {
                Err(e).context("Failed to fetch jitoSOL balance")
            }
        }
    }
}

/// Jito stake pool account layout offsets
/// Based on SPL stake pool state structure
mod stake_pool_layout {
    // Stake pool discriminant is 1 byte, then:
    // account_type: u8 (1 byte) -> offset 0
    // manager: Pubkey (32 bytes) -> offset 1
    // staker: Pubkey (32 bytes) -> offset 33
    // stake_deposit_authority: Pubkey (32 bytes) -> offset 65
    // stake_withdraw_bump_seed: u8 (1 byte) -> offset 97
    // validator_list: Pubkey (32 bytes) -> offset 98
    // reserve_stake: Pubkey (32 bytes) -> offset 130
    // pool_mint: Pubkey (32 bytes) -> offset 162
    // manager_fee_account: Pubkey (32 bytes) -> offset 194
    // token_program_id: Pubkey (32 bytes) -> offset 226
    // total_lamports: u64 (8 bytes) -> offset 258
    // pool_token_supply: u64 (8 bytes) -> offset 266

    pub const TOTAL_LAMPORTS_OFFSET: usize = 258;
    pub const POOL_TOKEN_SUPPLY_OFFSET: usize = 266;
    pub const MIN_SIZE: usize = 274; // Minimum size to read both values
}

/// Fetch current jitoSOL to SOL exchange rate from Jito stake pool
/// Returns the rate: 1 jitoSOL = rate SOL
/// Parses the stake pool account directly to get total_lamports / pool_token_supply
pub async fn fetch_jitosol_exchange_rate(client: &RpcClient) -> Result<f64> {
    let stake_pool = Pubkey::from_str(constants::JITO_STAKE_POOL).context("Invalid JITO_STAKE_POOL constant")?;

    let account = match client.get_account(&stake_pool) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "Warning: Failed to fetch Jito stake pool account, using 1.0 rate: {}",
                e
            );
            return Ok(1.0);
        }
    };

    // Parse stake pool state to get total_lamports / pool_token_supply
    let data = &account.data;

    if data.len() < stake_pool_layout::MIN_SIZE {
        eprintln!(
            "Warning: Jito stake pool account too small ({} bytes), using 1.0 rate",
            data.len()
        );
        return Ok(1.0);
    }

    // Read total_lamports (u64 little-endian at offset 258)
    let total_lamports = u64::from_le_bytes(
        data[stake_pool_layout::TOTAL_LAMPORTS_OFFSET..stake_pool_layout::TOTAL_LAMPORTS_OFFSET + 8]
            .try_into()
            .context("Failed to read total_lamports")?,
    );

    // Read pool_token_supply (u64 little-endian at offset 266)
    let pool_token_supply = u64::from_le_bytes(
        data[stake_pool_layout::POOL_TOKEN_SUPPLY_OFFSET..stake_pool_layout::POOL_TOKEN_SUPPLY_OFFSET + 8]
            .try_into()
            .context("Failed to read pool_token_supply")?,
    );

    // Calculate rate: total_lamports / pool_token_supply
    // This gives us how many lamports each pool token is worth
    if pool_token_supply == 0 {
        eprintln!("Warning: Jito stake pool has zero supply, using 1.0 rate");
        return Ok(1.0);
    }

    let rate = total_lamports as f64 / pool_token_supply as f64;

    // Sanity check: rate should be between 0.9 and 2.0 for a healthy stake pool
    if !(0.9..=2.0).contains(&rate) {
        eprintln!(
            "Warning: Jito stake pool rate {} looks suspicious (total_lamports={}, supply={}), using 1.0",
            rate, total_lamports, pool_token_supply
        );
        return Ok(1.0);
    }

    Ok(rate)
}

/// Discover stake accounts owned by the validator's withdraw authority
/// Returns stake accounts with properly parsed state, voter, lockup, and liquidity
pub async fn discover_stake_accounts(
    client: &RpcClient,
    withdraw_authority: &Pubkey,
    snapshot_slot: u64,
) -> Result<Vec<StakeAccountInfo>> {
    // Filter for stake accounts where withdraw authority matches
    // Stake account layout (after 4-byte enum discriminant):
    //   Meta.rent_exempt_reserve: u64 (8 bytes) -> offset 4-12
    //   Meta.authorized.staker: Pubkey (32 bytes) -> offset 12-44
    //   Meta.authorized.withdrawer: Pubkey (32 bytes) -> offset 44-76
    let filters = vec![RpcFilterType::Memcmp(Memcmp::new(
        44, // Offset of withdrawer in stake account
        MemcmpEncodedBytes::Base58(withdraw_authority.to_string()),
    ))];

    let config = RpcProgramAccountsConfig {
        filters: Some(filters),
        account_config: RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        },
        ..Default::default()
    };

    // Stake program ID
    let stake_program_id =
        Pubkey::from_str("Stake11111111111111111111111111111111111111").context("Invalid stake program ID")?;

    #[allow(deprecated)]
    let accounts = client
        .get_program_accounts_with_config(&stake_program_id, config)
        .context("Failed to fetch stake accounts")?;

    // Get current epoch for determining stake state
    let epoch_info = client.get_epoch_info().context("Failed to get epoch info")?;
    let current_epoch = epoch_info.epoch;

    let mut stake_accounts = Vec::new();

    for (pubkey, account) in accounts {
        match parse_stake_account(&account.data, current_epoch) {
            Ok(info) => {
                stake_accounts.push(StakeAccountInfo {
                    account: pubkey,
                    balance_lamports: account.lamports,
                    state: info.state,
                    voter: info.voter,
                    lockup_epoch: info.lockup_epoch,
                    is_liquid: info.is_liquid,
                    snapshot_slot,
                });
            }
            Err(e) => {
                // Log but don't fail - include as unknown state
                eprintln!("Warning: Failed to parse stake account {}: {}", pubkey, e);
                stake_accounts.push(StakeAccountInfo {
                    account: pubkey,
                    balance_lamports: account.lamports,
                    state: StakeState::Unknown,
                    voter: None,
                    lockup_epoch: None,
                    is_liquid: false, // Conservative: assume locked
                    snapshot_slot,
                });
            }
        }
    }

    Ok(stake_accounts)
}

/// Parsed stake account information
struct ParsedStakeInfo {
    state: StakeState,
    voter: Option<Pubkey>,
    lockup_epoch: Option<u64>,
    is_liquid: bool,
}

/// Parse stake account data to determine state and liquidity
fn parse_stake_account(data: &[u8], current_epoch: Epoch) -> Result<ParsedStakeInfo> {
    if data.len() < 4 {
        anyhow::bail!("Stake account data too short: {} bytes", data.len());
    }

    // First 4 bytes are the enum discriminant (u32 little-endian)
    let discriminant = u32::from_le_bytes(data[0..4].try_into().unwrap());

    match discriminant {
        0 => {
            // Uninitialized
            Ok(ParsedStakeInfo {
                state: StakeState::Unknown,
                voter: None,
                lockup_epoch: None,
                is_liquid: false,
            })
        }
        1 => {
            // Initialized - has Meta but no delegation
            let meta: Meta =
                bincode::deserialize(&data[4..]).context("Failed to deserialize Initialized stake state")?;

            let is_liquid = !is_locked(&meta, current_epoch);
            Ok(ParsedStakeInfo {
                state: StakeState::Inactive,
                voter: None,
                lockup_epoch: if meta.lockup.epoch > 0 {
                    Some(meta.lockup.epoch)
                } else {
                    None
                },
                is_liquid,
            })
        }
        2 => {
            // Stake - has Meta + Stake + Flags
            // Meta size: 8 (rent) + 64 (authorized) + 48 (lockup) = 120 bytes
            // After the 4-byte discriminant, parse Meta
            let meta: Meta = bincode::deserialize(&data[4..]).context("Failed to deserialize Stake state meta")?;

            // StakeData comes after Meta (offset 4 + 120 = 124)
            let stake_offset = 4 + 120; // discriminant + Meta size
            if data.len() < stake_offset + 8 {
                anyhow::bail!("Stake account data too short for stake data");
            }

            let stake_data: StakeData =
                bincode::deserialize(&data[stake_offset..]).context("Failed to deserialize Stake state delegation")?;

            let (state, is_liquid) = determine_stake_state(&meta, &stake_data, current_epoch);
            Ok(ParsedStakeInfo {
                state,
                voter: Some(stake_data.delegation.voter_pubkey),
                lockup_epoch: if meta.lockup.epoch > 0 {
                    Some(meta.lockup.epoch)
                } else {
                    None
                },
                is_liquid,
            })
        }
        3 => {
            // RewardsPool
            Ok(ParsedStakeInfo {
                state: StakeState::Unknown,
                voter: None,
                lockup_epoch: None,
                is_liquid: false,
            })
        }
        _ => {
            anyhow::bail!("Unknown stake state discriminant: {}", discriminant);
        }
    }
}

/// Check if stake account is locked based on lockup configuration
fn is_locked(meta: &Meta, current_epoch: Epoch) -> bool {
    // Lockup is in force if epoch hasn't passed
    // Note: We ignore unix_timestamp lockup for simplicity (most validators don't use it)
    meta.lockup.epoch > current_epoch
}

/// Determine stake state and liquidity based on delegation epochs
fn determine_stake_state(meta: &Meta, stake: &StakeData, current_epoch: Epoch) -> (StakeState, bool) {
    let delegation = &stake.delegation;

    // Check if locked
    let locked = is_locked(meta, current_epoch);

    // Determine state based on activation/deactivation epochs
    if delegation.deactivation_epoch != Epoch::MAX {
        // Deactivating or fully deactivated
        if current_epoch >= delegation.deactivation_epoch {
            // Fully deactivated - liquid if not locked
            (StakeState::Inactive, !locked)
        } else {
            // Still deactivating - not liquid
            (StakeState::Deactivating, false)
        }
    } else if delegation.activation_epoch == Epoch::MAX {
        // Not yet activated
        (StakeState::Inactive, !locked)
    } else if current_epoch < delegation.activation_epoch {
        // Activation hasn't started yet
        (StakeState::Activating, false)
    } else {
        // Check if fully activated (warmup complete)
        // Simplified: if activation epoch has passed, consider active
        // Full implementation would check effective stake vs delegated stake
        if current_epoch > delegation.activation_epoch {
            (StakeState::Active, false) // Active stake is locked
        } else {
            (StakeState::Activating, false)
        }
    }
}

// =============================================================================
// Position Building
// =============================================================================

/// Input data for building a position snapshot
pub struct IncomeData {
    pub total_income_lamports: u64,
    pub total_expenses_lamports: u64,
    pub total_withdrawals_lamports: u64,
    pub total_deposits_lamports: u64,
}

/// Build a complete position snapshot from fetched data
/// Uses saturating arithmetic to prevent overflow
pub fn build_position_snapshot(
    balances: &[AccountBalance],
    stake_accounts: &[StakeAccountInfo],
    jitosol_lamports: u64,
    jitosol_rate: f64,
    income_data: &IncomeData,
    snapshot_slot: u64,
    snapshot_time: i64,
) -> ValidatorPosition {
    // Extract balances by type (handling deduplication)
    let mut seen = HashSet::new();
    let mut vote_lamports = 0u64;
    let mut vote_withdrawable = 0u64;
    let mut identity_lamports = 0u64;
    let mut withdraw_auth_lamports = 0u64;

    for balance in balances {
        if !seen.insert(balance.account) {
            continue; // Skip duplicates
        }

        match balance.account_type {
            AccountType::VoteAccount => {
                vote_lamports = balance.balance_lamports;
                vote_withdrawable = balance.withdrawable_lamports;
            }
            AccountType::Identity => {
                identity_lamports = balance.balance_lamports;
            }
            AccountType::WithdrawAuthority => {
                withdraw_auth_lamports = balance.balance_lamports;
            }
            _ => {}
        }
    }

    // Aggregate stake accounts using saturating arithmetic
    let mut stake_liquid = 0u64;
    let mut stake_locked = 0u64;

    for stake in stake_accounts {
        if stake.is_liquid {
            stake_liquid = stake_liquid.saturating_add(stake.balance_lamports);
        } else {
            stake_locked = stake_locked.saturating_add(stake.balance_lamports);
        }
    }

    let stake_total = stake_liquid.saturating_add(stake_locked);

    // jitoSOL equivalent in lamports (with overflow protection)
    let jitosol_sol_equivalent = (jitosol_lamports as f64 * jitosol_rate).min(u64::MAX as f64) as u64;

    // Totals using saturating arithmetic
    // jitoSOL is liquid (can be unstaked at any time via Jito pool)
    let total_liquid = vote_withdrawable
        .saturating_add(identity_lamports)
        .saturating_add(withdraw_auth_lamports)
        .saturating_add(stake_liquid)
        .saturating_add(jitosol_sol_equivalent); // Include jitoSOL in liquid

    // Locked = vote account rent-exempt portion + locked stake
    let vote_locked = vote_lamports.saturating_sub(vote_withdrawable);
    let total_locked = vote_locked.saturating_add(stake_locked);

    // Total assets = all SOL + jitoSOL equivalent
    let total_assets = vote_lamports
        .saturating_add(identity_lamports)
        .saturating_add(withdraw_auth_lamports)
        .saturating_add(stake_total)
        .saturating_add(jitosol_sol_equivalent);

    // Reconciliation: net_cash_flow = income - expenses - withdrawals + deposits
    // Use i128 for intermediate calculation to prevent overflow
    let net_cash_flow = (income_data.total_income_lamports as i128)
        .saturating_sub(income_data.total_expenses_lamports as i128)
        .saturating_sub(income_data.total_withdrawals_lamports as i128)
        .saturating_add(income_data.total_deposits_lamports as i128);

    // Clamp to i64 range
    let net_cash_flow = net_cash_flow.clamp(i64::MIN as i128, i64::MAX as i128) as i64;

    // LST appreciation would need historical tracking to compute accurately
    // For now, set to 0 and document the limitation
    let lst_appreciation: i64 = 0;

    let expected = net_cash_flow.saturating_add(lst_appreciation);

    // Safe conversion of total_assets to i64 for reconciliation diff
    let total_assets_i64 = if total_assets > i64::MAX as u64 {
        i64::MAX
    } else {
        total_assets as i64
    };
    let reconciliation_diff = total_assets_i64.saturating_sub(expected);

    ValidatorPosition {
        snapshot_time,
        snapshot_slot,
        vote_account_lamports: vote_lamports,
        vote_account_withdrawable: vote_withdrawable,
        identity_lamports,
        withdraw_authority_lamports: withdraw_auth_lamports,
        jitosol_lamports,
        jitosol_sol_rate: jitosol_rate,
        jitosol_sol_equivalent,
        stake_accounts_liquid: stake_liquid,
        stake_accounts_locked: stake_locked,
        stake_accounts_total: stake_total,
        stake_account_count: stake_accounts.len(),
        total_liquid_lamports: total_liquid,
        total_locked_lamports: total_locked,
        total_assets_lamports: total_assets,
        lifetime_income_lamports: income_data.total_income_lamports,
        lifetime_expenses_lamports: income_data.total_expenses_lamports,
        lifetime_withdrawals_lamports: income_data.total_withdrawals_lamports,
        lifetime_deposits_lamports: income_data.total_deposits_lamports,
        lst_appreciation_lamports: lst_appreciation,
        net_cash_flow_lamports: net_cash_flow,
        expected_balance_lamports: expected,
        reconciliation_diff_lamports: reconciliation_diff,
    }
}

/// Perform reconciliation check
pub fn reconcile(position: &ValidatorPosition) -> ReconciliationResult {
    let status = if position.reconciliation_diff_lamports.abs() < constants::RECONCILIATION_TOLERANCE_LAMPORTS {
        ReconciliationStatus::Ok
    } else {
        ReconciliationStatus::Variance
    };

    ReconciliationResult {
        net_cash_flow_lamports: position.net_cash_flow_lamports,
        lst_adjustment_lamports: position.lst_appreciation_lamports,
        expected_lamports: position.expected_balance_lamports,
        actual_lamports: position.total_assets_lamports,
        difference_lamports: position.reconciliation_diff_lamports,
        status,
    }
}

// =============================================================================
// Display Helpers
// =============================================================================

/// Format lamports as SOL with specified decimal places
pub fn lamports_to_sol_string(lamports: u64, decimals: usize) -> String {
    let sol = lamports as f64 / constants::LAMPORTS_PER_SOL_U64 as f64;
    format!("{:.decimals$}", sol, decimals = decimals)
}

/// Format signed lamports as SOL
pub fn signed_lamports_to_sol_string(lamports: i64, decimals: usize) -> String {
    let sol = lamports as f64 / constants::LAMPORTS_PER_SOL_U64 as f64;
    format!("{:+.decimals$}", sol, decimals = decimals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lamports_to_sol() {
        assert_eq!(lamports_to_sol_string(1_000_000_000, 4), "1.0000");
        assert_eq!(lamports_to_sol_string(1_500_000_000, 2), "1.50");
        assert_eq!(lamports_to_sol_string(123_456_789, 6), "0.123457");
    }

    #[test]
    fn test_reconciliation_status() {
        // Within tolerance
        let position = ValidatorPosition {
            reconciliation_diff_lamports: 50_000, // 0.00005 SOL
            ..default_position()
        };
        assert!(position.is_reconciled());

        // Outside tolerance
        let position = ValidatorPosition {
            reconciliation_diff_lamports: 1_000_000, // 0.001 SOL
            ..default_position()
        };
        assert!(!position.is_reconciled());
    }

    fn default_position() -> ValidatorPosition {
        ValidatorPosition {
            snapshot_time: 0,
            snapshot_slot: 0,
            vote_account_lamports: 0,
            vote_account_withdrawable: 0,
            identity_lamports: 0,
            withdraw_authority_lamports: 0,
            jitosol_lamports: 0,
            jitosol_sol_rate: 1.0,
            jitosol_sol_equivalent: 0,
            stake_accounts_liquid: 0,
            stake_accounts_locked: 0,
            stake_accounts_total: 0,
            stake_account_count: 0,
            total_liquid_lamports: 0,
            total_locked_lamports: 0,
            total_assets_lamports: 0,
            lifetime_income_lamports: 0,
            lifetime_expenses_lamports: 0,
            lifetime_withdrawals_lamports: 0,
            lifetime_deposits_lamports: 0,
            lst_appreciation_lamports: 0,
            net_cash_flow_lamports: 0,
            expected_balance_lamports: 0,
            reconciliation_diff_lamports: 0,
        }
    }
}
