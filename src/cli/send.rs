use anyhow::{bail, Result};
use near_api::{NearToken, Tokens};
use owo_colors::OwoColorize;

use crate::cli::utils;
use crate::wallet;

/// Parse a NEAR amount string into NearToken.
pub fn parse_near_amount(input: &str, unit: &str) -> Result<NearToken> {
    match unit {
        "yocto" => {
            let yocto: u128 = input
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid yoctoNEAR amount: {}", input))?;
            if yocto == 0 {
                bail!("amount must be greater than zero");
            }
            Ok(NearToken::from_yoctonear(yocto))
        }
        "near" => {
            // Parse as decimal NEAR
            let trimmed = input.trim();
            if trimmed.starts_with('-') {
                bail!("amount must be positive");
            }

            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() > 2 {
                bail!("invalid NEAR amount: {}", input);
            }

            let whole: u128 = parts[0]
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid NEAR amount: {}", input))?;

            let frac_yocto: u128 = if parts.len() == 2 {
                let frac_str = parts[1];
                if frac_str.len() > 24 {
                    bail!("too many decimal places (max 24)");
                }
                // Pad to 24 decimal places
                let padded = format!("{:0<24}", frac_str);
                padded
                    .parse()
                    .map_err(|_| anyhow::anyhow!("invalid NEAR amount: {}", input))?
            } else {
                0
            };

            let yocto = whole
                .checked_mul(1_000_000_000_000_000_000_000_000u128)
                .and_then(|w| w.checked_add(frac_yocto))
                .ok_or_else(|| anyhow::anyhow!("amount too large"))?;
            if yocto == 0 {
                bail!("amount must be greater than zero");
            }
            Ok(NearToken::from_yoctonear(yocto))
        }
        _ => bail!("unknown unit: {} (use 'near' or 'yocto')", unit),
    }
}

/// Send NEAR to a recipient.
pub async fn run(
    wallet_name: Option<&str>,
    receiver: &str,
    amount: &str,
    unit: &str,
    cli_network: Option<&str>,
    confirmed: bool,
    json: bool,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;

    let receiver_id = utils::resolve_recipient(receiver, &w.network)?;

    let token_amount = parse_near_amount(amount, unit)?;

    // Display confirmation
    println!("{}", "Send NEAR".bold());
    println!();
    println!("  From:    {}", utils::short_account_id(sender_id.as_ref()));
    println!("  To:      {}", receiver_id.to_string().cyan());
    println!(
        "  Amount:  {}",
        utils::format_near_token(&token_amount).bold()
    );
    println!("  Network: {}", w.network);
    println!("  Key:     {}", utils::short_account_id(&w.public_key.to_string()).dimmed());
    println!("           {}", "full-access".red());
    println!();

    if !confirmed {
        let answer = inquire::Confirm::new("Confirm send?")
            .with_default(false)
            .prompt()?;
        if !answer {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Pre-check: verify balance is sufficient
    let balance = Tokens::account(sender_id.clone())
        .near_balance()
        .fetch_from(&net)
        .await?;
    let available = balance
        .total
        .as_yoctonear()
        .saturating_sub(balance.storage_locked.as_yoctonear())
        .saturating_sub(balance.locked.as_yoctonear());

    if token_amount.as_yoctonear() > available {
        bail!(
            "insufficient balance: {} available, trying to send {}",
            utils::format_near(available),
            utils::format_near_token(&token_amount)
        );
    }

    let signer = w.signer()?;
    let sender_str = sender_id.to_string();
    let receiver_str = receiver_id.to_string();

    let result = Tokens::account(sender_id)
        .send_to(receiver_id)
        .near(token_amount)
        .with_signer(signer)
        .send_to(&net)
        .await
        .map_err(|e| {
            let msg = format!("{:#}", e);
            if msg.contains("InvalidTransaction") || msg.contains("NotEnoughBalance") {
                anyhow::anyhow!("transaction rejected: insufficient balance or invalid transaction")
            } else if msg.contains("access key") || msg.contains("does not exist") {
                anyhow::anyhow!("signing key is not a full-access key on this account")
            } else {
                anyhow::anyhow!("transaction error: {}", msg)
            }
        })?
        .into_result()
        .map_err(|e| anyhow::anyhow!("transaction failed: {}", e))?;

    let tx_hash = result.outcome().transaction_hash.to_string();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "tx_hash": tx_hash,
                "from": sender_str,
                "to": receiver_str,
                "amount": token_amount.as_yoctonear().to_string(),
                "network": w.network,
            })
        );
    } else {
        println!("{}", "Sent!".green().bold());
        println!(
            "  Tx: {}",
            crate::network::explorer_tx_url(&w.network, &tx_hash).cyan()
        );
    }

    Ok(())
}

