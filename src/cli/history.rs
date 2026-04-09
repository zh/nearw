use anyhow::Result;
use owo_colors::OwoColorize;
use serde::Deserialize;

use crate::cli::token::nearblocks_api;
use crate::cli::utils;
use crate::wallet;

#[derive(Deserialize)]
struct TxnsResponse {
    txns: Vec<Txn>,
}

#[derive(Deserialize)]
struct Txn {
    transaction_hash: String,
    predecessor_account_id: String,
    receiver_account_id: String,
    block_timestamp: String,
    actions: Vec<TxAction>,
    outcomes: Option<TxOutcome>,
}

#[derive(Deserialize)]
struct TxAction {
    action: String,
    method: Option<String>,
    deposit: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct TxOutcome {
    status: bool,
}

/// Show transaction history for the wallet's account.
pub async fn run(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    limit: u32,
    json: bool,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;

    // Request extra to account for deduplication (one tx can have multiple receipts)
    let api_limit = limit.saturating_mul(3).min(100);
    let url = format!(
        "{}/account/{}/txns?limit={}",
        nearblocks_api(&w.network),
        account_id,
        api_limit
    );

    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        anyhow::bail!("nearblocks API returned status {}", resp.status());
    }
    let data: TxnsResponse = resp.json().await?;

    if data.txns.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("  No transactions found.");
        }
        return Ok(());
    }

    // Deduplicate by transaction_hash — API returns receipts, one tx can have many
    let mut seen = std::collections::HashSet::new();
    let unique_txns: Vec<&Txn> = data
        .txns
        .iter()
        .filter(|tx| seen.insert(tx.transaction_hash.clone()))
        .take(limit as usize)
        .collect();

    if json {
        let txns: Vec<serde_json::Value> = unique_txns
            .iter()
            .map(|tx| {
                let actions: Vec<serde_json::Value> = tx
                    .actions
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "action": a.action,
                            "method": a.method,
                            "deposit": a.deposit,
                        })
                    })
                    .collect();
                serde_json::json!({
                    "tx_hash": tx.transaction_hash,
                    "from": tx.predecessor_account_id,
                    "to": tx.receiver_account_id,
                    "timestamp": tx.block_timestamp,
                    "actions": actions,
                    "status": tx.outcomes.as_ref().map(|o| o.status),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&txns)?);
        return Ok(());
    }

    println!(
        "{} ({})",
        "Transaction History".bold(),
        utils::short_account_id(account_id.as_ref()),
    );
    println!();

    let account_str = account_id.to_string();

    for tx in &unique_txns {
        let status = match &tx.outcomes {
            Some(o) if o.status => "✓".green().to_string(),
            Some(_) => "✗".red().to_string(),
            None => "?".dimmed().to_string(),
        };

        // Determine direction
        let is_outgoing = tx.predecessor_account_id == account_str;

        // Format timestamp
        let ts = format_timestamp(&tx.block_timestamp);

        // Format action summary
        let action_summary = format_actions(&tx.actions, is_outgoing, &account_str);

        // Direction + counterparty
        let (direction, counterparty) = if is_outgoing {
            ("→".red().to_string(), &tx.receiver_account_id)
        } else {
            ("←".green().to_string(), &tx.predecessor_account_id)
        };

        println!(
            "  {} {} {} {}  {}",
            status,
            ts.dimmed(),
            direction,
            utils::short_account_id(counterparty),
            action_summary,
        );
        println!("    {}", tx.transaction_hash.dimmed());
    }

    Ok(())
}

fn format_actions(actions: &[TxAction], _is_outgoing: bool, _account: &str) -> String {
    if actions.is_empty() {
        return "unknown".to_string();
    }

    let mut parts = Vec::new();
    for a in actions {
        match a.action.as_str() {
            "TRANSFER" => {
                if let Some(ref deposit_val) = a.deposit {
                    let deposit_str = match deposit_val {
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::String(s) => s.clone(),
                        _ => "0".to_string(),
                    };
                    // Strip decimal point if present (API may return as float)
                    let clean = deposit_str.split('.').next().unwrap_or("0");
                    if let Ok(yocto) = clean.parse::<u128>() {
                        if yocto > 0 {
                            parts.push(crate::cli::utils::format_near(yocto));
                        }
                    }
                }
            }
            "FUNCTION_CALL" => {
                if let Some(ref method) = a.method {
                    parts.push(method.clone());
                } else {
                    parts.push("call".to_string());
                }
            }
            "CREATE_ACCOUNT" => parts.push("create-account".to_string()),
            "ADD_KEY" => parts.push("add-key".to_string()),
            "DELETE_KEY" => parts.push("delete-key".to_string()),
            "DEPLOY_CONTRACT" => parts.push("deploy".to_string()),
            "DELETE_ACCOUNT" => parts.push("delete-account".to_string()),
            "STAKE" => parts.push("stake".to_string()),
            other => parts.push(other.to_lowercase()),
        }
    }

    if parts.is_empty() {
        "tx".to_string()
    } else {
        parts.join(", ")
    }
}

fn format_timestamp(ts_str: &str) -> String {
    // Nearblocks returns nanosecond timestamps as strings
    let ts_nanos: u64 = ts_str.parse().unwrap_or(0);
    if ts_nanos == 0 {
        return "unknown".to_string();
    }
    let ts_secs = ts_nanos / 1_000_000_000;

    // Simple relative time
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let diff = now.saturating_sub(ts_secs);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 86400 * 30 {
        format!("{}d ago", diff / 86400)
    } else {
        format!("{}mo ago", diff / (86400 * 30))
    }
}
