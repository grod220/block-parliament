//! SQLite caching for historical epoch data and expense storage
//!
//! Completed epochs are immutable, so we cache them to avoid re-querying.
//! Current/incomplete epochs are always re-fetched.
//! Expenses are stored persistently for financial tracking.

use anyhow::{Context, Result};
use sqlx::{FromRow, SqlitePool};
use std::path::Path;

use crate::addresses::AddressCategory;
use crate::bam::BamClaim;
use crate::config::Config;
use crate::doublezero::DoubleZeroFee;
use crate::expenses::{Expense, ExpenseCategory, RecurringExpense};
use crate::jito::MevClaim;
use crate::leader_fees::EpochLeaderFees;
use crate::positions::{StakeAccountInfo, ValidatorPosition};
use crate::prices::PriceCache;
use crate::transactions::{EpochReward, SolTransfer};
use crate::vote_costs::EpochVoteCost;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Cache database wrapper
pub struct Cache {
    pool: SqlitePool,
}

/// Row type for epoch rewards query
#[derive(FromRow)]
struct EpochRewardRow {
    epoch: i64,
    amount_lamports: i64,
    amount_sol: f64,
    commission: i64,
    effective_slot: i64,
    date: Option<String>,
}

/// Row type for leader fees query
#[derive(FromRow)]
struct LeaderFeesRow {
    epoch: i64,
    leader_slots: i64,
    blocks_produced: i64,
    skipped_slots: i64,
    total_fees_lamports: i64,
    total_fees_sol: f64,
    date: Option<String>,
}

/// Row type for MEV claims query
#[derive(FromRow)]
struct MevClaimRow {
    epoch: i64,
    total_tips_lamports: i64,
    commission_lamports: i64,
    amount_sol: f64,
    date: Option<String>,
}

/// Row type for vote costs query
#[derive(FromRow)]
struct VoteCostRow {
    epoch: i64,
    vote_count: i64,
    total_fee_lamports: i64,
    total_fee_sol: f64,
    source: String,
    date: Option<String>,
}

/// Row type for DoubleZero fees query
#[derive(FromRow)]
#[allow(dead_code)]
struct DoubleZeroFeeRow {
    epoch: i64,
    fee_base_lamports: i64,
    liability_lamports: i64,
    liability_sol: f64,
    fee_rate_bps: i64,
    date: Option<String>,
    source: String,
    is_estimate: i64,
}

/// Row type for BAM claims query
#[derive(FromRow)]
struct BamClaimRow {
    tx_signature: String,
    epoch: i64,
    amount_jitosol_lamports: i64,
    amount_sol_equivalent: f64,
    jitosol_sol_rate: Option<f64>,
    claimed_at: Option<String>,
    date: String,
}

/// Row type for expenses query
#[derive(FromRow)]
struct ExpenseRow {
    id: i64,
    date: String,
    vendor: String,
    category: String,
    description: String,
    amount_usd: f64,
    paid_with: String,
    invoice_id: Option<String>,
}

/// Row type for recurring expenses query
#[derive(FromRow)]
struct RecurringExpenseRow {
    id: i64,
    vendor: String,
    category: String,
    description: String,
    amount_usd: f64,
    paid_with: String,
    start_date: String,
    end_date: Option<String>,
}

/// Row type for sol_transfers query
#[derive(FromRow)]
struct SolTransferRow {
    signature: String,
    slot: i64,
    timestamp: Option<i64>,
    date: Option<String>,
    from_address: String,
    to_address: String,
    amount_lamports: i64,
    amount_sol: f64,
    from_label: String,
    to_label: String,
    from_category: String,
    to_category: String,
}

impl Cache {
    /// Open or create cache database
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // SQLx requires the file to exist for SQLite
        if !path.exists() {
            std::fs::File::create(path)?;
        }

        let url = format!("sqlite:{}", path.display());
        let pool = SqlitePool::connect(&url)
            .await
            .context("Failed to open cache database")?;

        // Enable WAL mode for better concurrency and set busy timeout
        // This prevents SQLITE_BUSY errors when multiple processes access the DB
        sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
        sqlx::query("PRAGMA busy_timeout=5000").execute(&pool).await?;

        let cache = Self { pool };
        cache.init_schema().await?;

