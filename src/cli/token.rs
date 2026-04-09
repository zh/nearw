use anyhow::{bail, Result};
use near_api::types::tokens::FTBalance;
use near_api::Tokens;
use owo_colors::OwoColorize;
use serde::Deserialize;

use crate::cli::utils;
use crate::wallet;

#[derive(Deserialize)]
struct InventoryResponse {
    inventory: Inventory,
}

#[derive(Deserialize)]
pub struct Inventory {
    pub fts: Vec<FtEntry>,
    pub nfts: Vec<NftEntry>,
}

#[derive(Deserialize)]
pub struct FtEntry {
    pub contract: String,
    pub amount: String,
    pub ft_meta: Option<FtMeta>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct FtMeta {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Deserialize)]
pub struct NftEntry {
    pub contract: String,
    pub quantity: Option<String>,
    pub nft_meta: Option<NftMeta>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct NftMeta {
    pub name: String,
    pub symbol: String,
}

/// Infer network from an account/contract ID suffix.
pub fn network_from_account_id(id: &str) -> String {
    if id.ends_with(".testnet") {
        "testnet".to_string()
    } else {
        "mainnet".to_string()
    }
}

/// Nearblocks API base URL for the given network.
pub fn nearblocks_api(network: &str) -> &'static str {
    match network {
        "testnet" => "https://api-testnet.nearblocks.io/v1",
        _ => "https://api.nearblocks.io/v1",
    }
}

/// Fetch account inventory (FTs + NFTs) from Nearblocks.
pub async fn fetch_inventory(account_id: &str, network: &str) -> Result<Inventory> {
    let url = format!(
        "{}/account/{}/inventory",
        nearblocks_api(network),
        account_id
    );
    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        bail!("nearblocks API returned status {}", resp.status());
    }
    let data: InventoryResponse = resp.json().await?;
    Ok(data.inventory)
}

