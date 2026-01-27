# Position Tracking Feature Plan

## Overview

Add a "Balance Sheet" view to complement the existing "Income Statement" (P&L) tracking. This answers "where is the money NOW?" rather than just "where did it come from?"

## Problem Statement

The current system tracks:
- ✅ Revenue sources (commission, MEV, leader fees, BAM, SFDP)
- ✅ Costs (vote fees, hosting, contractors, etc.)
- ✅ Transfer movements (seeding, withdrawals, internal)

But cannot answer:
- ❌ "How much SOL is currently in the vote account?"
- ❌ "How much is in the identity account?"
- ❌ "What's my total validator-related net worth?"
- ❌ "I earned 500 SOL - where did it go?"
- ❌ Reconciliation: does (income - withdrawals) ≈ current balances?
- ❌ "How much is in my self-stake accounts?"
- ❌ "What's liquid vs locked?"

## Proposed Solution

### 1. New Data Models

```rust
// src/positions.rs

/// Current balance snapshot for an account
/// NOTE: Store lamports as u64 to avoid f64 precision issues.
/// Compute SOL/USD values at display time only.
#[derive(Debug, Clone, Serialize)]
pub struct AccountBalance {
    pub account: Pubkey,
    pub account_type: AccountType,
    pub balance_lamports: u64,
    pub rent_exempt_reserve: u64,     // Minimum balance (not withdrawable)
    pub withdrawable_lamports: u64,   // balance - rent_exempt
    pub snapshot_slot: u64,
    pub snapshot_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub enum AccountType {
    VoteAccount,
    Identity,
    WithdrawAuthority,
    JitosolTokenAccount,  // For BAM rewards
    StakeAccount,         // Self-stake or operational stake
    PersonalWallet,       // Validator-related portion
}

/// Stake account state for liquid vs locked tracking
#[derive(Debug, Clone, Serialize)]
pub enum StakeState {
    Activating,    // Warming up
    Active,        // Fully delegated
    Deactivating,  // Cooling down
    Inactive,      // Withdrawable
}

/// Extended stake account info
#[derive(Debug, Clone, Serialize)]
pub struct StakeAccountInfo {
    pub account: Pubkey,
    pub balance_lamports: u64,
    pub state: StakeState,
    pub voter: Option<Pubkey>,        // Who it's delegated to
    pub lockup_epoch: Option<u64>,    // Locked until this epoch
    pub is_liquid: bool,              // Can be withdrawn now
}

/// Aggregated position across all validator accounts
#[derive(Debug, Clone, Serialize)]
pub struct ValidatorPosition {
    pub snapshot_time: i64,
    pub snapshot_slot: u64,

    // Current balances (in lamports for precision)
    pub vote_account_lamports: u64,
    pub vote_account_withdrawable: u64,  // Excludes rent-exempt reserve
    pub identity_lamports: u64,
    pub withdraw_authority_lamports: u64,
    pub jitosol_lamports: u64,           // Raw jitoSOL token amount
    pub jitosol_sol_rate: f64,           // Current on-chain exchange rate
    pub jitosol_sol_equivalent: u64,     // jitoSOL * rate (in lamports)

    // Stake accounts (self-stake)
    pub stake_accounts_liquid: u64,      // Withdrawable stake
    pub stake_accounts_locked: u64,      // Delegated or locked stake
    pub stake_accounts_total: u64,

    // Computed totals (in lamports)
    pub total_liquid_lamports: u64,      // Immediately withdrawable
    pub total_locked_lamports: u64,      // Staked/locked
    pub total_assets_lamports: u64,      // Everything including jitoSOL

    // Reconciliation (corrected formula)
    pub lifetime_income_lamports: u64,
    pub lifetime_expenses_lamports: u64, // Costs paid from tracked accounts
    pub lifetime_withdrawals_lamports: u64,
    pub lifetime_deposits_lamports: u64, // External deposits (seeding)
    pub lst_appreciation_lamports: i64,  // Mark-to-market adjustment for LSTs

    // net_cash_flow = income - expenses - withdrawals + deposits
    pub net_cash_flow_lamports: i64,
    // expected = net_cash_flow + lst_appreciation
    pub expected_balance_lamports: i64,
    pub actual_balance_lamports: u64,
    pub reconciliation_diff_lamports: i64, // actual - expected
}

/// Historical balance record for tracking over time
#[derive(Debug, Clone, Serialize)]
pub struct BalanceSnapshot {
    pub date: String,       // YYYY-MM-DD
    pub epoch: u64,
    pub snapshot_slot: u64, // For precise timing alignment

    // All stored as lamports (u64)
    pub vote_account_lamports: u64,
    pub identity_lamports: u64,
    pub stake_liquid_lamports: u64,
    pub stake_locked_lamports: u64,
    pub jitosol_lamports: u64,
    pub jitosol_rate: f64,

    pub total_lamports: u64,
    pub cumulative_income_lamports: u64,
    pub cumulative_expenses_lamports: u64,
    pub cumulative_withdrawals_lamports: u64,
}
```

