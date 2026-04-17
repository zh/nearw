use anyhow::Result;
use near_api::{Contract, NearGas, NearToken};
use owo_colors::OwoColorize;
use serde_json::Value;

use crate::cli::token::network_from_account_id;
use crate::cli::utils;
use crate::network;
use crate::wallet;

/// Execute a read-only view call on a contract.
pub async fn view(
    cli_network: Option<&str>,
    contract: &str,
    method: &str,
    args: Option<&str>,
) -> Result<()> {
    let network = cli_network
        .map(|n| n.to_string())
        .unwrap_or_else(|| network_from_account_id(contract));
    let net = network::get_network_config(&network)?;

    let contract_id: near_api::AccountId = contract
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid contract ID: {}", contract))?;

    let args_json: Value = match args {
        Some(s) => serde_json::from_str(s)
            .map_err(|e| anyhow::anyhow!("invalid JSON args: {}", e))?,
        None => serde_json::json!({}),
    };

    let result = Contract(contract_id)
        .call_function(method, args_json)
        .read_only_raw()
        .fetch_from(&net)
        .await?;

    // result is Data<Vec<u8>>. Attempt JSON parse; fall back to raw string.
    match serde_json::from_slice::<Value>(&result.data) {
        Ok(json) => println!("{}", serde_json::to_string_pretty(&json)?),
        Err(_) => {
            // Not valid JSON -- print as UTF-8 string or hex
            match String::from_utf8(result.data.clone()) {
                Ok(s) => println!("{}", s),
                Err(_) => println!("0x{}", hex::encode(&result.data)),
            }
        }
    }

    Ok(())
}

/// Execute a state-changing contract call.
pub async fn run(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    contract: &str,
    method: &str,
    args: &str,
    deposit: Option<&str>,
    gas: Option<u64>,
    confirmed: bool,
    json_output: bool,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;

    let contract_id: near_api::AccountId = contract
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid contract ID: {}", contract))?;

    let args_json: Value = serde_json::from_str(args)
        .map_err(|e| anyhow::anyhow!("invalid JSON args: {}", e))?;

    // Parse deposit: accept NEAR (decimal) or yoctoNEAR (integer with "yocto" suffix)
    let deposit_amount = match deposit {
        Some(d) if d.ends_with("yocto") => {
            let raw = d.trim_end_matches("yocto");
            let yocto: u128 = raw.parse()
                .map_err(|_| anyhow::anyhow!("invalid yoctoNEAR deposit: {}", d))?;
            NearToken::from_yoctonear(yocto)
        }
        Some(d) => crate::cli::send::parse_near_amount(d, "near")?,
        None => NearToken::from_yoctonear(0),
    };

    let gas_amount = NearGas::from_tgas(gas.unwrap_or(100));

    // Display confirmation
    println!("{}", "Contract Call".bold());
    println!();
    println!("  Caller:   {}", utils::short_account_id(sender_id.as_ref()));
    println!("  Contract: {}", contract.cyan());
    println!("  Method:   {}", method);
    println!("  Args:     {}", args);
    if deposit_amount.as_yoctonear() > 0 {
        println!("  Deposit:  {}", utils::format_near_token(&deposit_amount).bold());
    }
    println!("  Gas:      {} TGas", gas_amount.as_tgas());
    println!("  Network:  {}", w.network);
    println!();

    if !confirmed {
        let answer = inquire::Confirm::new("Confirm call?")
            .with_default(false)
            .prompt()?;
        if !answer {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let signer = w.signer()?;
    let sender_str = sender_id.to_string();

    let result = Contract(contract_id)
        .call_function(method, args_json)
        .transaction()
        .deposit(deposit_amount)
        .gas(gas_amount)
        .with_signer(sender_id, signer)
        .send_to(&net)
        .await
        .map_err(|e| anyhow::anyhow!("transaction error: {:#}", e))?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "tx_hash": tx_hash,
                "caller": sender_str,
                "contract": contract,
                "method": method,
                "network": w.network,
            })
        );
    } else {
        println!("{}", "Done!".green().bold());
        println!(
            "  Tx: {}",
            crate::network::explorer_tx_url(&w.network, &tx_hash).cyan()
        );
    }

    Ok(())
}
