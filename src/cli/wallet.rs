use anyhow::{bail, Result};
use near_api::AccountId;
use owo_colors::OwoColorize;

use crate::storage;
use crate::wallet;

/// Check if a named account is available (does not exist on-chain).
/// Returns Ok(true) if available, Ok(false) if taken.
async fn is_account_available(
    account_id: &AccountId,
    net: &near_api::NetworkConfig,
) -> Result<bool> {
    match near_api::Account(account_id.clone())
        .view()
        .fetch_from(net)
        .await
    {
        Ok(_) => Ok(false), // Account exists — taken
        Err(e) => {
            let msg = format!("{:#}", e);
            // Only treat "unknown account" as available; other errors are real failures
            if msg.contains("UnknownAccount") || msg.contains("does not exist") {
                Ok(true)
            } else {
                Err(anyhow::anyhow!("failed to check account availability: {}", msg))
            }
        }
    }
}

/// After creating an account, verify our public key is an access key on it.
/// Returns Ok(true) if we own it, Ok(false) if someone else does.
async fn verify_account_ownership(
    account_id: &AccountId,
    public_key: &near_api::types::PublicKey,
    net: &near_api::NetworkConfig,
) -> bool {
    near_api::Account(account_id.clone())
        .access_key(*public_key)
        .fetch_from(net)
        .await
        .is_ok()
}

/// Create a new wallet.
pub fn create(name: &str, cli_network: Option<&str>) -> Result<()> {
    let network = cli_network.unwrap_or("mainnet");
    let info = wallet::generate_wallet(name, network)?;

    println!("{}", "Wallet created successfully!".green().bold());
    println!();
    println!("  Name:    {}", info.name.bold());
    println!("  Network: {}", network);
    println!("  Account: {}", info.implicit_account_id.cyan());
    println!("  Key:     {}", info.public_key.to_string().dimmed());
    println!();
    println!(
        "{}",
        "Seed phrase (WRITE THIS DOWN AND KEEP IT SAFE):"
            .yellow()
            .bold()
    );
    println!();
    let words: Vec<&str> = info.mnemonic.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        println!("  {:>2}. {}", i + 1, word);
    }
    println!();
    println!(
        "{}",
        "WARNING: Anyone with this phrase can access your funds.".red()
    );
    println!("{}", "         Store it securely and never share it.".red());

    // Next steps
    println!();
    let suffix = if network == "testnet" {
        ".testnet"
    } else {
        ".near"
    };
    println!("{}", "Next steps:".bold());
    println!(
        "  1. Fund this wallet by sending NEAR to: {}",
        info.implicit_account_id.cyan()
    );
    println!(
        "  2. Or register a named account:  nearw wallet register <name>{}",
        suffix
    );
    if network == "testnet" {
        println!(
            "     (testnet registration is free via faucet)"
        );
    }

    Ok(())
}

/// Import an existing wallet from seed phrase.
pub fn import(name: &str, cli_network: Option<&str>) -> Result<()> {
    let network = cli_network.unwrap_or("mainnet");

    let mnemonic = inquire::Password::new("Enter seed phrase:")
        .with_help_message("12-word BIP39 mnemonic")
        .without_confirmation()
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .prompt()?;

    let info = wallet::import_wallet(name, &mnemonic, network)?;

    println!("{}", "Wallet imported successfully!".green().bold());
    println!();
    println!("  Name:    {}", info.name.bold());
    println!("  Network: {}", network);
    println!("  Account: {}", info.implicit_account_id.cyan());

    Ok(())
}

