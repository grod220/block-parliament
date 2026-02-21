# Tax Report (`tax` subcommand)

A withdrawal-based tax reporting tool that generates a CSV and console summary of validator revenue, expenses, and SFDP reimbursements — ready for an accountant.

## Quick Start

```bash
# Full history
cargo run -p validator-accounting -- tax

# Single tax year
cargo run -p validator-accounting -- tax --year 2025
```

**Output:**
- `./output/tax_report.csv` — detailed tax event ledger
- `./output/tax_schedule_c.csv` (or `tax_schedule_c_<YEAR>.csv` with `--year`) — Schedule C line mapping
- `./output/tax_schedule_c_other_expenses.csv` (or `tax_schedule_c_other_expenses_<YEAR>.csv`) — detail table for the “Other expenses” line

---

## How It Works

### Revenue Recognition: Cash-Basis (Withdrawal = Realization)

Revenue is **not** recognized when SOL accumulates in the vote account. It is recognized when SOL leaves the validator business accounts to an external address:

| Transfer destination | Treatment |
|---------------------|-----------|
| Known exchange (Coinbase, Kraken, etc.) | Revenue |
| Personal wallet (`personal_wallet` in config) | Revenue |
| Any unlabeled external address | Revenue |
| Internal (vote account ↔ identity) | Ignored |

This means SOL sitting in your vote account is **not** taxable until you withdraw it.

### Return of Capital

Initial seed funds (SOL deposited into the validator from external sources) are tracked and offset against early withdrawals chronologically. If you seeded 20 SOL and later withdrew 25 SOL, the first 20 SOL of withdrawals are **non-taxable return of capital** and only the remaining 5 SOL is taxable revenue.

Capital consumption works **across years**: if you seed in 2025 and withdraw in 2026, the 2026 report correctly accounts for the remaining capital pool.

```
Seed:       20 SOL deposited
Withdraw 1: 15 SOL → 15 SOL return of capital, 0 SOL revenue
Withdraw 2: 10 SOL →  5 SOL return of capital, 5 SOL revenue
                       ↑ pool exhausted
```

### SFDP Reimbursements (Vote Fee Offset)