### 2. Database Schema Changes

Add to `cache.rs`:

```sql
-- Current/latest balance snapshots per account
-- NOTE: Store lamports as INTEGER (u64), never store SOL as REAL
-- Compute SOL/USD at display time to avoid precision issues
CREATE TABLE IF NOT EXISTS account_balances (
    account TEXT PRIMARY KEY,
    account_type TEXT NOT NULL,
    balance_lamports INTEGER NOT NULL,
    rent_exempt_lamports INTEGER NOT NULL DEFAULT 0,
    withdrawable_lamports INTEGER NOT NULL,
    snapshot_slot INTEGER NOT NULL,
    snapshot_time INTEGER,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Stake accounts owned by the validator (self-stake)
CREATE TABLE IF NOT EXISTS stake_accounts (
    account TEXT PRIMARY KEY,
    balance_lamports INTEGER NOT NULL,
    state TEXT NOT NULL,  -- 'activating', 'active', 'deactivating', 'inactive'
    voter TEXT,           -- Pubkey of vote account delegated to
    lockup_epoch INTEGER, -- Locked until this epoch (NULL if no lockup)
    is_liquid INTEGER NOT NULL DEFAULT 0,  -- 1 if withdrawable now
    snapshot_slot INTEGER NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Historical balance snapshots (daily/per-epoch)
-- All amounts in lamports for precision
CREATE TABLE IF NOT EXISTS balance_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    epoch INTEGER NOT NULL,
    snapshot_slot INTEGER NOT NULL,  -- For precise timing alignment

    vote_account_lamports INTEGER NOT NULL,
    identity_lamports INTEGER NOT NULL,
    withdraw_authority_lamports INTEGER NOT NULL,
    stake_liquid_lamports INTEGER NOT NULL DEFAULT 0,
    stake_locked_lamports INTEGER NOT NULL DEFAULT 0,
    jitosol_lamports INTEGER DEFAULT 0,
    jitosol_rate REAL,  -- On-chain rate at snapshot time

    total_lamports INTEGER NOT NULL,
    cumulative_income_lamports INTEGER NOT NULL,
    cumulative_expenses_lamports INTEGER NOT NULL DEFAULT 0,
    cumulative_withdrawals_lamports INTEGER NOT NULL,
    cumulative_deposits_lamports INTEGER NOT NULL DEFAULT 0,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(date, snapshot_slot)  -- Allow multiple snapshots per day at different slots
);

-- Track withdrawals more explicitly
-- (Enhances existing sol_transfers with withdrawal categorization)
CREATE INDEX IF NOT EXISTS idx_transfers_withdrawal
    ON sol_transfers(to_category)
    WHERE to_category IN ('Exchange', 'PersonalWallet');

-- Index for finding stake accounts by state
CREATE INDEX IF NOT EXISTS idx_stake_state ON stake_accounts(state, is_liquid);
```

### 3. New RPC Functions

Add to `positions.rs`:

```rust
/// Fetch all validator account balances atomically at a single slot
/// Uses getMultipleAccounts to ensure consistent snapshot
pub async fn fetch_all_balances_atomic(
    client: &RpcClient,
    config: &Config,
) -> Result<(Vec<AccountBalance>, u64)> {  // Returns (balances, snapshot_slot)
    // Collect all accounts to fetch
    let accounts = vec![
        config.vote_account,
        config.identity,
        config.withdraw_authority,
    ];

    // Deduplicate (identity may == withdraw_authority)
    let unique_accounts: Vec<_> = accounts.into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    // Fetch atomically at confirmed commitment
    let response = client.get_multiple_accounts_with_commitment(
        &unique_accounts,
        CommitmentConfig::confirmed(),
    )?;

    let slot = response.context.slot;
    // ... parse account data with rent-exempt calculation
}

/// Fetch jitoSOL token balance and current exchange rate from stake pool
pub async fn fetch_jitosol_balance(
    client: &RpcClient,
    identity: &Pubkey,
) -> Result<(u64, f64)> {  // (lamports, current_sol_rate)
    let jitosol_mint = Pubkey::from_str(constants::JITOSOL_MINT)?;
    let ata = get_associated_token_address(identity, &jitosol_mint);

    // Handle missing ATA gracefully (returns 0 if doesn't exist)
    let balance = match client.get_token_account_balance(&ata) {
        Ok(b) => b.amount.parse::<u64>().unwrap_or(0),
        Err(_) => 0,  // ATA doesn't exist yet
    };

    // Fetch current jitoSOL/SOL rate from Jito stake pool
    let rate = fetch_jitosol_exchange_rate(client).await?;

    Ok((balance, rate))
}

/// Fetch current jitoSOL exchange rate from Jito stake pool on-chain state
pub async fn fetch_jitosol_exchange_rate(client: &RpcClient) -> Result<f64> {
    let stake_pool = Pubkey::from_str(constants::JITO_STAKE_POOL)?;
    let account = client.get_account(&stake_pool)?;
    // Parse stake pool state to get total_lamports / pool_token_supply
    // This gives the current SOL backing per jitoSOL token
    // ...
}

/// Discover stake accounts owned by the validator's withdraw authority
pub async fn discover_stake_accounts(
    client: &RpcClient,
    withdraw_authority: &Pubkey,
) -> Result<Vec<StakeAccountInfo>> {
    use solana_account_decoder::UiAccountEncoding;
    use solana_client::rpc_filter::{RpcFilterType, Memcmp, MemcmpEncodedBytes};

    // Filter for stake accounts where withdraw authority matches
    // Stake account layout: bytes 44-76 contain the withdraw authority
    let filters = vec![
        RpcFilterType::Memcmp(Memcmp::new(
            44,  // Offset of withdraw authority in stake account
            MemcmpEncodedBytes::Base58(withdraw_authority.to_string()),
        )),
    ];

    let accounts = client.get_program_accounts_with_config(
        &solana_sdk::stake::program::id(),
        RpcProgramAccountsConfig {
            filters: Some(filters),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                ..Default::default()
            },
            ..Default::default()
        },
    )?;

    // Parse each stake account to determine state and lockup
    // ...
}

/// Build complete position snapshot with reconciliation
pub async fn build_position_snapshot(
    config: &Config,
    balances: &[AccountBalance],
    stake_accounts: &[StakeAccountInfo],
    jitosol_lamports: u64,
    jitosol_rate: f64,
    income_data: &IncomeData,
    snapshot_slot: u64,
) -> Result<ValidatorPosition>;
```

### 4. CLI Commands

Add new subcommand to `main.rs`:

```rust
#[derive(Subcommand, Debug)]
enum Command {
    // ... existing commands ...

    /// Show current account positions and balances
    Position {
        #[command(subcommand)]
        action: PositionCommand,
    },
}

#[derive(Subcommand, Debug)]
enum PositionCommand {
    /// Show current balances across all accounts
    Now,

    /// Show position history over time
    History {
        /// Number of days to show
        #[arg(long, default_value = "30")]
        days: u32,
    },

    /// Reconcile: compare expected vs actual balances
    Reconcile,

    /// Take a balance snapshot (stores to history)
    Snapshot,
}
```

### 5. New Report Output

Add `position_report.csv`:

```csv
account,type,balance_sol,balance_usd,pct_of_total
vote_account,VoteAccount,45.234,9046.80,52.3%
identity,Identity,38.521,7704.20,44.5%
withdraw_authority,WithdrawAuthority,2.100,420.00,2.4%
jitosol_ata,JitoSOL,0.650,130.00,0.8%
TOTAL,,86.505,17301.00,100%

--- Reconciliation ---
lifetime_income,500.000
lifetime_withdrawals,413.495
expected_balance,86.505
actual_balance,86.505
difference,0.000
status,OK
```