/// Show wallet info.
pub async fn info(wallet_name: Option<&str>, cli_network: Option<&str>) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;

    println!("{}", "Wallet Info".bold());
    println!();
    println!("  Wallet:   {}", w.name.bold());
    println!("  Network:  {}", w.network);
    if let Some(ref named) = w.named_account_id {
        println!("  Account:  {}", named.cyan());
        println!(
            "  Implicit: {}",
            w.implicit_account_id.dimmed()
        );
    } else {
        println!("  Account:  {}", w.implicit_account_id.cyan());
    }
    println!("  Key:      {}", w.public_key.to_string().dimmed());

    let is_default = storage::get_default_wallet()?.as_deref() == Some(&w.name);
    if is_default {
        println!("  Default: {}", "yes".green());
    }

    // Verify named account ownership if one is set
    let account_id = w.account_id()?;
    let net = w.network_config()?;

    if let Some(ref named) = w.named_account_id {
        if !verify_account_ownership(
            &named.parse()?,
            &w.public_key,
            &net,
        )
        .await
        {
            println!();
            println!(
                "  {} '{}' is not controlled by this wallet's key.",
                "WARNING:".red().bold(),
                named
            );
            println!(
                "  Run {} to clear it, then register a new name.",
                "nearw wallet unregister".bold()
            );
        }
    }

    // Query on-chain balance
    match near_api::Tokens::account(account_id.clone())
        .near_balance()
        .fetch_from(&net)
        .await
    {
        Ok(balance) => {
            println!();
            println!("{}", "On-chain:".bold());
            println!(
                "  Balance: {}",
                crate::cli::utils::format_near_token(&balance.total)
            );
            println!("  Storage: {} bytes", balance.storage_usage);
            println!(
                "  Staked:  {}",
                crate::cli::utils::format_near_token(&balance.locked)
            );
        }
        Err(_) => {
            println!();
            println!("  {}", "Not funded yet.".yellow());
            println!();
            println!("  Send NEAR to this address to activate:");
            println!("    {}", w.implicit_account_id.cyan().bold());
        }
    }

    println!();
    let explorer_account = w.named_account_id.as_deref().unwrap_or(&w.implicit_account_id);
    println!(
        "  Explorer: {}",
        crate::network::explorer_account_url(&w.network, explorer_account).cyan()
    );

    Ok(())
}

/// Export wallet seed phrase.
pub fn export(wallet_name: Option<&str>) -> Result<()> {
    let resolved = storage::resolve_wallet_name(wallet_name)?;
    let mnemonic = storage::get_mnemonic(&resolved)?
        .ok_or_else(|| anyhow::anyhow!("wallet '{}' mnemonic file is empty", resolved))?;

    println!(
        "{}",
        "WARNING: Your seed phrase controls all funds in this wallet."
            .red()
            .bold()
    );
    println!("{}", "         Do not share it with anyone.".red());
    println!();

    let confirmed = inquire::Confirm::new("Display seed phrase on screen?")
        .with_default(false)
        .with_help_message("Make sure no one can see your screen")
        .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    println!("Wallet: {}", resolved.bold());
    println!();

    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        println!("  {:>2}. {}", i + 1, word);
    }

    Ok(())
}

/// Delete a wallet.
pub fn delete(name: &str) -> Result<()> {
    if !storage::wallet_exists(name)? {
        bail!("wallet '{}' not found", name);
    }

    let confirmed = inquire::Confirm::new(&format!("Delete wallet '{}'?", name))
        .with_default(false)
        .with_help_message("This cannot be undone. Make sure you have your seed phrase backed up.")
        .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    storage::delete_wallet(name)?;
    println!("Wallet '{}' deleted.", name);
    Ok(())
}

/// Set a wallet as the default.
pub fn set_default(name: &str) -> Result<()> {
    if !storage::wallet_exists(name)? {
        bail!("wallet '{}' not found", name);
    }
    storage::set_default_wallet(name)?;
    println!("Default wallet set to '{}'.", name.bold());
    Ok(())
}

/// List all stored wallets.
pub fn list() -> Result<()> {
    let wallets = storage::list_wallets()?;
    if wallets.is_empty() {
        println!("No wallets found. Create one with: nearw wallet create <name>");
        return Ok(());
    }

    let default_wallet = storage::get_default_wallet()?;

    println!("{}", "Wallets:".bold());
    println!();

    for name in &wallets {
        let is_default = default_wallet.as_deref() == Some(name.as_str());
        let network = storage::get_network(name)?.unwrap_or_else(|| "mainnet".to_string());
        let named_account = storage::get_account_id(name)?;

        let marker = if is_default { "*" } else { " " };
        if let Some(account) = named_account {
            println!(
                "  {} {} ({}) → {}",
                marker.green().bold(),
                name.bold(),
                network.dimmed(),
                account.cyan()
            );
        } else {
            println!(
                "  {} {} ({})",
                marker.green().bold(),
                name.bold(),
                network.dimmed()
            );
        }
    }

    if default_wallet.is_some() {
        println!();
        println!("  * = default wallet");
    }

    Ok(())
}

