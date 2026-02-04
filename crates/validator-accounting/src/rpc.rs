//! RPC client helpers (avoid system proxy on macOS)

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_rpc_client::http_sender::HttpSender;
use solana_rpc_client::rpc_client::RpcClientConfig;

/// Build an RpcClient with system proxy disabled.
///
/// On some macOS environments, system proxy detection can panic. This avoids
/// that path by disabling automatic system proxy usage.
pub fn new_rpc_client(url: &str, commitment: CommitmentConfig) -> RpcClient {
    let client = reqwest_012::Client::builder()
        .no_proxy()
        .build()
        .expect("build reqwest client");
    let sender = HttpSender::new_with_client(url.to_string(), client);
    RpcClient::new_sender(sender, RpcClientConfig::with_commitment(commitment))
}