        Ok(cache)
    }

    /// Initialize database schema
    async fn init_schema(&self) -> Result<()> {
        sqlx::query(
            "
            -- Commission rewards per epoch
            CREATE TABLE IF NOT EXISTS epoch_rewards (
                epoch INTEGER PRIMARY KEY,
                amount_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                commission INTEGER NOT NULL,
                effective_slot INTEGER NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Leader slot fees per epoch
            CREATE TABLE IF NOT EXISTS leader_fees (
                epoch INTEGER PRIMARY KEY,
                leader_slots INTEGER NOT NULL,
                blocks_produced INTEGER NOT NULL,
                skipped_slots INTEGER NOT NULL,
                total_fees_lamports INTEGER NOT NULL,
                total_fees_sol REAL NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Jito MEV claims per epoch
            CREATE TABLE IF NOT EXISTS mev_claims (
                epoch INTEGER PRIMARY KEY,
                total_tips_lamports INTEGER NOT NULL,
                commission_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Jito BAM claims (jitoSOL rewards per JIP-31)
            -- Uses tx_signature as primary key for idempotent inserts
            CREATE TABLE IF NOT EXISTS bam_claims (
                tx_signature TEXT PRIMARY KEY,
                epoch INTEGER NOT NULL,
                amount_jitosol_lamports INTEGER NOT NULL,
                amount_sol_equivalent REAL NOT NULL,
                jitosol_sol_rate REAL,
                claimed_at TEXT,
                date TEXT NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Index for quick lookups by epoch
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_bam_claims_epoch ON bam_claims(epoch)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "
            -- Vote transaction costs per epoch
            CREATE TABLE IF NOT EXISTS vote_costs (
                epoch INTEGER PRIMARY KEY,
                vote_count INTEGER NOT NULL,
                total_fee_lamports INTEGER NOT NULL,
                total_fee_sol REAL NOT NULL,
                source TEXT NOT NULL,
                date TEXT,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- DoubleZero fees per epoch (liability accruals)
            CREATE TABLE IF NOT EXISTS doublezero_fees (
                epoch INTEGER PRIMARY KEY,
                fee_base_lamports INTEGER NOT NULL,
                liability_lamports INTEGER NOT NULL,
                liability_sol REAL NOT NULL,
                fee_rate_bps INTEGER NOT NULL,
                date TEXT,
                source TEXT NOT NULL,
                is_estimate INTEGER NOT NULL DEFAULT 0,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Historical SOL prices
            CREATE TABLE IF NOT EXISTS prices (
                date TEXT PRIMARY KEY,
                usd_price REAL NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Cache metadata
            CREATE TABLE IF NOT EXISTS metadata (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Expenses (persistent storage, not cache)
            CREATE TABLE IF NOT EXISTS expenses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                vendor TEXT NOT NULL,
                category TEXT NOT NULL,
                description TEXT NOT NULL,
                amount_usd REAL NOT NULL,
                paid_with TEXT NOT NULL,
                invoice_id TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "
            -- Recurring expenses (templates that expand into monthly entries)
            CREATE TABLE IF NOT EXISTS recurring_expenses (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                vendor TEXT NOT NULL,
                category TEXT NOT NULL,
                description TEXT NOT NULL,
                amount_usd REAL NOT NULL,
                paid_with TEXT NOT NULL,
                start_date TEXT NOT NULL,
                end_date TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // SOL transfers table:
        // We store each distinct SOL movement once, keyed by (signature, from, to, amount).
        // This avoids silently dropping multi-transfer transactions and avoids double-counting
        // the same transfer fetched from multiple account histories.
        //
        // If an older schema exists (with `account_key` as part of the primary key), migrate it.
        self.maybe_migrate_sol_transfers().await?;

        sqlx::query(
            "
            -- SOL transfers (cached)
            CREATE TABLE IF NOT EXISTS sol_transfers (
                signature TEXT NOT NULL,
                slot INTEGER NOT NULL,
                timestamp INTEGER,
                date TEXT,
                from_address TEXT NOT NULL,
                to_address TEXT NOT NULL,
                amount_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                from_label TEXT NOT NULL,
                to_label TEXT NOT NULL,
                from_category TEXT NOT NULL,
                to_category TEXT NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (signature, from_address, to_address, amount_lamports)
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_transfers_slot ON sol_transfers(slot)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "
            -- Track the highest slot checked per account (even if no transfers found)
            CREATE TABLE IF NOT EXISTS account_progress (
                account_key TEXT PRIMARY KEY,
                highest_slot INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // =====================================================================
        // Position Tracking Tables
        // =====================================================================

        sqlx::query(
            "
            -- Stake accounts owned by the validator (self-stake)
            CREATE TABLE IF NOT EXISTS stake_accounts (
                account TEXT PRIMARY KEY,
                balance_lamports INTEGER NOT NULL,
                state TEXT NOT NULL,
                voter TEXT,
                lockup_epoch INTEGER,
                is_liquid INTEGER NOT NULL DEFAULT 0,
                snapshot_slot INTEGER NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_stake_state ON stake_accounts(state, is_liquid)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            "
            -- Historical balance snapshots (daily/per-epoch)
            -- All amounts in lamports for precision
            CREATE TABLE IF NOT EXISTS balance_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                date TEXT NOT NULL,
                epoch INTEGER NOT NULL,
                snapshot_slot INTEGER NOT NULL,
                vote_account_lamports INTEGER NOT NULL,
                identity_lamports INTEGER NOT NULL,
                withdraw_authority_lamports INTEGER NOT NULL,
                stake_liquid_lamports INTEGER NOT NULL DEFAULT 0,
                stake_locked_lamports INTEGER NOT NULL DEFAULT 0,
                jitosol_lamports INTEGER DEFAULT 0,
                jitosol_rate REAL,
                total_lamports INTEGER NOT NULL,
                cumulative_income_lamports INTEGER NOT NULL,
                cumulative_expenses_lamports INTEGER NOT NULL DEFAULT 0,
                cumulative_withdrawals_lamports INTEGER NOT NULL,
                cumulative_deposits_lamports INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(date, snapshot_slot)
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Index for withdrawal tracking
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transfers_withdrawal
             ON sol_transfers(to_category)
             WHERE to_category IN ('Exchange', 'PersonalWallet')",
        )
        .execute(&self.pool)
        .await
        .ok(); // Ignore error if partial index not supported

        Ok(())
    }

    async fn maybe_migrate_sol_transfers(&self) -> Result<()> {
        // Check if table exists and whether it has the legacy `account_key` column.
        let table_exists: Option<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name='sol_transfers'")
                .fetch_optional(&self.pool)
                .await?;
        if table_exists.is_none() {
            return Ok(());
        }

        let columns: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info('sol_transfers')")
            .fetch_all(&self.pool)
            .await?;
        let has_account_key = columns.iter().any(|(name,)| name == "account_key");
        if !has_account_key {
            return Ok(());
        }

        eprintln!("Migrating legacy sol_transfers schema (dropping account_key, improving dedupe)...");

        let mut tx = self.pool.begin().await?;

        sqlx::query("DROP TABLE IF EXISTS sol_transfers_new")
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "
            CREATE TABLE IF NOT EXISTS sol_transfers_new (
                signature TEXT NOT NULL,
                slot INTEGER NOT NULL,
                timestamp INTEGER,
                date TEXT,
                from_address TEXT NOT NULL,
                to_address TEXT NOT NULL,
                amount_lamports INTEGER NOT NULL,
                amount_sol REAL NOT NULL,
                from_label TEXT NOT NULL,
                to_label TEXT NOT NULL,
                from_category TEXT NOT NULL,
                to_category TEXT NOT NULL,
                fetched_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (signature, from_address, to_address, amount_lamports)
            )
            ",
        )
        .execute(&mut *tx)
        .await?;

        // Insert one row per distinct transfer key, choosing the max slot/timestamp/date for that key.
        sqlx::query(
            "
            INSERT OR IGNORE INTO sol_transfers_new
                (signature, slot, timestamp, date, from_address, to_address,
                 amount_lamports, amount_sol, from_label, to_label,
                 from_category, to_category, fetched_at)
            SELECT
                signature,
                MAX(slot) as slot,
                MAX(timestamp) as timestamp,
                MAX(date) as date,
                from_address,
                to_address,
                amount_lamports,
                MAX(amount_sol) as amount_sol,
                MAX(from_label) as from_label,
                MAX(to_label) as to_label,
                MAX(from_category) as from_category,
                MAX(to_category) as to_category,
                MIN(fetched_at) as fetched_at
            FROM sol_transfers
            GROUP BY signature, from_address, to_address, amount_lamports
            ",
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query("DROP TABLE sol_transfers").execute(&mut *tx).await?;
        sqlx::query("ALTER TABLE sol_transfers_new RENAME TO sol_transfers")
            .execute(&mut *tx)
            .await?;

        sqlx::query("DROP INDEX IF EXISTS idx_transfers_account")
            .execute(&mut *tx)
            .await
            .ok();
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_transfers_slot ON sol_transfers(slot)")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_transfers_withdrawal
             ON sol_transfers(to_category)
             WHERE to_category IN ('Exchange', 'PersonalWallet')",
        )
        .execute(&mut *tx)
        .await
        .ok();

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Epoch Rewards (Commission)
    // =========================================================================

    /// Get cached epoch rewards
    pub async fn get_epoch_rewards(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<EpochReward>> {
        let rows: Vec<EpochRewardRow> = sqlx::query_as(
            "SELECT epoch, amount_lamports, amount_sol, commission, effective_slot, date
             FROM epoch_rewards
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EpochReward {
                epoch: r.epoch as u64,
                amount_lamports: r.amount_lamports as u64,
                amount_sol: r.amount_sol,
                commission: r.commission as u8,
                effective_slot: r.effective_slot as u64,
                date: r.date,
            })
            .collect())
    }

    /// Get epochs that are missing from cache
    pub async fn get_missing_reward_epochs(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<u64>> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT epoch FROM epoch_rewards WHERE epoch >= ? AND epoch <= ?")
            .bind(start_epoch as i64)
            .bind(end_epoch as i64)
            .fetch_all(&self.pool)
            .await?;

        let cached: Vec<u64> = rows.into_iter().map(|(e,)| e as u64).collect();

        let missing: Vec<u64> = (start_epoch..=end_epoch).filter(|e| !cached.contains(e)).collect();

        Ok(missing)
    }

    /// Store epoch rewards (in a transaction for atomicity)
    pub async fn store_epoch_rewards(&self, rewards: &[EpochReward]) -> Result<()> {
        if rewards.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for reward in rewards {
            sqlx::query(
                "INSERT OR REPLACE INTO epoch_rewards
                 (epoch, amount_lamports, amount_sol, commission, effective_slot, date)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(reward.epoch as i64)
            .bind(reward.amount_lamports as i64)
            .bind(reward.amount_sol)
            .bind(reward.commission as i64)
            .bind(reward.effective_slot as i64)
            .bind(&reward.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Leader Fees
    // =========================================================================

    /// Get cached leader fees
    pub async fn get_leader_fees(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<EpochLeaderFees>> {
        let rows: Vec<LeaderFeesRow> = sqlx::query_as(
            "SELECT epoch, leader_slots, blocks_produced, skipped_slots,
                    total_fees_lamports, total_fees_sol, date
             FROM leader_fees
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EpochLeaderFees {
                epoch: r.epoch as u64,
                leader_slots: r.leader_slots as u64,
                blocks_produced: r.blocks_produced as u64,
                skipped_slots: r.skipped_slots as u64,
                total_fees_lamports: r.total_fees_lamports as u64,
                total_fees_sol: r.total_fees_sol,
                date: r.date,
            })
            .collect())
    }

    /// Get epochs missing leader fee data
    pub async fn get_missing_leader_fee_epochs(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<u64>> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT epoch FROM leader_fees WHERE epoch >= ? AND epoch <= ?")
            .bind(start_epoch as i64)
            .bind(end_epoch as i64)
            .fetch_all(&self.pool)
            .await?;

        let cached: Vec<u64> = rows.into_iter().map(|(e,)| e as u64).collect();

        let missing: Vec<u64> = (start_epoch..=end_epoch).filter(|e| !cached.contains(e)).collect();

        Ok(missing)
    }

    /// Store leader fees (in a transaction for atomicity)
    pub async fn store_leader_fees(&self, fees: &[EpochLeaderFees]) -> Result<()> {
        if fees.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for fee in fees {
            sqlx::query(
                "INSERT OR REPLACE INTO leader_fees
                 (epoch, leader_slots, blocks_produced, skipped_slots, total_fees_lamports, total_fees_sol, date)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(fee.epoch as i64)
            .bind(fee.leader_slots as i64)
            .bind(fee.blocks_produced as i64)
            .bind(fee.skipped_slots as i64)
            .bind(fee.total_fees_lamports as i64)
            .bind(fee.total_fees_sol)
            .bind(&fee.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // MEV Claims
    // =========================================================================

    /// Get cached MEV claims
    pub async fn get_mev_claims(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<MevClaim>> {
        let rows: Vec<MevClaimRow> = sqlx::query_as(
            "SELECT epoch, total_tips_lamports, commission_lamports, amount_sol, date
             FROM mev_claims
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| MevClaim {
                epoch: r.epoch as u64,
                total_tips_lamports: r.total_tips_lamports as u64,
                commission_lamports: r.commission_lamports as u64,
                amount_sol: r.amount_sol,
                date: r.date,
            })
            .collect())
    }

    /// Store MEV claims (in a transaction for atomicity)
    pub async fn store_mev_claims(&self, claims: &[MevClaim]) -> Result<()> {
        if claims.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for claim in claims {
            sqlx::query(
                "INSERT OR REPLACE INTO mev_claims
                 (epoch, total_tips_lamports, commission_lamports, amount_sol, date)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(claim.epoch as i64)
            .bind(claim.total_tips_lamports as i64)
            .bind(claim.commission_lamports as i64)
            .bind(claim.amount_sol)
            .bind(&claim.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // BAM Claims (jitoSOL rewards)
    // =========================================================================

    /// Get cached BAM claims for an epoch range
    pub async fn get_bam_claims(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<BamClaim>> {
        let rows: Vec<BamClaimRow> = sqlx::query_as(
            "SELECT tx_signature, epoch, amount_jitosol_lamports, amount_sol_equivalent,
                    jitosol_sol_rate, claimed_at, date
             FROM bam_claims
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| BamClaim {
                epoch: r.epoch as u64,
                amount_jitosol_lamports: r.amount_jitosol_lamports as u64,
                amount_sol_equivalent: r.amount_sol_equivalent,
                jitosol_sol_rate: r.jitosol_sol_rate,
                claimed_at: r.claimed_at,
                tx_signature: r.tx_signature,
                date: Some(r.date),
            })
            .collect())
    }

    /// Get epochs that have BAM claims cached
    pub async fn get_cached_bam_epochs(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<u64>> {
        let rows: Vec<(i64,)> = sqlx::query_as("SELECT DISTINCT epoch FROM bam_claims WHERE epoch >= ? AND epoch <= ?")
            .bind(start_epoch as i64)
            .bind(end_epoch as i64)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|(e,)| e as u64).collect())
    }

    /// Store BAM claims (uses INSERT OR REPLACE to allow updates on re-fetch)
    ///
    /// Matches the pattern used by other epoch-based tables (epoch_rewards, mev_claims, etc.)
    /// so that re-running reports can update cached data if needed.
    pub async fn store_bam_claims(&self, claims: &[BamClaim]) -> Result<()> {
        if claims.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for claim in claims {
            sqlx::query(
                "INSERT OR REPLACE INTO bam_claims
                 (tx_signature, epoch, amount_jitosol_lamports, amount_sol_equivalent,
                  jitosol_sol_rate, claimed_at, date)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&claim.tx_signature)
            .bind(claim.epoch as i64)
            .bind(claim.amount_jitosol_lamports as i64)
            .bind(claim.amount_sol_equivalent)
            .bind(claim.jitosol_sol_rate)
            .bind(&claim.claimed_at)
            .bind(claim.date.as_deref().unwrap_or("unknown"))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Vote Costs
    // =========================================================================

    /// Get cached vote costs
    pub async fn get_vote_costs(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<EpochVoteCost>> {
        let rows: Vec<VoteCostRow> = sqlx::query_as(
            "SELECT epoch, vote_count, total_fee_lamports, total_fee_sol, source, date
             FROM vote_costs
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| EpochVoteCost {
                epoch: r.epoch as u64,
                vote_count: r.vote_count as u64,
                total_fee_lamports: r.total_fee_lamports as u64,
                total_fee_sol: r.total_fee_sol,
                source: r.source,
                date: r.date,
            })
            .collect())
    }

    /// Store vote costs (in a transaction for atomicity)
    pub async fn store_vote_costs(&self, costs: &[EpochVoteCost]) -> Result<()> {
        if costs.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for cost in costs {
            sqlx::query(
                "INSERT OR REPLACE INTO vote_costs
                 (epoch, vote_count, total_fee_lamports, total_fee_sol, source, date)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(cost.epoch as i64)
            .bind(cost.vote_count as i64)
            .bind(cost.total_fee_lamports as i64)
            .bind(cost.total_fee_sol)
            .bind(&cost.source)
            .bind(&cost.date)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // DoubleZero Fees
    // =========================================================================

    /// Get cached DoubleZero fees
    #[allow(dead_code)]
    pub async fn get_doublezero_fees(&self, start_epoch: u64, end_epoch: u64) -> Result<Vec<DoubleZeroFee>> {
        let rows: Vec<DoubleZeroFeeRow> = sqlx::query_as(
            "SELECT epoch, fee_base_lamports, liability_lamports, liability_sol,
                    fee_rate_bps, date, source, is_estimate
             FROM doublezero_fees
             WHERE epoch >= ? AND epoch <= ?
             ORDER BY epoch",
        )
        .bind(start_epoch as i64)
        .bind(end_epoch as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| DoubleZeroFee {
                epoch: r.epoch as u64,
                fee_base_lamports: r.fee_base_lamports as u64,
                liability_lamports: r.liability_lamports as u64,
                liability_sol: r.liability_sol,
                fee_rate_bps: r.fee_rate_bps as u64,
                date: r.date,
                source: r.source,
                is_estimate: r.is_estimate != 0,
            })
            .collect())
    }

    /// Store DoubleZero fees (in a transaction for atomicity)
    pub async fn store_doublezero_fees(&self, fees: &[DoubleZeroFee]) -> Result<()> {
        if fees.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for fee in fees {
            sqlx::query(
                "INSERT OR REPLACE INTO doublezero_fees
                 (epoch, fee_base_lamports, liability_lamports, liability_sol,
                  fee_rate_bps, date, source, is_estimate)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(fee.epoch as i64)
            .bind(fee.fee_base_lamports as i64)
            .bind(fee.liability_lamports as i64)
            .bind(fee.liability_sol)
            .bind(fee.fee_rate_bps as i64)
            .bind(&fee.date)
            .bind(&fee.source)
            .bind(if fee.is_estimate { 1i64 } else { 0i64 })
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Prices
    // =========================================================================

    /// Get cached prices
    pub async fn get_prices(&self) -> Result<PriceCache> {
        let rows: Vec<(String, f64)> = sqlx::query_as("SELECT date, usd_price FROM prices")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().collect())
    }

    /// Store prices (in a transaction for atomicity)
    pub async fn store_prices(&self, prices: &PriceCache) -> Result<()> {
        if prices.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for (date, price) in prices {
            sqlx::query("INSERT OR REPLACE INTO prices (date, usd_price) VALUES (?, ?)")
                .bind(date)
                .bind(price)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Metadata
    // =========================================================================

    /// Get metadata value
    #[allow(dead_code)]
    pub async fn get_metadata(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM metadata WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|(v,)| v))
    }

    /// Set metadata value
    #[allow(dead_code)]
    pub async fn set_metadata(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO metadata (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // =========================================================================
    // Expenses
    // =========================================================================

    /// Get all expenses
    pub async fn get_expenses(&self) -> Result<Vec<Expense>> {
        let rows: Vec<ExpenseRow> = sqlx::query_as(
            "SELECT id, date, vendor, category, description, amount_usd, paid_with, invoice_id
             FROM expenses
             ORDER BY date, id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let category = match r.category.as_str() {
                    "Hosting" => ExpenseCategory::Hosting,
                    "Contractor" => ExpenseCategory::Contractor,
                    "Hardware" => ExpenseCategory::Hardware,
                    "Software" => ExpenseCategory::Software,
                    "VoteFees" => ExpenseCategory::VoteFees,
                    _ => ExpenseCategory::Other,
                };

                Expense {
                    id: Some(r.id),
                    date: r.date,
                    vendor: r.vendor,
                    category,
                    description: r.description,
                    amount_usd: r.amount_usd,
                    paid_with: r.paid_with,
                    invoice_id: r.invoice_id,
                }
            })
            .collect())
    }

    /// Add a new expense, returns the ID
    pub async fn add_expense(&self, expense: &Expense) -> Result<i64> {
        let category_str = match expense.category {
            ExpenseCategory::Hosting => "Hosting",
            ExpenseCategory::Contractor => "Contractor",
            ExpenseCategory::Hardware => "Hardware",
            ExpenseCategory::Software => "Software",
            ExpenseCategory::VoteFees => "VoteFees",
            ExpenseCategory::Other => "Other",
        };

        let result = sqlx::query(
            "INSERT INTO expenses (date, vendor, category, description, amount_usd, paid_with, invoice_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&expense.date)
        .bind(&expense.vendor)
        .bind(category_str)
        .bind(&expense.description)
        .bind(expense.amount_usd)
        .bind(&expense.paid_with)
        .bind(&expense.invoice_id)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Delete an expense by ID
    pub async fn delete_expense(&self, id: i64) -> Result<bool> {
        let result = sqlx::query("DELETE FROM expenses WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Import multiple expenses (for bulk import from CSV)
    pub async fn import_expenses(&self, expenses: &[Expense]) -> Result<usize> {
        let mut count = 0;
        for expense in expenses {
            self.add_expense(expense).await?;
            count += 1;
        }
        Ok(count)
    }

    // =========================================================================
    // Recurring Expenses
    // =========================================================================

    /// Get all recurring expenses
    pub async fn get_recurring_expenses(&self) -> Result<Vec<RecurringExpense>> {
        let rows: Vec<RecurringExpenseRow> = sqlx::query_as(
            "SELECT id, vendor, category, description, amount_usd, paid_with, start_date, end_date
             FROM recurring_expenses
             ORDER BY vendor, start_date",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let category = match r.category.as_str() {
                    "Hosting" => ExpenseCategory::Hosting,
                    "Contractor" => ExpenseCategory::Contractor,
                    "Hardware" => ExpenseCategory::Hardware,
                    "Software" => ExpenseCategory::Software,
                    "VoteFees" => ExpenseCategory::VoteFees,
                    _ => ExpenseCategory::Other,
                };

                RecurringExpense {
                    id: Some(r.id),
                    vendor: r.vendor,
                    category,
                    description: r.description,
                    amount_usd: r.amount_usd,
                    paid_with: r.paid_with,
                    start_date: r.start_date,
                    end_date: r.end_date,
                }
            })
            .collect())
    }

    /// Add a new recurring expense, returns the ID
    pub async fn add_recurring_expense(&self, expense: &RecurringExpense) -> Result<i64> {
        let category_str = match expense.category {
            ExpenseCategory::Hosting => "Hosting",
            ExpenseCategory::Contractor => "Contractor",
            ExpenseCategory::Hardware => "Hardware",
            ExpenseCategory::Software => "Software",
            ExpenseCategory::VoteFees => "VoteFees",
            ExpenseCategory::Other => "Other",
        };

        let result = sqlx::query(
            "INSERT INTO recurring_expenses (vendor, category, description, amount_usd, paid_with, start_date, end_date)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&expense.vendor)
        .bind(category_str)
        .bind(&expense.description)
        .bind(expense.amount_usd)
        .bind(&expense.paid_with)
        .bind(&expense.start_date)
        .bind(&expense.end_date)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Delete a recurring expense by ID
    pub async fn delete_recurring_expense(&self, id: i64) -> Result<bool> {
        let result = sqlx::query("DELETE FROM recurring_expenses WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // =========================================================================
    // SOL Transfers
    // =========================================================================

    /// Get all cached transfers
    pub async fn get_all_transfers(&self) -> Result<Vec<SolTransfer>> {
        let rows: Vec<SolTransferRow> = sqlx::query_as(
            "SELECT signature, slot, timestamp, date, from_address, to_address,
                    amount_lamports, amount_sol, from_label, to_label,
                    from_category, to_category
             FROM sol_transfers
             ORDER BY slot DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().filter_map(row_to_transfer).collect())
    }

    /// Get the highest slot we've checked for an account (even if no transfers were found)
    /// This is useful for accounts with only versioned/undecodable transactions
    pub async fn get_account_progress(&self, account_key: &str) -> Result<Option<u64>> {
        // We track per-account progress independently of transfer storage.
        let progress_row: Option<(i64,)> =
            sqlx::query_as("SELECT highest_slot FROM account_progress WHERE account_key = ?")
                .bind(account_key)
                .fetch_optional(&self.pool)
                .await?;

        let progress_slot = progress_row.map(|(s,)| s as u64);
        Ok(progress_slot)
    }

    /// Store the highest slot we've checked for an account
    pub async fn set_account_progress(&self, account_key: &str, highest_slot: u64) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO account_progress (account_key, highest_slot) VALUES (?, ?)")
            .bind(account_key)
            .bind(highest_slot as i64)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Store transfers (in a transaction for atomicity)
    pub async fn store_transfers(&self, transfers: &[SolTransfer]) -> Result<()> {
        if transfers.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for transfer in transfers {
            sqlx::query(
                "INSERT OR REPLACE INTO sol_transfers
                 (signature, slot, timestamp, date, from_address, to_address,
                  amount_lamports, amount_sol, from_label, to_label,
                  from_category, to_category)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&transfer.signature)
            .bind(transfer.slot as i64)
            .bind(transfer.timestamp)
            .bind(&transfer.date)
            .bind(transfer.from.to_string())
            .bind(transfer.to.to_string())
            .bind(transfer.amount_lamports as i64)
            .bind(transfer.amount_sol)
            .bind(&transfer.from_label)
            .bind(&transfer.to_label)
            .bind(category_to_string(&transfer.from_category))
            .bind(category_to_string(&transfer.to_category))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Position Tracking
    // =========================================================================

    /// Store stake accounts
    /// Uses a single transaction for DELETE + INSERT to prevent data loss on failure
    pub async fn store_stake_accounts(&self, stakes: &[StakeAccountInfo]) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Clear old stake accounts first (inside transaction)
        sqlx::query("DELETE FROM stake_accounts").execute(&mut *tx).await?;

        // Insert new stake accounts
        for s in stakes {
            sqlx::query(
                "INSERT INTO stake_accounts
                 (account, balance_lamports, state, voter, lockup_epoch, is_liquid, snapshot_slot)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(s.account.to_string())
            .bind(s.balance_lamports as i64)
            .bind(s.state.to_string())
            .bind(s.voter.map(|v| v.to_string()))
            .bind(s.lockup_epoch.map(|e| e as i64))
            .bind(if s.is_liquid { 1i64 } else { 0i64 })
            .bind(s.snapshot_slot as i64)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Store a historical balance snapshot
    pub async fn store_balance_snapshot(&self, position: &ValidatorPosition, date: &str, epoch: u64) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO balance_history
             (date, epoch, snapshot_slot, vote_account_lamports, identity_lamports,
              withdraw_authority_lamports, stake_liquid_lamports, stake_locked_lamports,
              jitosol_lamports, jitosol_rate, total_lamports, cumulative_income_lamports,
              cumulative_expenses_lamports, cumulative_withdrawals_lamports,
              cumulative_deposits_lamports)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(date)
        .bind(epoch as i64)
        .bind(position.snapshot_slot as i64)
        .bind(position.vote_account_lamports as i64)
        .bind(position.identity_lamports as i64)
        .bind(position.withdraw_authority_lamports as i64)
        .bind(position.stake_accounts_liquid as i64)
        .bind(position.stake_accounts_locked as i64)
        .bind(position.jitosol_lamports as i64)
        .bind(position.jitosol_sol_rate)
        .bind(position.total_assets_lamports as i64)
        .bind(position.lifetime_income_lamports as i64)
        .bind(position.lifetime_expenses_lamports as i64)
        .bind(position.lifetime_withdrawals_lamports as i64)
        .bind(position.lifetime_deposits_lamports as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // =========================================================================
    // Income/Expense Aggregation for Reconciliation
    // =========================================================================

    /// Get total lifetime income in lamports
    /// Includes: staking rewards, leader fees, MEV tips, BAM rewards
    pub async fn get_total_income_lamports(&self) -> Result<u64> {
        // Staking commission rewards
        let rewards: (Option<i64>,) = sqlx::query_as("SELECT SUM(amount_lamports) FROM epoch_rewards")
            .fetch_one(&self.pool)
            .await?;
        let rewards_lamports = rewards.0.unwrap_or(0).max(0) as u64;

        // Leader slot fees
        let leader: (Option<i64>,) = sqlx::query_as("SELECT SUM(total_fees_lamports) FROM leader_fees")
            .fetch_one(&self.pool)
            .await?;
        let leader_lamports = leader.0.unwrap_or(0).max(0) as u64;

        // Jito MEV commission
        let mev: (Option<i64>,) = sqlx::query_as("SELECT SUM(commission_lamports) FROM mev_claims")
            .fetch_one(&self.pool)
            .await?;
        let mev_lamports = mev.0.unwrap_or(0).max(0) as u64;

        // BAM rewards (jitoSOL converted to SOL equivalent at claim time)
        // BAM is in jitoSOL, so we use the SOL equivalent stored at claim time
        let bam: (Option<f64>,) = sqlx::query_as("SELECT SUM(amount_sol_equivalent) FROM bam_claims")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((None,));
        let bam_lamports = ((bam.0.unwrap_or(0.0) * 1_000_000_000.0) as i64).max(0) as u64;

        Ok(rewards_lamports
            .saturating_add(leader_lamports)
            .saturating_add(mev_lamports)
            .saturating_add(bam_lamports))
    }

    /// Get total lifetime expenses in lamports
    /// Includes: vote transaction costs
    /// Note: USD expenses are not included (would need price conversion)
    pub async fn get_total_expenses_lamports(&self) -> Result<u64> {
        // Vote transaction costs
        let vote_costs: (Option<i64>,) = sqlx::query_as("SELECT SUM(total_fee_lamports) FROM vote_costs")
            .fetch_one(&self.pool)
            .await?;

        let doublezero: (Option<i64>,) = sqlx::query_as("SELECT SUM(liability_lamports) FROM doublezero_fees")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((None,));

        Ok(vote_costs
            .0
            .unwrap_or(0)
            .saturating_add(doublezero.0.unwrap_or(0))
            .max(0) as u64)
    }

    /// Get total lifetime withdrawals in lamports
    /// Includes: transfers to exchanges or personal wallets
    pub async fn get_total_withdrawals_lamports(&self, config: &Config) -> Result<u64> {
        // Avoid relying on stored categories (which may be missing/wrong in older caches).
        // Compute withdrawals as transfers FROM our validator accounts TO:
        // - personal wallet
        // - known exchange addresses
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT to_address, amount_lamports
             FROM sol_transfers
             WHERE from_address IN (?, ?, ?)",
        )
        .bind(config.vote_account.to_string())
        .bind(config.identity.to_string())
        .bind(config.withdraw_authority.to_string())
        .fetch_all(&self.pool)
        .await?;

        let mut total: u64 = 0;
        for (to_str, amount) in rows {
            let Ok(to) = Pubkey::from_str(&to_str) else {
                continue;
            };
            if to == config.personal_wallet || crate::addresses::is_exchange(&to) {
                total = total.saturating_add(amount.max(0) as u64);
            }
        }
        Ok(total)
    }

    /// Get total lifetime deposits in lamports
    /// Includes: transfers from personal wallet to validator accounts (seeding)
    pub async fn get_total_deposits_lamports(&self, config: &Config) -> Result<u64> {
        // Compute deposits as transfers FROM the configured personal wallet TO our validator accounts.
        let deposits: (Option<i64>,) = sqlx::query_as(
            "SELECT SUM(amount_lamports) FROM sol_transfers
             WHERE from_address = ?
             AND to_address IN (?, ?, ?)",
        )
        .bind(config.personal_wallet.to_string())
        .bind(config.vote_account.to_string())
        .bind(config.identity.to_string())
        .bind(config.withdraw_authority.to_string())
        .fetch_one(&self.pool)
        .await
        .unwrap_or((None,));

        Ok(deposits.0.unwrap_or(0).max(0) as u64)
    }

    /// Get all income/expense data for reconciliation
    pub async fn get_reconciliation_data(&self, config: &Config) -> Result<crate::positions::IncomeData> {
        let total_income = self.get_total_income_lamports().await?;
        let total_expenses = self.get_total_expenses_lamports().await?;
        let total_withdrawals = self.get_total_withdrawals_lamports(config).await?;
        let total_deposits = self.get_total_deposits_lamports(config).await?;

        Ok(crate::positions::IncomeData {
            total_income_lamports: total_income,
            total_expenses_lamports: total_expenses,
            total_withdrawals_lamports: total_withdrawals,
            total_deposits_lamports: total_deposits,
        })
    }

    /// Get external transfer summary for reconciliation
    /// Returns transfers to/from external addresses (excludes internal validator account transfers)
    ///
    /// `internal_addresses` - addresses to exclude (vote account, identity, withdraw authority)
    pub async fn get_external_transfer_summary(
        &self,
        internal_addresses: &[String],
    ) -> Result<ExternalTransferSummary> {
        // Build exclusion list for SQL
        let internal_set: std::collections::HashSet<&str> = internal_addresses.iter().map(|s| s.as_str()).collect();

        // Deposits IN from external addresses (exclude internal-to-internal)
        let all_deposits: Vec<(String, String, String, String, i64)> = sqlx::query_as(
            "SELECT from_address, to_address, from_label, from_category, SUM(amount_lamports) as total
             FROM sol_transfers
             GROUP BY from_address, to_address, from_label, from_category
             ORDER BY total DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        // Filter: from_address is external (not in internal_addresses)
        let mut deposits_in: Vec<ExternalAddressFlow> = all_deposits
            .iter()
            .filter(|(from, to, _, _, _)| {
                // Include if FROM is external and TO is internal (deposit to us)
                !internal_set.contains(from.as_str()) && internal_set.contains(to.as_str())
            })
            .fold(
                std::collections::HashMap::new(),
                |mut acc, (from, _, label, category, amount)| {
                    let entry = acc
                        .entry(from.clone())
                        .or_insert((label.clone(), category.clone(), 0i64));
                    entry.2 += amount;
                    acc
                },
            )
            .into_iter()
            .map(|(addr, (label, category, amount))| ExternalAddressFlow {
                address: addr,
                label: if label.is_empty() { category } else { label },
                amount_lamports: amount.max(0) as u64,
            })
            .collect();
        deposits_in.sort_by_key(|a| std::cmp::Reverse(a.amount_lamports));
        deposits_in.truncate(10);

        // Withdrawals OUT to external addresses
        let all_withdrawals: Vec<(String, String, String, String, i64)> = sqlx::query_as(
            "SELECT from_address, to_address, to_label, to_category, SUM(amount_lamports) as total
             FROM sol_transfers
             GROUP BY from_address, to_address, to_label, to_category
             ORDER BY total DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        // Filter: to_address is external (not in internal_addresses)
        let mut withdrawals_out: Vec<ExternalAddressFlow> = all_withdrawals
            .iter()
            .filter(|(from, to, _, _, _)| {
                // Include if FROM is internal and TO is external (withdrawal from us)
                internal_set.contains(from.as_str()) && !internal_set.contains(to.as_str())
            })
            .fold(
                std::collections::HashMap::new(),
                |mut acc, (_, to, label, category, amount)| {
                    let entry = acc.entry(to.clone()).or_insert((label.clone(), category.clone(), 0i64));
                    entry.2 += amount;
                    acc
                },
            )
            .into_iter()
            .map(|(addr, (label, category, amount))| ExternalAddressFlow {
                address: addr,
                label: if label.is_empty() { category } else { label },
                amount_lamports: amount.max(0) as u64,
            })
            .collect();
        withdrawals_out.sort_by_key(|a| std::cmp::Reverse(a.amount_lamports));
        withdrawals_out.truncate(10);

        Ok(ExternalTransferSummary {
            deposits_in,
            withdrawals_out,
        })
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    /// Get cache statistics
    pub async fn stats(&self) -> Result<CacheStats> {
        let epoch_rewards: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM epoch_rewards")
            .fetch_one(&self.pool)
            .await?;
        let leader_fees: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM leader_fees")
            .fetch_one(&self.pool)
            .await?;
        let mev_claims: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM mev_claims")
            .fetch_one(&self.pool)
            .await?;
        let bam_claims: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bam_claims")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((0,));
        let doublezero_fees: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM doublezero_fees")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((0,));
        let vote_costs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM vote_costs")
            .fetch_one(&self.pool)
            .await?;
        let prices: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM prices")
            .fetch_one(&self.pool)
            .await?;
        let expenses: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM expenses")
            .fetch_one(&self.pool)
            .await?;
        let recurring_expenses: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM recurring_expenses")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((0,));
        let transfers: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sol_transfers")
            .fetch_one(&self.pool)
            .await
            .unwrap_or((0,));

        Ok(CacheStats {
            epoch_rewards: epoch_rewards.0 as u64,
            leader_fees: leader_fees.0 as u64,
            mev_claims: mev_claims.0 as u64,
            bam_claims: bam_claims.0 as u64,
            doublezero_fees: doublezero_fees.0 as u64,
            vote_costs: vote_costs.0 as u64,
            prices: prices.0 as u64,
            expenses: expenses.0 as u64,
            recurring_expenses: recurring_expenses.0 as u64,
            transfers: transfers.0 as u64,
        })
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Convert a SolTransferRow to a SolTransfer
fn row_to_transfer(r: SolTransferRow) -> Option<SolTransfer> {
    let from = Pubkey::from_str(&r.from_address).ok()?;
    let to = Pubkey::from_str(&r.to_address).ok()?;

    Some(SolTransfer {
        signature: r.signature,
        slot: r.slot as u64,
        timestamp: r.timestamp,
        date: r.date,
        from,
        to,
        amount_lamports: r.amount_lamports as u64,
        amount_sol: r.amount_sol,
        from_label: r.from_label,
        to_label: r.to_label,
        from_category: string_to_category(&r.from_category),
        to_category: string_to_category(&r.to_category),
    })
}

/// Convert AddressCategory to string for storage
fn category_to_string(cat: &AddressCategory) -> &'static str {
    match cat {
        AddressCategory::SolanaFoundation => "SolanaFoundation",
        AddressCategory::JitoMev => "JitoMev",
        AddressCategory::BamRewards => "BamRewards",
        AddressCategory::Exchange => "Exchange",
        AddressCategory::ValidatorSelf => "ValidatorSelf",
        AddressCategory::PersonalWallet => "PersonalWallet",
        AddressCategory::DeFiProtocol => "DeFiProtocol",
        AddressCategory::SystemProgram => "SystemProgram",
        AddressCategory::StakeProgram => "StakeProgram",
        AddressCategory::VoteProgram => "VoteProgram",
        AddressCategory::Unknown => "Unknown",
    }
}

/// Convert string to AddressCategory
fn string_to_category(s: &str) -> AddressCategory {
    match s {
        "SolanaFoundation" => AddressCategory::SolanaFoundation,
        "JitoMev" => AddressCategory::JitoMev,
        "BamRewards" => AddressCategory::BamRewards,
        "Exchange" => AddressCategory::Exchange,
        "ValidatorSelf" => AddressCategory::ValidatorSelf,
        "PersonalWallet" => AddressCategory::PersonalWallet,
        "DeFiProtocol" => AddressCategory::DeFiProtocol,
        "SystemProgram" => AddressCategory::SystemProgram,
        "StakeProgram" => AddressCategory::StakeProgram,
        "VoteProgram" => AddressCategory::VoteProgram,
        _ => AddressCategory::Unknown,
    }
}

/// Cache statistics
#[derive(Debug)]
pub struct CacheStats {
    pub epoch_rewards: u64,
    pub leader_fees: u64,
    pub mev_claims: u64,
    pub bam_claims: u64,
    pub doublezero_fees: u64,
    pub vote_costs: u64,
    pub prices: u64,
    pub expenses: u64,
    pub recurring_expenses: u64,
    pub transfers: u64,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} rewards, {} leader fees, {} MEV claims, {} BAM claims, {} DoubleZero fees, {} vote costs, {} transfers, {} prices, {} expenses, {} recurring",
            self.epoch_rewards,
            self.leader_fees,
            self.mev_claims,
            self.bam_claims,
            self.doublezero_fees,
            self.vote_costs,
            self.transfers,
            self.prices,
            self.expenses,
            self.recurring_expenses
        )
    }
}

/// Summary of external transfers for reconciliation
#[derive(Debug, Default)]
pub struct ExternalTransferSummary {
    /// Deposits received from external addresses
    pub deposits_in: Vec<ExternalAddressFlow>,
    /// Withdrawals sent to external addresses
    pub withdrawals_out: Vec<ExternalAddressFlow>,
}

/// SOL flow to/from an external address
#[derive(Debug)]
pub struct ExternalAddressFlow {
    /// The external address
    pub address: String,
    /// Human-readable label (if known)
    pub label: String,
    /// Total amount in lamports
    pub amount_lamports: u64,
}