/// Send all NEAR to a recipient (drain wallet).
pub async fn run_send_all(
    wallet_name: Option<&str>,
    receiver: &str,
    cli_network: Option<&str>,
) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;

    let receiver_id = utils::resolve_recipient(receiver, &w.network)?;

    // Fetch current balance
    let balance = Tokens::account(sender_id.clone())
        .near_balance()
        .fetch_from(&net)
        .await?;

    let available = balance
        .total
        .as_yoctonear()
        .saturating_sub(balance.storage_locked.as_yoctonear())
        .saturating_sub(balance.locked.as_yoctonear());

    // Reserve 0.005 NEAR for storage (dynamic computation is complex for v1)
    let reserve = NearToken::from_millinear(5).as_yoctonear();

    if available <= reserve {
        bail!(
            "insufficient balance: available {} but need at least {} for storage reserve",
            utils::format_near(available),
            utils::format_near(reserve)
        );
    }

    let send_amount = NearToken::from_yoctonear(available - reserve);

    println!("{}", "Send All NEAR".bold());
    println!();
    println!("  From:    {}", utils::short_account_id(sender_id.as_ref()));
    println!("  To:      {}", receiver_id.to_string().cyan());
    println!(
        "  Amount:  {}",
        utils::format_near_token(&send_amount).bold()
    );
    println!(
        "  Reserve: {} (kept for storage)",
        utils::format_near(reserve).dimmed()
    );
    println!("  Network: {}", w.network);
    println!("  Key:     {}", utils::short_account_id(&w.public_key.to_string()).dimmed());
    println!("           {}", "full-access".red());
    println!();

    let confirmed = inquire::Confirm::new("Confirm send-all?")
        .with_default(false)
        .prompt()?;

    if !confirmed {
        println!("Cancelled.");
        return Ok(());
    }

    let signer = w.signer()?;

    let result = Tokens::account(sender_id)
        .send_to(receiver_id)
        .near(send_amount)
        .with_signer(signer)
        .send_to(&net)
        .await
        .map_err(|e| {
            let msg = format!("{:#}", e);
            if msg.contains("InvalidTransaction") || msg.contains("NotEnoughBalance") {
                anyhow::anyhow!("transaction rejected: insufficient balance or invalid transaction")
            } else if msg.contains("access key") || msg.contains("does not exist") {
                anyhow::anyhow!("signing key is not a full-access key on this account")
            } else {
                anyhow::anyhow!("transaction error: {}", msg)
            }
        })?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_near_amount_whole() {
        let token = parse_near_amount("1", "near").unwrap();
        assert_eq!(token, NearToken::from_near(1));
    }

    #[test]
    fn test_parse_near_amount_fractional() {
        let token = parse_near_amount("0.5", "near").unwrap();
        assert_eq!(
            token,
            NearToken::from_yoctonear(500_000_000_000_000_000_000_000)
        );
    }

    #[test]
    fn test_parse_near_amount_small() {
        let token = parse_near_amount("0.001", "near").unwrap();
        assert_eq!(
            token,
            NearToken::from_yoctonear(1_000_000_000_000_000_000_000)
        );
    }

    #[test]
    fn test_parse_near_amount_yocto_unit() {
        let token = parse_near_amount("1000000000000000000000000", "yocto").unwrap();
        assert_eq!(token, NearToken::from_near(1));
    }

    #[test]
    fn test_parse_near_amount_invalid() {
        assert!(parse_near_amount("abc", "near").is_err());
    }

    #[test]
    fn test_parse_near_amount_negative() {
        assert!(parse_near_amount("-1", "near").is_err());
    }

    #[test]
    fn test_parse_near_amount_zero() {
        assert!(parse_near_amount("0", "near").is_err());
    }

}
