//! RPC client helpers (avoid system proxy panics on macOS)

use solana_client::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_rpc_client::http_sender::HttpSender;
use solana_rpc_client::rpc_client::RpcClientConfig;
use std::env;

/// Build an RpcClient with system proxy disabled.
///
/// On some macOS environments, system proxy detection can panic. This avoids
/// that path by disabling automatic system proxy usage.
pub fn new_rpc_client(url: &str, commitment: CommitmentConfig) -> RpcClient {
    let mut builder = reqwest_012::Client::builder();

    if should_disable_proxy() {
        builder = builder.no_proxy();
    }

    let client = builder.build().unwrap_or_else(|err| {
        eprintln!(
            "Warning: failed to build custom RPC client ({}); falling back to default client.",
            err
        );
        reqwest_012::Client::new()
    });
    let sender = HttpSender::new_with_client(url.to_string(), client);
    RpcClient::new_sender(sender, RpcClientConfig::with_commitment(commitment))
}

fn should_disable_proxy() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }

    if env::var_os("VALIDATOR_ACCOUNTING_NO_PROXY").is_some() {
        return true;
    }

    if env::var_os("VALIDATOR_ACCOUNTING_USE_PROXY").is_some() {
        return false;
    }

    let proxy_env_vars = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ];

    !proxy_env_vars.iter().any(|key| env::var_os(key).is_some())
}