/// Register a named account. Suffix is added automatically based on network.
/// If account_name is None, defaults to the wallet name.
/// e.g. "alice" → "alice.near" (mainnet) or "alice.testnet" (testnet)
pub async fn register(
    wallet_name: Option<&str>,
    account_name: Option<&str>,
    cli_network: Option<&str>,
) -> Result<()> {
    use near_api::{Account, NearToken};

    // If no -n given and account name matches a wallet, use that wallet
    let effective_wallet = match (wallet_name, account_name) {
        (Some(_), _) => wallet_name,                          // explicit -n wins
        (None, Some(acct)) if storage::wallet_exists(acct).unwrap_or(false) => Some(acct),
        _ => wallet_name,                                      // fall through to default
    };

    let w = wallet::load_wallet(effective_wallet, cli_network)?;
    let network = &w.network;
    let net_config = w.network_config()?;

    // Default account name to wallet name
    let account_name = account_name.unwrap_or(&w.name);

    // Auto-append network suffix
    let suffix = match network.as_str() {
        "mainnet" => ".near",
        "testnet" => ".testnet",
        other => bail!("unknown network '{}' — cannot determine account suffix", other),
    };

    // Strip suffix if user provided it anyway
    let base_name = account_name
        .strip_suffix(suffix)
        .unwrap_or(account_name);

    let named_account = format!("{}{}", base_name, suffix);

    // Validate account ID
    let named_id: AccountId = named_account
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid account name: {}", named_account))?;

    let public_key = w.public_key;

    // Check if the name already exists
    if !is_account_available(&named_id, &net_config).await? {
        // Account exists — check if we own it (our key is an access key)
        if verify_account_ownership(&named_id, &public_key, &net_config).await {
            storage::store_account_id(&w.name, &named_account)?;
            println!(
                "{}",
                format!("Account '{}' is already yours — linked to wallet '{}'.", named_account, w.name)
                    .green()
                    .bold()
            );
            return Ok(());
        }
        bail!(
            "'{}' is already taken by someone else. Choose a different name.",
            named_account
        );
    }

    let signer = w.signer()?;
    // Always use implicit account for funding — named account may not exist yet
    let implicit_id: near_api::AccountId = w.implicit_account_id.parse()?;

    // Confirmation prompt (security: value-transferring operation)
    let confirmed = inquire::Confirm::new(&format!(
        "Register '{}' on {} (funded from {})?",
        named_account,
        network,
        crate::cli::utils::short_account_id(&w.implicit_account_id),
    ))
    .with_default(false)
    .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    // On testnet, try faucet first
    if network == "testnet" {
        println!("Requesting testnet faucet...");
        let faucet_result = Account::create_account(named_id.clone())
            .sponsor_by_faucet_service()
            .with_public_key(public_key)
            .expect("infallible")
            .send_to_testnet_faucet()
            .await;

        match faucet_result {
            Ok(resp) if resp.status().is_success() => {
                // Faucet returned success — we created this account with our key,
                // so store it. Skip ownership verification (RPC propagation delay
                // can cause false negatives right after creation).
                storage::store_account_id(&w.name, &named_account)?;
                println!(
                    "{}",
                    format!("Account '{}' created and funded!", named_account)
                        .green()
                        .bold()
                );
                return Ok(());
            }
            Ok(resp) => {
                eprintln!(
                    "Faucet returned status {}. Falling back to self-funding...",
                    resp.status()
                );
            }
            Err(e) => {
                eprintln!("Faucet error: {}. Falling back to self-funding...", e);
            }
        }
    }

    // Fund from implicit account
    println!("Creating account from implicit wallet...");
    let deposit = NearToken::from_millinear(100); // 0.1 NEAR

    let result = Account::create_account(named_id)
        .fund_myself(implicit_id, deposit)
        .with_public_key(public_key)
        .with_signer(signer)
        .send_to(&net_config)
        .await?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();
    storage::store_account_id(&w.name, &named_account)?;
    println!(
        "{}",
        format!("Account '{}' registered!", named_account)
            .green()
            .bold()
    );
    println!(
        "  Tx: {}",
        crate::network::explorer_tx_url(network, &tx_hash).cyan()
    );

    Ok(())
}

/// Clear the named account association for a wallet.
pub fn unregister(wallet_name: Option<&str>) -> Result<()> {
    let resolved = storage::resolve_wallet_name(wallet_name)?;
    let current = storage::get_account_id(&resolved)?;

    match current {
        Some(account) => {
            storage::clear_account_id(&resolved)?;
            println!(
                "Cleared named account '{}' from wallet '{}'.",
                account, resolved
            );
            println!("Wallet will now use its implicit account.");
        }
        None => {
            println!("Wallet '{}' has no named account to clear.", resolved);
        }
    }

    Ok(())
}
