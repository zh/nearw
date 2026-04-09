use anyhow::{bail, Result};
use near_api::signer::{generate_secret_key, generate_seed_phrase};
use near_api::types::transaction::actions::FunctionCallPermission;
use near_api::types::{AccessKeyPermission, PublicKey};
use near_api::Account;
use owo_colors::OwoColorize;

use crate::cli::utils;
use crate::storage;
use crate::wallet;

/// List all access keys on the wallet's account.
pub async fn list(wallet_name: Option<&str>, cli_network: Option<&str>) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;
    let net = w.network_config()?;

    let keys = Account(account_id.clone())
        .list_keys()
        .fetch_from(&net)
        .await?;

    println!(
        "{} ({})",
        "Access Keys".bold(),
        utils::short_account_id(account_id.as_ref()),
    );
    println!();

    if keys.data.is_empty() {
        println!("  No access keys found.");
        return Ok(());
    }

    for (pk, ak) in &keys.data {
        let pk_str = pk.to_string();
        let is_wallet_key = pk_str == w.public_key.to_string();
        let marker = if is_wallet_key { " (wallet)" } else { "" };

        match &ak.permission {
            AccessKeyPermission::FullAccess => {
                println!(
                    "  {} {}{}",
                    "full-access".red().bold(),
                    pk_str.dimmed(),
                    marker.green()
                );
            }
            AccessKeyPermission::FunctionCall(fc) => {
                let methods = if fc.method_names.is_empty() {
                    "all methods".to_string()
                } else {
                    fc.method_names.join(", ")
                };
                let allowance = match &fc.allowance {
                    Some(a) => utils::format_near_token(a),
                    None => "unlimited".to_string(),
                };
                println!(
                    "  {} → {} [{}] ({})",
                    "function-call".cyan().bold(),
                    fc.receiver_id.bold(),
                    methods,
                    allowance.dimmed()
                );
                println!("    {}", pk_str.dimmed());
            }
        }
    }

    Ok(())
}

/// Add a function-call access key for a contract.
/// Generates a new key, adds it on-chain, and saves the secret locally.
pub async fn add_function_call(
    wallet_name: Option<&str>,
    contract: &str,
    methods: Option<&str>,
    allowance: &str,
    existing_public_key: Option<&str>,
    cli_network: Option<&str>,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;
    let net = w.network_config()?;

    // Resolve contract name
    let contract_id = utils::resolve_recipient(contract, &w.network)?;
    let contract_str = contract_id.to_string();

    // Parse allowance using integer math (same as send's parse_near_amount)
    let allowance_token = crate::cli::send::parse_near_amount(allowance, "near")?;

    // Parse methods
    let method_names: Vec<String> = methods
        .map(|m| m.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    // Use existing public key or generate a new secret key
    let (secret_key, new_public_key) = if let Some(pk_str) = existing_public_key {
        let pk: PublicKey = pk_str
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid public key: {}", pk_str))?;
        // Can't save the secret locally if only public key was provided
        (None, pk)
    } else {
        let sk = generate_secret_key()?;
        let pk = sk.public_key();
        (Some(sk), pk)
    };

    let permission = AccessKeyPermission::FunctionCall(FunctionCallPermission {
        allowance: Some(allowance_token),
        receiver_id: contract_str.clone(),
        method_names: method_names.clone(),
    });

    let methods_display = if method_names.is_empty() {
        "all methods".to_string()
    } else {
        method_names.join(", ")
    };

    println!("{}", "Add Function-Call Key".bold());
    println!();
    println!("  Account:   {}", utils::short_account_id(account_id.as_ref()));
    println!("  Contract:  {}", contract_str.cyan());
    println!("  Methods:   {}", methods_display);
    println!("  Allowance: {}", utils::format_near_token(&allowance_token));
    println!("  Key:       {}", new_public_key.to_string().dimmed());
    if secret_key.is_some() {
        println!("  Stored:    {}", "locally (auto-used for this contract)".green());
    }
    println!();

    let confirmed = inquire::Confirm::new("Add this key?")
        .with_default(false)
        .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    let signer = w.signer()?;

    let result = Account(account_id)
        .add_key(permission, new_public_key)
        .with_signer(signer)
        .send_to(&net)
        .await
        .map_err(|e| {
            let msg = format!("{:#}", e);
            if msg.contains("access key") || msg.contains("does not exist") {
                anyhow::anyhow!("signing key is not a full-access key on this account")
            } else {
                anyhow::anyhow!("transaction error: {}", msg)
            }
        })?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();

    // Save secret key locally for auto-use
    if let Some(sk) = secret_key {
        storage::store_key(&w.name, &contract_str, &sk.to_string())?;
        println!("{}", "Key added and saved locally!".green().bold());
        println!(
            "  Contract calls to {} will now use this key automatically.",
            contract_str.cyan()
        );
    } else {
        println!("{}", "Key added on-chain!".green().bold());
        println!(
            "  {}",
            "Note: secret not stored locally (public key was provided). Use 'key import' to store it.".dimmed()
        );
    }
    println!(
        "  Tx: {}",
        crate::network::explorer_tx_url(&w.network, &tx_hash).cyan()
    );

    Ok(())
}

/// Add a full-access key (dangerous).
pub async fn add_full_access(
    wallet_name: Option<&str>,
    existing_public_key: Option<&str>,
    cli_network: Option<&str>,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;
    let net = w.network_config()?;

    // Use existing key or generate a new one
    let (seed_phrase, new_public_key) = if let Some(pk_str) = existing_public_key {
        let pk: PublicKey = pk_str
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid public key: {}", pk_str))?;
        (None, pk)
    } else {
        let (sp, pk) = generate_seed_phrase()?;
        (Some(sp), pk)
    };

    println!(
        "{}",
        "WARNING: Full-access keys grant complete control over the account."
            .red()
            .bold()
    );
    println!(
        "{}",
        "         Anyone with this key can transfer all funds and delete the account.".red()
    );
    println!();
    println!("  Account: {}", utils::short_account_id(account_id.as_ref()));
    println!("  Key:     {}", new_public_key.to_string().dimmed());
    println!();

    let confirmed = inquire::Confirm::new("Add full-access key? This is dangerous.")
        .with_default(false)
        .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    let signer = w.signer()?;

    let result = Account(account_id)
        .add_key(AccessKeyPermission::FullAccess, new_public_key)
        .with_signer(signer)
        .send_to(&net)
        .await
        .map_err(|e| {
            let msg = format!("{:#}", e);
            if msg.contains("access key") || msg.contains("does not exist") {
                anyhow::anyhow!("signing key is not a full-access key on this account")
            } else {
                anyhow::anyhow!("transaction error: {}", msg)
            }
        })?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();
    println!("{}", "Full-access key added!".green().bold());

    if let Some(sp) = seed_phrase {
        println!();
        println!(
            "  {}",
            "Save this seed phrase — it grants full control:"
                .yellow()
                .bold()
        );
        let words: Vec<&str> = sp.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            println!("    {:>2}. {}", i + 1, word);
        }
    }

    println!(
        "  Tx: {}",
        crate::network::explorer_tx_url(&w.network, &tx_hash).cyan()
    );

    Ok(())
}

