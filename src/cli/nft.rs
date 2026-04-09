use anyhow::Result;
use near_api::Tokens;
use owo_colors::OwoColorize;

use crate::cli::utils;
use crate::wallet;

/// List NFTs. Without a contract, discovers via Nearblocks. With a contract, queries RPC.
pub async fn list(
    wallet_name: Option<&str>,
    contract: Option<&str>,
    cli_network: Option<&str>,
    json: bool,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;

    if let Some(contract) = contract {
        // Query specific contract via RPC
        let net = w.network_config()?;
        let contract_id: near_api::AccountId = contract
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid contract ID: {}", contract))?;

        let tokens = Tokens::account(account_id.clone())
            .nft_assets(contract_id)
            .fetch_from(&net)
            .await?;

        if tokens.data.is_empty() {
            println!(
                "No NFTs found for {} in contract {}",
                utils::short_account_id(account_id.as_ref()),
                contract
            );
            return Ok(());
        }

        println!(
            "{} ({} in {})",
            "NFTs".bold(),
            utils::short_account_id(account_id.as_ref()),
            contract
        );
        println!();

        for token in &tokens.data {
            println!("  Token ID: {}", token.token_id.bold());
            if let Some(ref metadata) = token.metadata {
                if let Some(ref title) = metadata.title {
                    println!("    Title:  {}", title);
                }
                if let Some(ref desc) = metadata.description {
                    println!("    Desc:   {}", desc);
                }
                if let Some(ref media) = metadata.media {
                    println!("    Media:  {}", media.cyan());
                }
            }
            println!();
        }
    } else {
        // Discover NFTs via Nearblocks inventory
        let inventory =
            crate::cli::token::fetch_inventory(account_id.as_ref(), &w.network).await?;

        if json {
            let nfts: Vec<serde_json::Value> = inventory
                .nfts
                .iter()
                .map(|nft| {
                    let name = nft
                        .nft_meta
                        .as_ref()
                        .map(|m| m.name.clone())
                        .unwrap_or_default();
                    serde_json::json!({
                        "contract": nft.contract,
                        "name": name,
                        "quantity": nft.quantity.as_deref().unwrap_or("0"),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&nfts)?);
            return Ok(());
        }

        println!(
            "{} ({})",
            "NFTs".bold(),
            utils::short_account_id(account_id.as_ref()),
        );
        println!();

        if inventory.nfts.is_empty() {
            println!("  No NFTs found.");
            return Ok(());
        }

        for nft in &inventory.nfts {
            let qty = nft.quantity.as_deref().unwrap_or("?");
            if let Some(ref meta) = nft.nft_meta {
                println!(
                    "  {} x{}  ({})",
                    meta.name.bold(),
                    qty,
                    nft.contract.dimmed()
                );
            } else {
                println!("  {} x{}", nft.contract.bold(), qty);
            }
        }
    }

    Ok(())
}

/// Show NFT contract metadata.
pub async fn info(_wallet_name: Option<&str>, contract: &str, cli_network: Option<&str>) -> Result<()> {
    let network = cli_network
        .map(|n| n.to_string())
        .unwrap_or_else(|| crate::cli::token::network_from_account_id(contract));
    let net = crate::network::get_network_config(&network)?;

    let contract_id: near_api::AccountId = contract
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid contract ID: {}", contract))?;

    let metadata = Tokens::nft_metadata(contract_id).fetch_from(&net).await?;

    let m = &metadata.data;
    println!("{}", "NFT Metadata".bold());
    println!();
    println!("  Name:     {}", m.name.bold());
    println!("  Symbol:   {}", m.symbol);
    println!("  Spec:     {}", m.spec);
    if let Some(ref base_uri) = m.base_uri {
        println!("  Base URI: {}", base_uri);
    }
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

/// Send an NFT.
pub async fn send(
    wallet_name: Option<&str>,
    receiver: &str,
    contract: &str,
    token_id: &str,
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

    println!("{}", "Send NFT".bold());
    println!();
    println!(
        "  From:     {}",
        utils::short_account_id(sender_id.as_ref())
    );
    println!("  To:       {}", receiver_id.to_string().cyan());
    println!("  Token:    {}", token_id.bold());
    println!("  Contract: {}", contract);
    println!("  Network:  {}", w.network);
    // NFT transfers require 1 yoctoNEAR deposit — only full-access keys can do this
    println!("  Key:      {}", "full-access (nft_transfer requires deposit)".dimmed());
    println!();

    if !confirmed {
        let answer = inquire::Confirm::new("Confirm NFT send?")
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
        .nft(contract_id, token_id.to_string())
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