### 6. Integration with Existing Reports

Modify `reports.rs` to include position summary in the main output:

```
Block Parliament Validator Financial Tracker
=============================================

... existing output ...

=== CURRENT POSITIONS ===
Vote Account:          45.234 SOL
Identity:              38.521 SOL
Withdraw Authority:     2.100 SOL
jitoSOL (ATA):          0.650 jitoSOL (~0.715 SOL)
-----------------------------------------
Total Held:            86.570 SOL (~$17,314)

Reconciliation:
  Lifetime Income:    500.000 SOL
  Lifetime Withdrawn: 413.430 SOL
  Expected Balance:    86.570 SOL
  Actual Balance:      86.570 SOL
  Difference:           0.000 SOL ✓
```

### 7. Implementation Order

1. **Phase 1: Core Data Models** (positions.rs)
   - Define structs for AccountBalance, ValidatorPosition, StakeAccountInfo
   - Add AccountType and StakeState enums
   - Use u64 for all lamport values

2. **Phase 2: Atomic Balance Fetching**
   - `fetch_all_balances_atomic()` using `getMultipleAccounts`
   - Deduplicate accounts by pubkey before summing
   - Calculate rent-exempt reserves per account type
   - Handle rate limiting and errors

3. **Phase 2.5: Stake Account Discovery**
   - `discover_stake_accounts()` using `getProgramAccounts`
   - Filter by withdraw authority
   - Parse stake state (active, deactivating, etc.)
   - Track lockup epochs

4. **Phase 3: jitoSOL Rate Fetching**
   - `fetch_jitosol_balance()` with graceful ATA handling
   - `fetch_jitosol_exchange_rate()` from on-chain stake pool
   - Make mint address configurable (mainnet vs devnet)

5. **Phase 4: Database Schema**
   - Add account_balances table (lamports only, no floats)
   - Add stake_accounts table
   - Add balance_history table with snapshot_slot
   - Migration for existing databases

6. **Phase 5: Reconciliation Logic**
   - Implement corrected formula: `net_cash_flow = income - expenses - withdrawals + deposits`
   - Add LST appreciation tracking (mark-to-market)
   - Filter income/transfer data to `<= snapshot_slot`

7. **Phase 6: CLI Commands**
   - `position now` - show current state (liquid vs locked)
   - `position reconcile` - detailed reconciliation with breakdown
   - `position snapshot` - store to history
   - `position stake` - show stake account details

8. **Phase 7: Report Integration**
   - Add position summary to main report
   - Generate position_report.csv
   - Generate stake_accounts.csv
   - Optional: balance_history.csv

### 8. Key Implementation Details

#### Atomic Multi-Account Fetch
```rust
// Fetch all accounts at the same slot to avoid timing skew
use solana_client::rpc_response::RpcResult;

let accounts_to_fetch = vec![
    config.vote_account,
    config.identity,
    config.withdraw_authority,
];

// Deduplicate (identity may == withdraw_authority)
let unique: HashSet<Pubkey> = accounts_to_fetch.into_iter().collect();
let unique_vec: Vec<Pubkey> = unique.into_iter().collect();

let response = client.get_multiple_accounts_with_commitment(
    &unique_vec,
    CommitmentConfig::confirmed(),
)?;

let snapshot_slot = response.context.slot;
// All balances now from same slot
```

#### Rent-Exempt Reserve Calculation
```rust
// Vote accounts have ~0.00289 SOL minimum (rent-exempt)
// This balance cannot be withdrawn
const VOTE_ACCOUNT_SIZE: usize = 3762;  // Current vote account size

let rent = client.get_minimum_balance_for_rent_exemption(VOTE_ACCOUNT_SIZE)?;
let withdrawable = balance_lamports.saturating_sub(rent);
```

#### Graceful ATA Handling
```rust
// jitoSOL ATA may not exist if no BAM rewards claimed yet
let jitosol_mint = Pubkey::from_str(constants::JITOSOL_MINT)?;
let ata = get_associated_token_address(&config.identity, &jitosol_mint);

let balance = match client.get_token_account_balance(&ata) {
    Ok(b) => b.amount.parse::<u64>().unwrap_or(0),
    Err(_) => 0,  // ATA doesn't exist - that's fine
};
```