/// Delete an access key.
pub async fn delete(
    wallet_name: Option<&str>,
    public_key_str: &str,
    cli_network: Option<&str>,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;
    let net = w.network_config()?;

    let public_key: PublicKey = public_key_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid public key: {}", public_key_str))?;

    // Prevent deleting the wallet's own key
    if public_key.to_string() == w.public_key.to_string() {
        bail!("cannot delete the wallet's own key — this would lock you out of the account");
    }

    println!("{}", "Delete Access Key".bold());
    println!();
    println!("  Account: {}", utils::short_account_id(account_id.as_ref()));
    println!("  Key:     {}", public_key_str.dimmed());
    println!();

    let confirmed = inquire::Confirm::new("Delete this key?")
        .with_default(false)
        .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    let signer = w.signer()?;

    let result = Account(account_id)
        .delete_key(public_key)
        .with_signer(signer)
        .send_to(&net)
        .await
        .map_err(|e| {
            let msg = format!("{:#}", e);
            if msg.contains("access key") || msg.contains("does not exist") {
                anyhow::anyhow!("signing key is not a full-access key on this account")
            } else {
                anyhow::anyhow!("transaction error: {}", msg)
            }
        })?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();
    println!("{}", "Key deleted.".green().bold());
    println!(
        "  Tx: {}",
        crate::network::explorer_tx_url(&w.network, &tx_hash).cyan()
    );

    Ok(())
}

/// Generate a new key pair (display only, does not add to account).
pub fn generate() -> Result<()> {
    let (seed_phrase, public_key) = generate_seed_phrase()?;

    println!("{}", "New Key Pair".bold());
    println!();
    println!("  Public key: {}", public_key.to_string().cyan());
    println!();
    println!("  Seed phrase:");
    let words: Vec<&str> = seed_phrase.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        println!("    {:>2}. {}", i + 1, word);
    }
    println!();
    println!(
        "  {}",
        "Use 'nearw key add <contract>' to add on-chain, or 'nearw key import' to store locally.".dimmed()
    );

    Ok(())
}

/// Import a key from seed phrase and store it locally for a contract.
pub fn import(wallet_name: Option<&str>, contract: &str) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, None)?;
    let contract_id = utils::resolve_recipient(contract, &w.network)?;
    let contract_str = contract_id.to_string();

    let seed_phrase = inquire::Password::new("Enter seed phrase:")
        .with_help_message("12-word BIP39 mnemonic for this function-call key")
        .without_confirmation()
        .with_display_mode(inquire::PasswordDisplayMode::Masked)
        .prompt()?;

    let trimmed = seed_phrase.trim().to_lowercase();

    // Derive secret key from seed phrase
    let secret_key = near_api::signer::generate_secret_key_from_seed_phrase(trimmed)?;
    let public_key = secret_key.public_key();

    storage::store_key(&w.name, &contract_str, &secret_key.to_string())?;

    println!("{}", "Key imported and saved!".green().bold());
    println!();
    println!("  Wallet:   {}", w.name);
    println!("  Contract: {}", contract_str.cyan());
    println!("  Key:      {}", public_key.to_string().dimmed());
    println!();
    println!(
        "  Contract calls to {} will now use this key automatically.",
        contract_str.cyan()
    );

    Ok(())
}