/// List FT balances via Nearblocks inventory API.
pub async fn list(wallet_name: Option<&str>, cli_network: Option<&str>, json: bool) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;

    let inventory = fetch_inventory(account_id.as_ref(), &w.network).await?;

    if json {
        let tokens: Vec<serde_json::Value> = inventory
            .fts
            .iter()
            .filter(|ft| ft.amount.parse::<u128>().unwrap_or(0) > 0)
            .map(|ft| {
                let (symbol, decimals) = ft
                    .ft_meta
                    .as_ref()
                    .map(|m| (m.symbol.clone(), m.decimals))
                    .unwrap_or(("?".to_string(), 0));
                serde_json::json!({
                    "contract": ft.contract,
                    "amount": ft.amount,
                    "symbol": symbol,
                    "decimals": decimals,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&tokens)?);
        return Ok(());
    }

    println!(
        "{} ({})",
        "Token Balances".bold(),
        utils::short_account_id(account_id.as_ref()),
    );
    println!();

    if inventory.fts.is_empty() {
        println!("  No tokens found.");
        return Ok(());
    }

    for ft in &inventory.fts {
        let amount: u128 = ft.amount.parse().unwrap_or(0);
        if amount == 0 {
            continue;
        }

        if let Some(ref meta) = ft.ft_meta {
            let formatted = utils::format_ft(amount, meta.decimals, &meta.symbol);
            println!("  {}  ({})", formatted.bold(), ft.contract.dimmed());
        } else {
            println!("  {}  ({})", ft.amount.bold(), ft.contract.dimmed());
        }
    }

    Ok(())
}

/// Show FT contract metadata.
pub async fn info(_wallet_name: Option<&str>, contract: &str, cli_network: Option<&str>) -> Result<()> {
    let network = cli_network
        .map(|n| n.to_string())
        .unwrap_or_else(|| network_from_account_id(contract));
    let net = crate::network::get_network_config(&network)?;

    let contract_id: near_api::AccountId = contract
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid contract ID: {}", contract))?;

    let metadata = Tokens::ft_metadata(contract_id).fetch_from(&net).await?;

    let m = &metadata.data;
    println!("{}", "FT Metadata".bold());
    println!();
    println!("  Name:     {}", m.name.bold());
    println!("  Symbol:   {}", m.symbol);
    println!("  Decimals: {}", m.decimals);
    if let Some(ref icon) = m.icon {
        if !icon.is_empty() {
            println!("  Icon:     (present)");
        }
    }
    if let Some(ref reference) = m.reference {
        println!("  Ref:      {}", reference);
    }

    Ok(())
}

/// Send fungible tokens.
pub async fn send(
    wallet_name: Option<&str>,
    receiver: &str,
    amount: &str,
    contract: &str,
    cli_network: Option<&str>,
    confirmed: bool,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;

    let receiver_id = crate::cli::utils::resolve_recipient(receiver, &w.network)?;

    let contract_id: near_api::AccountId = contract
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid contract ID: {}", contract))?;

    // Fetch metadata to get decimals
    let metadata = Tokens::ft_metadata(contract_id.clone())
        .fetch_from(&net)
        .await?;

    // Parse amount with decimals
    let ft_amount = parse_ft_amount(amount, metadata.data.decimals)?;
    let ft_balance = FTBalance::with_decimals(metadata.data.decimals).with_amount(ft_amount);

    let formatted = utils::format_ft(ft_amount, metadata.data.decimals, &metadata.data.symbol);

    println!("{}", "Send FT".bold());
    println!();
    println!(
        "  From:     {}",
        utils::short_account_id(sender_id.as_ref())
    );
    println!("  To:       {}", receiver_id.to_string().cyan());
    println!("  Amount:   {}", formatted.bold());
    println!("  Contract: {}", contract);
    println!("  Network:  {}", w.network);
    // FT transfers require 1 yoctoNEAR deposit — only full-access keys can do this
    println!("  Key:      {}", "full-access (ft_transfer requires deposit)".dimmed());
    println!();

    if !confirmed {
        let answer = inquire::Confirm::new("Confirm FT send?")
            .with_default(false)
            .prompt()?;
        if !answer {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let signer = w.signer()?;

    let result = Tokens::account(sender_id)
        .send_to(receiver_id)
        .ft(contract_id, ft_balance)
        .with_signer(signer)
        .send_to(&net)
        .await?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();
    println!("{}", "Sent!".green().bold());
    println!(
        "  Tx: {}",
        crate::network::explorer_tx_url(&w.network, &tx_hash).cyan()
    );

    Ok(())
}

/// Parse a human-readable FT amount into raw u128 given decimals.
pub fn parse_ft_amount(input: &str, decimals: u8) -> Result<u128> {
    let trimmed = input.trim();
    if trimmed.starts_with('-') {
        bail!("amount must be positive");
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    if parts.len() > 2 {
        bail!("invalid amount: {}", input);
    }

    let whole: u128 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid amount: {}", input))?;

    let frac_raw: u128 = if parts.len() == 2 {
        let frac_str = parts[1];
        if frac_str.len() > decimals as usize {
            bail!("too many decimal places (max {})", decimals);
        }
        let padded = format!("{:0<width$}", frac_str, width = decimals as usize);
        padded
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid amount: {}", input))?
    } else {
        0
    };

    let divisor = 10u128.pow(decimals as u32);
    let raw = whole
        .checked_mul(divisor)
        .and_then(|w| w.checked_add(frac_raw))
        .ok_or_else(|| anyhow::anyhow!("amount too large"))?;

    if raw == 0 {
        bail!("amount must be greater than zero");
    }

    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ft_amount_with_6_decimals() {
        let raw = parse_ft_amount("100", 6).unwrap();
        assert_eq!(raw, 100_000_000);
    }

    #[test]
    fn test_ft_amount_with_18_decimals() {
        let raw = parse_ft_amount("1.5", 18).unwrap();
        assert_eq!(raw, 1_500_000_000_000_000_000);
    }

    #[test]
    fn test_ft_amount_with_0_decimals() {
        let raw = parse_ft_amount("42", 0).unwrap();
        assert_eq!(raw, 42);
    }
}