#### On-Chain jitoSOL Exchange Rate
```rust
// Fetch current rate from Jito stake pool, not a hardcoded value
// The rate changes as staking rewards accrete
const JITO_STAKE_POOL: &str = "Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb";

let pool_account = client.get_account(&Pubkey::from_str(JITO_STAKE_POOL)?)?;
// Parse StakePool struct to get:
//   total_lamports (SOL in pool)
//   pool_token_supply (jitoSOL minted)
// rate = total_lamports / pool_token_supply
```

#### Stake Account Discovery
```rust
// Find all stake accounts where we are the withdraw authority
use solana_sdk::stake::state::StakeStateV2;

let stake_accounts = client.get_program_accounts_with_config(
    &stake::program::id(),
    RpcProgramAccountsConfig {
        filters: Some(vec![
            // Filter by withdraw authority at offset 44
            RpcFilterType::Memcmp(Memcmp::new(
                44,
                MemcmpEncodedBytes::Base58(config.withdraw_authority.to_string()),
            )),
        ]),
        ..Default::default()
    },
)?;

// Parse each to determine: active/deactivating/inactive, lockup status
for (pubkey, account) in stake_accounts {
    let stake_state: StakeStateV2 = bincode::deserialize(&account.data)?;
    // Extract delegation info, lockup, etc.
}
```

#### Corrected Reconciliation Logic
```rust
fn reconcile(position: &ValidatorPosition) -> ReconciliationResult {
    // Net cash flow: what should be in the accounts
    let net_cash_flow = position.lifetime_income_lamports as i64
        - position.lifetime_expenses_lamports as i64  // Costs paid from accounts
        - position.lifetime_withdrawals_lamports as i64
        + position.lifetime_deposits_lamports as i64;  // Seeding

    // Asset value adjustments (LST appreciation is mark-to-market)
    let lst_adjustment = position.lst_appreciation_lamports;

    // Expected balance
    let expected = net_cash_flow + lst_adjustment;

    // Actual balance (all tracked accounts)
    let actual = position.total_assets_lamports as i64;

    let diff = actual - expected;

    ReconciliationResult {
        net_cash_flow_lamports: net_cash_flow,
        lst_adjustment_lamports: lst_adjustment,
        expected_lamports: expected,
        actual_lamports: actual,
        difference_lamports: diff,
        // Allow small variance for rounding, dust, etc.
        status: if diff.abs() < 100_000 { "OK" } else { "VARIANCE" },  // 0.0001 SOL tolerance
    }
}
```

#### Account Deduplication
```rust
// Identity and withdraw_authority may be the same pubkey
// Don't double-count!
let mut seen = HashSet::new();
let mut total_lamports = 0u64;

for balance in &balances {
    if seen.insert(balance.account) {
        total_lamports += balance.balance_lamports;
    }
}
```

### 9. Solana-Specific Considerations

Based on peer review feedback (Gemini + Codex):

#### Vote Account Balances
- Vote account balance includes **rent-exempt reserve** (~0.00289 SOL)
- This minimum balance **cannot be withdrawn**
- Always show both "total" and "withdrawable" amounts

#### Stake Account Complexities
- **Activating**: Warming up, not yet earning rewards
- **Active**: Fully delegated, earning rewards
- **Deactivating**: Cooling down (1 epoch), soon withdrawable
- **Inactive**: Fully withdrawable
- **Lockup**: Some stake accounts have custodian lockups until a specific epoch

#### jitoSOL Valuation
- jitoSOL value accretes over time as staking rewards compound
- **Do NOT use a hardcoded rate** - fetch from on-chain stake pool
- Rate changes with every epoch as rewards distribute
- Store both raw jitoSOL amount AND rate at snapshot time

#### Missing Token Accounts
- Associated Token Account (ATA) may not exist if user never received that token
- `getTokenAccountBalance` will **fail** for missing ATA
- Always handle gracefully by treating as zero balance

#### MEV/Tips Distribution
- Jito tips may land in separate accounts or be auto-swapped
- Consider discovering tip distribution accounts rather than hardcoding
- Some validators use multiple tip receivers

### 10. Trade-offs & Considerations

**Option A: Snapshot on every run**
- Pro: Always up-to-date
- Con: Extra RPC calls, slower

**Option B: Separate `position` command**
- Pro: Explicit, user-controlled
- Con: Data might be stale in reports

