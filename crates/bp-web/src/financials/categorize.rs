//! String-based transfer categorization (no Solana SDK).
//!
//! Mirrors `validator-accounting/src/transactions.rs::categorize_transfers()`
//! but operates on string addresses instead of `Pubkey`.

use std::collections::HashSet;
use std::sync::LazyLock;

use super::config::ValidatorConfig;
use super::types::{CategorizedTransfers, SolTransfer};

// ── Known address sets ────────────────────────────────────────────────────────
// Ported from validator-accounting/src/addresses.rs (string-only, no Pubkey).

static SF_ADDRESSES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "mpa4abUkjQoAvPzREkh5Mo75hZhPFQ2FSH6w7dWKuQ5",  // Solana Foundation main
        "7K8DVxtNJGnMtUY1CQJT5jcs8sFGSZTDiG7kowvFpECh", // SF Stake Authority
        "DRpbCBMxVnDK7maPM5tGv6MvB3v1sRMC86PZ8okm21hy", // SF Delegation Program
        "4ZJhPQAgUseCsWhKvJLTmmRRUV74fdoTpQLNfKoHtFSP", // SF Operations
        "DtZWL3BPKa5hw7yQYvaFR29PcXThpLHVU2XAAZrcLiSe", // SFDP Vote Reimbursement
    ]
    .into_iter()
    .collect()
});

static JITO_ADDRESSES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "T1pyyaTNZsKv2WcRAB8oVnk93mLJw2XzjtVYqCsaHqt",  // Tip Payment Program
        "4R3gSG8BpU4t19KYj8CfnbtRpnT8gtk4dvTHxVRwc2r7", // Tip Distribution Program
        "8F4jGUmxF36vQ6yabnsxX6AQVXdKBhs8kGSUuRKSg8Xt", // Merkle Root Authority
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5", // Tip Account 1
        "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe", // Tip Account 2
        "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY", // Tip Account 3
        "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49", // Tip Account 4
        "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh", // Tip Account 5
        "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt", // Tip Account 6
        "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL", // Tip Account 7
        "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT", // Tip Account 8
    ]
    .into_iter()
    .collect()
});

static EXCHANGE_ADDRESSES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "H8sMJSCQxfKiFTCfDR3DUMLPwcRbM61LGFJ8N4dK3WjS", // Coinbase
        "2AQdpHJ2JpcEgPiATUXjQxA8QmafFegfQwSLWSprPicm", // Binance
        "5tzFkiKscXHK5ZXCGbXZxdw7gTjjD1mBwuoFbhUvuAi9", // Kraken
    ]
    .into_iter()
    .collect()
});

pub fn is_solana_foundation(addr: &str) -> bool {
    SF_ADDRESSES.contains(addr)
}

pub fn is_jito(addr: &str) -> bool {
    JITO_ADDRESSES.contains(addr)
}

pub fn is_exchange(addr: &str) -> bool {
    EXCHANGE_ADDRESSES.contains(addr)
}

// ── Categorize transfers ──────────────────────────────────────────────────────

/// Bucket transfers by purpose using string-based address matching.
///
/// Logic mirrors `transactions.rs:categorize_transfers()` exactly:
///   1. DZ deposit (to == dz_deposit_account && from is ours)
///   2. Incoming to our accounts:
///      - from personal wallet → seeding
///      - from SF → SFDP reimbursement
///      - from Jito → MEV deposit
///      - from our account → vote funding (internal)
///      - else → other
///   3. Outgoing from our accounts:
///      - to exchange or personal wallet → withdrawal
///      - to our account → vote funding (internal)
///      - else → other
pub fn categorize_transfers(transfers: &[SolTransfer], config: &ValidatorConfig) -> CategorizedTransfers {
    let mut cat = CategorizedTransfers::default();

    for t in transfers {
        // 1. DoubleZero deposit
        if let Some(ref dz) = config.doublezero_deposit_account
            && t.to_address == *dz
            && config.is_our_account(&t.from_address)
        {
            let mut labeled = t.clone();
            labeled.to_label = "DoubleZero Deposit".to_string();
            cat.doublezero_payments.push(labeled);
            continue;
        }

        let is_incoming = config.is_our_account(&t.to_address);
        let is_outgoing = config.is_our_account(&t.from_address);

        if is_incoming {
            if t.from_address == config.personal_wallet {
                cat.seeding.push(t.clone());
            } else if is_solana_foundation(&t.from_address) {
                cat.sfdp_reimbursements.push(t.clone());
            } else if is_jito(&t.from_address) {
                cat.mev_deposits.push(t.clone());
            } else if config.is_our_account(&t.from_address) {
                cat.vote_funding.push(t.clone());
            } else {
                cat.other.push(t.clone());
            }
        } else if is_outgoing {
            if is_exchange(&t.to_address) || t.to_address == config.personal_wallet {
                cat.withdrawals.push(t.clone());
            } else if config.is_our_account(&t.to_address) {
                cat.vote_funding.push(t.clone());
            } else {
                cat.other.push(t.clone());
            }
        }
    }

    cat
}
