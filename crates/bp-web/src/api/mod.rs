mod http;
mod jito;
mod sfdp;
mod solana_rpc;
mod stakewiz;

// Types always available (for serialization on both sides)
pub use jito::{JitoEpochReward, JitoMevHistory, format_lamports_to_sol};
pub use sfdp::SfdpStatus;
pub use solana_rpc::NetworkComparison;
pub use stakewiz::{StakewizValidator, format_percent, format_stake};

// Fetch functions only on server (avoids CORS issues from client-side requests)
#[cfg(feature = "ssr")]
pub use jito::get_jito_mev_history;
#[cfg(feature = "ssr")]
pub use sfdp::get_sfdp_status;
#[cfg(feature = "ssr")]
pub use solana_rpc::get_network_comparison;
#[cfg(feature = "ssr")]
pub use stakewiz::get_validator_data;
