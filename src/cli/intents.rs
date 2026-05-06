//! NEAR Intents (intents.near) helpers.
//!
//! Lets users inspect and withdraw the multi-token balances they hold inside
//! the `intents.near` settlement contract — typically credits left over from
//! a swap whose `recipientType` was `INTENTS`.

use anyhow::{bail, Result};
use near_api::{Contract, NearGas, NearToken};
use owo_colors::OwoColorize;
use serde_json::json;

use crate::cli::token::parse_ft_amount;
use crate::cli::utils;
use crate::constants::token_alias;
use crate::network;
use crate::wallet;

/// NEAR Intents settlement contract on mainnet.
const INTENTS_CONTRACT_ID: &str = "intents.near";

/// Resolve a token name/alias to (defuse_asset_id, contract_id, decimals).
async fn resolve_token(
    input: &str,
    net: &near_api::NetworkConfig,
) -> Result<(String, String, u8)> {
    if let Some((defuse_id, contract_id, decimals)) = token_alias(input) {
        return Ok((defuse_id.to_string(), contract_id.to_string(), decimals));
    }

    let contract_id: near_api::AccountId = input
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown token alias and invalid contract ID: {}", input))?;

    let metadata = near_api::Tokens::ft_metadata(contract_id.clone())
        .fetch_from(net)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch metadata for {}: {}", input, e))?;

    let defuse_id = format!("nep141:{}", contract_id);
    Ok((defuse_id, contract_id.to_string(), metadata.data.decimals))
}

/// `nearw intents balance [token]` — show MT balance(s) inside intents.near.
pub async fn balance(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    token: Option<&str>,
    json_output: bool,
) -> Result<()> {
    mainnet_guard(cli_network)?;

    let w = wallet::load_wallet(wallet_name, Some("mainnet"))?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;
    let intents_contract: near_api::AccountId = INTENTS_CONTRACT_ID.parse()?;

    let token = token.ok_or_else(|| anyhow::anyhow!(
        "specify a token alias or contract ID (e.g. USDC, wrap.near)"
    ))?;
    let (defuse_id, contract_id, decimals) = resolve_token(token, &net).await?;

    let result = Contract(intents_contract)
        .call_function("mt_balance_of", json!({
            "token_id": defuse_id,
            "account_id": sender_id.to_string(),
        }))
        .read_only_raw()
        .fetch_from(&net)
        .await?;

    let raw: String = serde_json::from_slice(&result.data)
        .map_err(|e| anyhow::anyhow!("invalid mt_balance_of response: {}", e))?;
    let formatted = format_amount_with_decimals(&raw, decimals);

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "account_id": sender_id.to_string(),
                "token": contract_id,
                "token_id": defuse_id,
                "balance_raw": raw,
                "balance": formatted,
                "decimals": decimals,
            }))?
        );
    } else {
        println!("{}", "Intents Balance".bold());
        println!();
        println!("  Account:  {}", utils::short_account_id(sender_id.as_ref()));
        println!("  Token:    {}", token.to_uppercase().bold());
        println!("  Contract: {}", contract_id.dimmed());
        println!("  Balance:  {} (raw {})", formatted.bold(), raw.dimmed());
    }

    Ok(())
}

/// `nearw intents withdraw <token> <amount> [--receiver]` — pull FT out of
/// intents.near to an on-chain FT balance.
pub async fn withdraw(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    token: &str,
    amount: &str,
    receiver: Option<&str>,
    confirmed: bool,
    json_output: bool,
) -> Result<()> {
    mainnet_guard(cli_network)?;

    let w = wallet::load_wallet(wallet_name, Some("mainnet"))?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;
    let signer = w.signer()?;
    let intents_contract: near_api::AccountId = INTENTS_CONTRACT_ID.parse()?;

    let (_defuse_id, contract_id, decimals) = resolve_token(token, &net).await?;

    let receiver_id: near_api::AccountId = match receiver {
        Some(r) => r.parse()
            .map_err(|_| anyhow::anyhow!("invalid receiver account: {}", r))?,
        None => sender_id.clone(),
    };

    let raw_amount = parse_ft_amount(amount, decimals)?;

    if !json_output {
        println!("{}", "Intents Withdraw".bold());
        println!();
        println!("  From:     {} (intents.near ledger)", utils::short_account_id(sender_id.as_ref()));
        println!("  To:       {}", receiver_id);
        println!("  Token:    {}", token.to_uppercase().bold());
        println!("  Contract: {}", contract_id.dimmed());
        println!("  Amount:   {}", amount.bold());
        println!();
    }

    if !confirmed {
        let answer = inquire::Confirm::new("Confirm withdraw?")
            .with_default(false)
            .prompt()?;
        if !answer {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let result = Contract(intents_contract)
        .call_function("ft_withdraw", json!({
            "token": contract_id,
            "receiver_id": receiver_id.to_string(),
            "amount": raw_amount.to_string(),
        }))
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(NearGas::from_tgas(50))
        .with_signer(sender_id.clone(), signer)
        .send_to(&net)
        .await
        .map_err(|e| anyhow::anyhow!("ft_withdraw error: {:#}", e))?
        .into_result()
        .map_err(|e| anyhow::anyhow!("ft_withdraw tx failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "tx_hash": tx_hash,
                "from": sender_id.to_string(),
                "receiver_id": receiver_id.to_string(),
                "token": contract_id,
                "amount_raw": raw_amount.to_string(),
                "amount": amount,
                "network": "mainnet",
            }))?
        );
    } else {
        println!("{}", "Withdraw submitted!".green().bold());
        println!(
            "  Tx: {}",
            network::explorer_tx_url("mainnet", &tx_hash).cyan()
        );
        println!(
            "  Verify: nearw view {} ft_balance_of '{{\"account_id\":\"{}\"}}'",
            contract_id, receiver_id
        );
    }

    Ok(())
}

fn mainnet_guard(cli_network: Option<&str>) -> Result<()> {
    let effective = cli_network.unwrap_or("mainnet");
    if effective != "mainnet" {
        bail!("intents.near is mainnet-only");
    }
    Ok(())
}

/// Format a raw amount string with decimals for display.
fn format_amount_with_decimals(raw: &str, decimals: u8) -> String {
    let amount: u128 = match raw.parse() {
        Ok(a) => a,
        Err(_) => return raw.to_string(),
    };
    if decimals == 0 {
        return amount.to_string();
    }
    let divisor = 10u128.pow(decimals as u32);
    let whole = amount / divisor;
    let frac = amount % divisor;
    if frac == 0 {
        whole.to_string()
    } else {
        let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", whole, trimmed)
    }
}