If enrolled in the [Solana Foundation Delegation Program](https://solana.org/delegation-program), vote costs are partially or fully reimbursed on a declining schedule:

| Months since acceptance | Coverage |
|------------------------|----------|
| 1–3 | 100% |
| 4–6 | 75% |
| 7–9 | 50% |
| 10–12 | 25% |
| 13+ | 0% |

The tax report shows **both** the gross vote fee expense and the SFDP reimbursement as separate line items. They cancel out in the net calculation, providing a clear audit trail:

```
Vote Fees (gross):     2.155 SOL = $265.52   ← Expense
SFDP reimbursement:    2.155 SOL = $265.52   ← Reimbursement (offsets above)
                                     Net: $0
```

> [!NOTE]
> SFDP reimbursements are **calculated** using the config-defined coverage schedule, not from raw on-chain SFDP transfers. This matches the main financial report and ensures reimbursements never exceed vote costs. Set `sfdp_acceptance_date` in `config.toml` to enable.

### Expenses

| Category | Source | On-chain? |
|----------|--------|-----------|
| Vote Fees | Estimated from epoch vote counts × typical fee | Yes (Dune) |
| DoubleZero | 5% of leader fees paid to DoubleZero PDA | Yes |
| Hosting | Notion hours database | No |
| Contractor | Notion hours database | No |
| Software | Notion hours database | No |

### Net Taxable Income Formula

```
Net = Revenue + Reimbursements − Expenses
    = Revenue − (Gross Expenses − Reimbursements)
```

Reimbursements offset gross expenses. If SFDP covers 100% of vote fees, vote fees and reimbursements cancel out and do not affect the net.

---

## Configuration

All settings live in `config.toml`:

```toml
[validator]
vote_account = "4PL2Z..."
identity = "mD1af..."
withdraw_authority = "AN58n..."
personal_wallet = "CDfxi..."
commission_percent = 5
first_reward_epoch = 900
bootstrap_date = "2025-11-19"
sfdp_acceptance_date = "2025-12-16"   # Omit if not in SFDP
```

| Field | Purpose |
|-------|---------|
| `personal_wallet` | Withdrawals to this address are treated as revenue |
| `first_reward_epoch` | First epoch the validator earned rewards (filters noise) |
| `bootstrap_date` | Date the validator started — used for transfer history fetch |
| `sfdp_acceptance_date` | Date accepted into SFDP — triggers coverage schedule |

---

## CSV Output

The generated `tax_report.csv` has these columns:

| Column | Description |
|--------|-------------|
| `Date` | YYYY-MM-DD |
| `Type` | `Revenue`, `Expense`, `Return of Capital`, or `Reimbursement` |
| `Category` | e.g. `Withdrawal`, `Vote Fees`, `SFDP Vote Fee Reimbursement`, `Hosting` |
| `Description` | Human-readable detail |
| `SOL Amount` | Amount in SOL (blank for off-chain expenses) |
| `SOL Price (USD)` | Price on that date (blank for off-chain) |
| `USD Value` | SOL × price, or direct USD for off-chain expenses |
| `Destination` | Shortened pubkey for withdrawals |
| `Tx Signature` | On-chain signature (blank for off-chain/estimated) |

Rows are sorted by date, then Revenue → Return of Capital → Reimbursement → Expense within each day.

---

## Console Summary

```
══════════════════════════════════════════════════
  TAX REPORT SUMMARY (2025)
══════════════════════════════════════════════════

  REVENUE (External Withdrawals)
  ─────────────────────────────────────────────
    Return of capital:  4.000005 SOL = $493.32 (non-taxable)
    Taxable revenue:   0 withdrawal(s): 0.000000 SOL = $0.00

  REIMBURSEMENTS (SFDP)
  ─────────────────────────────────────────────
    SFDP:              4 entries  8.623175 SOL = $1064.49

  EXPENSES (Period Costs)
  ─────────────────────────────────────────────
    Contractor             6 entries              $150.00
    DoubleZero             2 entries  0.113012 SOL = $14.04
    Hosting                3 entries              $1305.00
    Software               1 entries              $25.00
    Vote Fees              4 entries  8.623175 SOL = $1064.49
  ─────────────────────────────────────────────
                                      Total: $2558.52

  ═════════════════════════════════════════════
  NET TAXABLE INCOME:                $-1494.04
  ═════════════════════════════════════════════
```

---

## Architecture

### Source Files

| File | Role |
|------|------|
| `src/tax_report.rs` | Report generation, row builders, console summary |
| `src/main.rs` (`handle_tax_command`) | CLI entry point, data loading, transfer categorization |
| `src/transactions.rs` (`categorize_transfers`) | Classifies transfers into withdrawals, seeding, SFDP, other |
| `src/config.rs` (`sfdp_coverage_percent`) | SFDP declining coverage schedule |
| `src/prices.rs` | SOL/USD price cache (CoinGecko) |
| `src/vote_costs.rs` | Per-epoch vote cost estimation (Dune) |

### Data Flow

```
config.toml
     ↓
handle_tax_command()
     ├─ fetch_transfers_with_cache()  →  SolTransfer[]
     │    └─ categorize_transfers()   →  CategorizedTransfers
     ├─ fetch_vote_costs()            →  EpochVoteCost[]
     ├─ fetch_expenses()              →  Expense[]
     └─ load_prices()                 →  PriceCache
          ↓
     generate_tax_report()
          ├─ add_withdrawal_rows()       (Revenue + Return of Capital)
          ├─ add_vote_cost_rows()        (Expense + SFDP Reimbursement)
          ├─ add_doublezero_rows()       (Expense)
          ├─ add_offchain_expense_rows() (Expense)
          ↓
     tax_report.csv + console summary
```

### Key Data Structures

```rust
struct TaxRow {
    date: String,
    entry_type: String,     // "Revenue", "Expense", "Return of Capital", "Reimbursement"
    category: String,       // "Withdrawal", "Vote Fees", "SFDP Vote Fee Reimbursement", ...
    description: String,
    sol_amount: Option<f64>,
    sol_price_usd: Option<f64>,
    usd_value: f64,
    destination: String,
    tx_signature: String,
}

struct TaxReportData<'a> {
    config: &'a Config,
    categorized: &'a CategorizedTransfers,
    vote_costs: &'a [EpochVoteCost],
    doublezero_fees: &'a [DoubleZeroFee],
    expenses: &'a [Expense],
    prices: &'a PriceCache,
}
```

---

## Edge Cases and Known Limitations

| Scenario | Behavior |
|----------|----------|
| Unknown-date transfers | Included with fallback price ($185); skipped counter incremented |
| Multiple seed deposits | All seeds summed into a single capital pool — consumed FIFO |
| Zero taxable revenue | Displays as `0.000000 SOL = $0.00` (no negative-zero) |
| Missing SOL price for a date | Falls back to closest available date's price |
| Epoch straddles SFDP acceptance | Coverage based on epoch end date, same as main report |
| `--year` filter with cross-year capital | Capital consumed from ALL years, only matching rows emitted |