**Recommendation**: Option B with optional `--include-positions` flag on main report

**Historical Tracking Granularity**:
- Per-epoch: More granular, more storage
- Per-day: Good balance
- Per-month: Minimal storage, less useful

**Recommendation**: Per-day with `snapshot_slot` recorded for precise timing

**Precision Strategy**:
- Store all amounts as `u64` lamports in database
- Compute SOL (÷1e9) and USD (×price) at display time only
- This avoids f64 precision drift that defeats reconciliation

### 11. Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `src/positions.rs` | Create | New module for position tracking, balance fetching, reconciliation |
| `src/stake.rs` | Create | Stake account discovery and parsing |
| `src/cache.rs` | Modify | Add balance tables, stake tables, and queries |
| `src/main.rs` | Modify | Add `position` subcommand with now/reconcile/stake actions |
| `src/reports.rs` | Modify | Add position summary, generate position_report.csv |
| `src/config.rs` | Modify | Add configurable mint addresses (mainnet/devnet) |
| `src/constants.rs` | Modify | Add JITOSOL_MINT, JITO_STAKE_POOL, VOTE_ACCOUNT_SIZE |
| `Cargo.toml` | Modify | Add spl-associated-token-account, spl-token, bincode deps |

### 12. Testing Strategy

1. **Unit tests** for reconciliation math (use u64 lamports, verify no precision loss)
2. **Unit tests** for account deduplication logic
3. **Unit tests** for stake state parsing
4. **Integration tests** with mocked RPC responses (including missing ATA case)
5. **Integration tests** for atomic multi-account fetch
6. **Manual testing** against devnet first, then mainnet
7. **Reconciliation validation** against known historical data

### 13. Future Enhancements

- ~~Track stake account delegations~~ (now in Phase 2.5)
- Alert when reconciliation variance exceeds threshold
- Historical charts/graphs of position over time
- Integration with external portfolio trackers
- Track multiple tip distribution accounts
- Support for other LSTs (mSOL, bSOL, etc.)
- Automatic jitoSOL → SOL conversion tracking
- "What-if" analysis (if I withdraw X, what's the impact?)

### 14. Open Questions (from Peer Review)

These need clarification before/during implementation:

1. **Stake account scope**: Should "position" include only liquid balances, or also self-stake and locked stake accounts?
   - **Recommendation**: Include all, but clearly separate liquid vs locked

2. **Expense payment source**: Are expenses paid from tracked validator accounts, or off-chain (fiat)?
   - **If from tracked accounts**: Must include in reconciliation formula
   - **If off-chain**: Can exclude from on-chain reconciliation

3. **PersonalWallet allocation**: How is the "validator-related" portion of personal wallet determined?
   - **Options**: Manual percentage, tagged transfers, or explicit account list
   - **Recommendation**: Use transfer history to identify seeding/withdrawal flows

---

## Appendix: Peer Review Summary

This plan was reviewed by **Gemini** and **Codex** CLI tools on 2026-01-27.

### Critical Issues Identified

| Severity | Issue | Resolution |
|----------|-------|------------|
| 🔴 High | Reconciliation formula ignored expenses | Fixed: Now uses `income - expenses - withdrawals + deposits` |
| 🔴 High | Missing stake account tracking | Fixed: Added Phase 2.5 for stake account discovery |
| 🔴 High | jitoSOL rate was hardcoded | Fixed: Now fetches on-chain rate from stake pool |
| 🟡 Medium | f64 precision issues | Fixed: Store lamports as u64, compute SOL at display |
| 🟡 Medium | Non-atomic balance fetches | Fixed: Use `getMultipleAccounts` for single-slot snapshot |
| 🟡 Medium | Account deduplication missing | Fixed: Added dedup logic for identity==withdraw_auth case |
| 🟡 Medium | Missing rent-exempt tracking | Fixed: Added `withdrawable_lamports` field |

### Solana-Specific Gaps Addressed

- Vote account rent-exempt reserve handling
- Stake account state tracking (active/deactivating/inactive)
- Lockup epoch awareness
- Graceful missing ATA handling
- MEV/tip distribution account discovery consideration

### Reviewers

- **Gemini CLI** (Google): Provided implementation code draft
- **Codex CLI** (OpenAI): Provided critical analysis and Solana-specific gaps
