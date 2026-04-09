use anyhow::{bail, Result};
use near_api::{NetworkConfig, RPCEndpoint};

use crate::config::load_config;

/// Get a NetworkConfig for the given network name, applying custom RPC if configured.
pub fn get_network_config(network: &str) -> Result<NetworkConfig> {
    let mut config = match network {
        "mainnet" => NetworkConfig::mainnet(),
        "testnet" => NetworkConfig::testnet(),
        _ => bail!(
            "unsupported network: {} (use 'mainnet' or 'testnet')",
            network
        ),
    };

    // Apply custom RPC from config.toml if available
    if let Ok(nearw_config) = load_config() {
        if let Some(rpc) = nearw_config.rpc {
            let urls = match network {
                "mainnet" => rpc.mainnet,
                "testnet" => rpc.testnet,
                _ => None,
            };
            if let Some(urls) = urls {
                if let Some(first_url) = urls.first() {
                    // Security: block non-HTTPS on mainnet, warn on testnet
                    if !first_url.starts_with("https://") {
                        if network == "mainnet"
                            && std::env::var("NEARW_ALLOW_INSECURE").as_deref() != Ok("1")
                        {
                            bail!(
                                "non-HTTPS RPC endpoints are blocked on mainnet for security. \
                                 Use NEARW_ALLOW_INSECURE=1 to override."
                            );
                        }
                        eprintln!("WARNING: Custom RPC endpoint is not HTTPS: {}", first_url);
                        eprintln!(
                            "         Transactions may be intercepted by a network attacker."
                        );
                    }
                    if let Ok(url) = first_url.parse() {
                        config.rpc_endpoints = vec![RPCEndpoint::new(url)];
                        eprintln!("Using custom RPC: {}", first_url);
                    }
                }
            }
        }
    }

    Ok(config)
}

/// Get a nearblocks transaction URL.
pub fn explorer_tx_url(network: &str, tx_hash: &str) -> String {
    match network {
        "testnet" => format!("https://testnet.nearblocks.io/txns/{}", tx_hash),
        _ => format!("https://nearblocks.io/txns/{}", tx_hash),
    }
}

/// Get a nearblocks account URL.
pub fn explorer_account_url(network: &str, account_id: &str) -> String {
    match network {
        "testnet" => format!("https://testnet.nearblocks.io/address/{}", account_id),
        _ => format!("https://nearblocks.io/address/{}", account_id),
    }
}
