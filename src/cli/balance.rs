use anyhow::Result;
use near_api::Tokens;
use owo_colors::OwoColorize;

use crate::cli::utils;
use crate::wallet;

/// Show NEAR balance, or FT balance if --token is specified.
pub async fn run(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    ft_contract: Option<&str>,
    json: bool,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let account_id = w.account_id()?;
    let net = w.network_config()?;

    if let Some(contract) = ft_contract {
        // FT balance
        let ft_contract_id: near_api::AccountId = contract
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid FT contract ID: {}", contract))?;

        let ft_balance = match Tokens::account(account_id.clone())
            .ft_balance(ft_contract_id.clone())
            .fetch_from(&net)
            .await
        {
            Ok(balance) => balance,
            Err(e) => {
                let err_str = format!("{:#}", e);
                if err_str.contains("does not exist") || err_str.contains("account") {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "account": account_id.to_string(),
                                "contract": contract,
                                "balance": "0",
                                "error": "account not found"
                            })
                        );
                    } else {
                        println!(
                            "0 (account {} not yet funded)",
                            utils::short_account_id(account_id.as_ref())
                        );
                    }
                    return Ok(());
                }
                return Err(e.into());
            }
        };

        // Fetch metadata for symbol and decimals
        let metadata = Tokens::ft_metadata(ft_contract_id).fetch_from(&net).await?;

        let formatted = utils::format_ft(
            ft_balance.amount(),
            metadata.data.decimals,
            &metadata.data.symbol,
        );

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "account": account_id.to_string(),
                    "contract": contract,
                    "balance": ft_balance.amount().to_string(),
                    "decimals": metadata.data.decimals,
                    "symbol": metadata.data.symbol,
                    "formatted": formatted,
                })
            );
        } else {
            println!("{}", formatted.bold());
        }
    } else {
        // NEAR balance
        let balance = match Tokens::account(account_id.clone())
            .near_balance()
            .fetch_from(&net)
            .await
        {
            Ok(b) => b,
            Err(e) => {
                let err_str = format!("{:#}", e);
                if err_str.contains("does not exist") || err_str.contains("account") {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "account": account_id.to_string(),
                                "network": w.network,
                                "total": "0",
                                "available": "0",
                                "storage_locked": "0",
                                "staked": "0",
                                "status": "not_funded",
                            })
                        );
                    } else {
                        println!(
                            "{} (account not yet funded on {})",
                            "0 NEAR".bold(),
                            w.network,
                        );
                        println!();
                        println!(
                            "  Send NEAR to {} to activate this account.",
                            account_id.to_string().cyan()
                        );
                    }
                    return Ok(());
                }
                return Err(e.into());
            }
        };

        let available = balance
            .total
            .as_yoctonear()
            .saturating_sub(balance.storage_locked.as_yoctonear())
            .saturating_sub(balance.locked.as_yoctonear());

        if json {
            println!(
                "{}",
                serde_json::json!({
                    "account": account_id.to_string(),
                    "network": w.network,
                    "total": balance.total.as_yoctonear().to_string(),
                    "available": available.to_string(),
                    "storage_locked": balance.storage_locked.as_yoctonear().to_string(),
                    "staked": balance.locked.as_yoctonear().to_string(),
                    "storage_usage": balance.storage_usage,
                })
            );
        } else {
            println!("{}", "NEAR Balance".bold());
            println!();
            println!(
                "  Total:          {}",
                utils::format_near_token(&balance.total).bold()
            );
            println!("  Available:      {}", utils::format_near(available));
            println!(
                "  Storage locked: {}",
                utils::format_near_token(&balance.storage_locked).dimmed()
            );
            println!(
                "  Staked:         {}",
                utils::format_near_token(&balance.locked).dimmed()
            );
        }
    }

    Ok(())
}
