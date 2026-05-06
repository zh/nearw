use anyhow::{bail, Result};
use near_api::types::tokens::FTBalance;
use near_api::{Contract, NearGas, NearToken, Tokens};
use owo_colors::OwoColorize;
use serde_json::json;

use crate::cli::token::parse_ft_amount;
use crate::cli::utils;
use crate::constants::token_alias;
use crate::network;
use crate::oneclick::{OneClickClient, QuoteRequest};
use crate::wallet;

/// Maximum time to poll for swap completion (5 minutes).
const POLL_TIMEOUT_SECS: u64 = 300;
/// Initial poll interval (2 seconds).
const POLL_INITIAL_INTERVAL_MS: u64 = 2000;
/// Maximum poll interval after backoff (10 seconds).
const POLL_MAX_INTERVAL_MS: u64 = 10_000;
/// Backoff multiplier per iteration.
const POLL_BACKOFF_FACTOR: f64 = 1.5;

/// F2: Deposit addresses from 1Click API are 64-char hex implicit NEAR accounts.
/// Validate format before sending funds.
const DEPOSIT_ADDR_HEX_LEN: usize = 64;

/// NEAR Intents settlement contract on mainnet.
const INTENTS_CONTRACT_ID: &str = "intents.near";

/// Resolve a token name/alias to (defuse_asset_id, contract_id, decimals).
/// Tries alias map first, then treats input as a raw contract ID.
async fn resolve_token(
    input: &str,
    net: &near_api::NetworkConfig,
) -> Result<(String, String, u8)> {
    // Check alias map first
    if let Some((defuse_id, contract_id, decimals)) = token_alias(input) {
        return Ok((defuse_id.to_string(), contract_id.to_string(), decimals));
    }

    // Treat as raw contract ID -- fetch metadata from chain
    let contract_id: near_api::AccountId = input
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown token alias and invalid contract ID: {}", input))?;

    let metadata = Tokens::ft_metadata(contract_id.clone())
        .fetch_from(net)
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch metadata for {}: {}", input, e))?;

    let defuse_id = format!("nep141:{}", contract_id);
    Ok((defuse_id, contract_id.to_string(), metadata.data.decimals))
}

/// F2: Validate that a deposit address is a valid 64-char hex implicit NEAR account.
/// 1Click API returns these as deposit targets.
fn is_valid_deposit_address(addr: &str) -> bool {
    addr.len() == DEPOSIT_ADDR_HEX_LEN && addr.chars().all(|c| c.is_ascii_hexdigit())
}

/// List supported tokens from the 1Click API.
pub async fn tokens(json_output: bool) -> Result<()> {
    let client = make_client()?;
    let token_list = client.tokens().await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&token_list)?);
        return Ok(());
    }

    println!("{}", "Supported Swap Tokens".bold());
    println!();
    for t in &token_list {
        let symbol = t.symbol.as_deref().unwrap_or("?");
        let chain = t.blockchain.as_deref().unwrap_or("");
        println!("  {:>8}  {}  ({})", symbol.bold(), chain, t.asset_id.dimmed());
    }

    Ok(())
}

/// Show a swap quote without executing.
pub async fn quote(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    from: &str,
    to: &str,
    amount: &str,
    json_output: bool,
) -> Result<()> {
    mainnet_guard(cli_network)?;

    let w = wallet::load_wallet(wallet_name, Some("mainnet"))?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;

    let (from_defuse, _from_contract, from_decimals) = resolve_token(from, &net).await?;
    let (to_defuse, _to_contract, to_decimals) = resolve_token(to, &net).await?;

    let raw_amount = parse_ft_amount(amount, from_decimals)?;
    let sender_str = sender_id.to_string();

    let client = make_client()?;
    let quote_resp = client
        .quote(&build_quote_request(
            &from_defuse, &to_defuse, raw_amount, &sender_str,
        ))
        .await?;

    let q = &quote_resp.quote;

    if json_output {
        println!(
            "{}",
            serde_json::json!({
                "from_token": from_defuse,
                "to_token": to_defuse,
                "amount_in": q.amount_in,
                "amount_out": q.amount_out,
                "amount_out_formatted": q.amount_out_formatted,
                "deposit_address": q.deposit_address,
                "deadline": q.deadline,
            })
        );
    } else {
        let from_sym = from.to_uppercase();
        let to_sym = to.to_uppercase();
        println!("{}", "Swap Quote".bold());
        println!();
        println!("  From:     {} {}", amount, from_sym);
        let expected = q.amount_out_formatted.clone()
            .unwrap_or_else(|| format_amount_with_decimals(&q.amount_out, to_decimals));
        println!("  Expected: {} {}", expected, to_sym);
        println!("  Deposit:  {}", q.deposit_address.dimmed());
        if let Some(secs) = q.time_estimate {
            println!("  Est swap: ~{}s", secs);
        }
        if let Some(ref deadline) = q.deadline {
            println!("  Valid:    {}", format_deadline(deadline));
        }
    }

    Ok(())
}

/// Poll status of an existing swap by deposit address.
pub async fn status(
    deposit_address: &str,
    json_output: bool,
) -> Result<()> {
    let client = make_client()?;
    let swap_status = client.status(deposit_address).await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&swap_status)?);
    } else {
        println!("{}", "Swap Status".bold());
        println!();
        println!("  Deposit:  {}", deposit_address);
        println!("  Status:   {}", colorize_status(&swap_status.status));
        if let Some(hash) = swap_status.tx_hash() {
            println!("  Tx:       {}", hash);
        }
        if let Some(reason) = swap_status.refund_reason() {
            println!("  Refund:   {}", reason.red());
        }
        if let Some(out) = swap_status.amount_out_formatted() {
            println!("  Received: {}", out);
        }
    }

    Ok(())
}

/// Execute a full swap: quote -> deposit -> poll.
pub async fn execute(
    wallet_name: Option<&str>,
    cli_network: Option<&str>,
    from: &str,
    to: &str,
    amount: &str,
    confirmed: bool,
    json_output: bool,
) -> Result<()> {
    mainnet_guard(cli_network)?;

    let w = wallet::load_wallet(wallet_name, Some("mainnet"))?;
    let sender_id = w.account_id()?;
    let net = w.network_config()?;
    let signer = w.signer()?;

    // Step 1: Resolve tokens
    let (from_defuse, from_contract, from_decimals) = resolve_token(from, &net).await?;
    let (to_defuse, to_contract, to_decimals) = resolve_token(to, &net).await?;

    let is_native_near = from.to_uppercase() == "NEAR";
    let raw_amount = parse_ft_amount(amount, from_decimals)?;

    // Step 2: Get quote
    let sender_str = sender_id.to_string();
    let client = make_client()?;
    let quote_resp = client
        .quote(&build_quote_request(
            &from_defuse, &to_defuse, raw_amount, &sender_str,
        ))
        .await?;

    let q = &quote_resp.quote;

    // F2: Validate deposit address is a valid 64-char hex implicit account
    let addr_str = &q.deposit_address;
    if !is_valid_deposit_address(addr_str) {
        bail!(
            "deposit address '{}' is not a valid implicit account. \
             This may indicate a compromised API response. Aborting.",
            addr_str
        );
    }

    // Step 3: Display quote and confirm
    let from_sym = from.to_uppercase();
    let to_sym = to.to_uppercase();

    if !json_output {
        println!("{}", "Swap".bold());
        println!();
        println!("  From:     {} {} ({})", amount, from_sym,
            utils::short_account_id(sender_id.as_ref()));
        let expected = q.amount_out_formatted.clone()
            .unwrap_or_else(|| format_amount_with_decimals(&q.amount_out, to_decimals));
        println!("  To:       ~{} {}", expected, to_sym);
        println!("  Deposit:  {}", q.deposit_address.dimmed());
        if let Some(secs) = q.time_estimate {
            println!("  Est swap: ~{}s", secs);
        }
        if let Some(ref deadline) = q.deadline {
            println!("  Valid:    {}", format_deadline(deadline));
        }
        if is_native_near {
            println!("  Note:     {} (auto-wrap to wNEAR)", "native NEAR".yellow());
        }
        println!();
    }

    if !confirmed {
        let answer = inquire::Confirm::new("Execute swap?")
            .with_default(false)
            .prompt()?;
        if !answer {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // Step 4: If native NEAR, wrap to wNEAR first
    if is_native_near {
        if !json_output {
            println!("Wrapping {} NEAR to wNEAR...", amount);
        }

        let wrap_contract: near_api::AccountId = "wrap.near".parse()?;
        let deposit_near = NearToken::from_yoctonear(raw_amount);

        Contract(wrap_contract)
            .call_function("near_deposit", json!({}))
            .transaction()
            .deposit(deposit_near)
            .gas(NearGas::from_tgas(10))
            .with_signer(sender_id.clone(), signer.clone())
            .send_to(&net)
            .await
            .map_err(|e| anyhow::anyhow!("wNEAR wrap failed: {:#}", e))?
            .into_result()
            .map_err(|e| anyhow::anyhow!("wNEAR wrap tx failed: {}", e))?;
    }

    // Step 5: Two-tx deposit into NEAR Intents.
    //
    // 1Click depositType=INTENTS expects funds inside the `intents.near` MT
    // ledger, credited to a virtual sub-account (the 64-char hex
    // `depositAddress`). Sending plain ft_transfer (or ft_transfer_call) to
    // the implicit address parks funds in an unrecoverable on-chain account.
    // The correct flow is:
    //
    //   5a) ft_transfer_call(wrap.near -> intents.near, msg=sender_id)
    //       Deposits FT into intents.near under the sender's account.
    //   5b) mt_transfer(intents.near, token_id=from_defuse,
    //                   receiver_id=deposit_address)
    //       Routes the credit to the 1Click deposit slot inside intents.
    //
    // Reference: nearuaguild/near-intents-1click-example/src/near.ts.
    //
    // Recovery: if 5a succeeds but 5b fails, funds sit in intents.near under
    // the sender's account and can be recovered via mt_transfer/withdraw.
    if !json_output {
        println!("Depositing {} {} into intents.near...", amount, from_sym);
    }

    let ft_contract: near_api::AccountId = from_contract.parse()?;
    let intents_contract: near_api::AccountId = INTENTS_CONTRACT_ID.parse()?;
    // Validate the API-provided deposit address (also serves as receiver_id
    // inside intents.near for the mt_transfer call).
    let _: near_api::AccountId = q.deposit_address.parse()
        .map_err(|_| anyhow::anyhow!(
            "invalid deposit address from API: {}", q.deposit_address
        ))?;

    let ft_balance = FTBalance::with_decimals(from_decimals).with_amount(raw_amount);

    // Step 5a: ft_transfer_call to intents.near
    let intents_deposit_result = Tokens::account(sender_id.clone())
        .send_to(intents_contract.clone())
        .ft_call(ft_contract, ft_balance, sender_id.to_string())
        .with_signer(signer.clone())
        .send_to(&net)
        .await;

    let intents_deposit_result = match intents_deposit_result {
        Err(e) if is_native_near => {
            eprintln!("ERROR: Intents deposit failed after wNEAR wrap succeeded.");
            eprintln!("Your {} wNEAR is safe in your account.", amount);
            eprintln!(
                "To unwrap: nearw call wrap.near near_withdraw \
                 '{{\"amount\": \"{}\"}}' --deposit 1yocto",
                raw_amount
            );
            eprintln!("To retry with existing wNEAR: nearw swap execute WNEAR {} {}", to, amount);
            return Err(anyhow::anyhow!("intents deposit (ft_transfer_call) error: {:#}", e));
        }
        Err(e) => return Err(anyhow::anyhow!("intents deposit (ft_transfer_call) error: {:#}", e)),
        Ok(r) => r,
    };

    let _intents_deposit = intents_deposit_result
        .into_result()
        .map_err(|e| {
            eprintln!(
                "ERROR: Intents deposit (ft_transfer_call) tx failed: {}",
                e
            );
            if is_native_near {
                eprintln!("Your {} wNEAR is safe in your account.", amount);
                eprintln!(
                    "To unwrap: nearw call wrap.near near_withdraw \
                     '{{\"amount\": \"{}\"}}' --deposit 1yocto",
                    raw_amount
                );
                eprintln!("To retry with existing wNEAR: nearw swap execute WNEAR {} {}", to, amount);
            }
            anyhow::anyhow!("intents deposit tx failed: {}", e)
        })?;

    // Step 5b: mt_transfer on intents.near to the 1Click deposit_address.
    // If this fails, funds remain inside intents.near under the sender's
    // account and can be withdrawn or retried.
    let mt_result = Contract(intents_contract.clone())
        .call_function("mt_transfer", json!({
            "token_id": from_defuse,
            "receiver_id": q.deposit_address,
            "amount": raw_amount.to_string(),
        }))
        .transaction()
        .deposit(NearToken::from_yoctonear(1))
        .gas(NearGas::from_tgas(30))
        .with_signer(sender_id.clone(), signer.clone())
        .send_to(&net)
        .await
        .map_err(|e| {
            eprintln!(
                "ERROR: mt_transfer to deposit address failed: {:#}",
                e
            );
            eprintln!(
                "Your {} {} is held inside intents.near under {}.",
                amount, from_sym, sender_id
            );
            eprintln!(
                "Retry: nearw call intents.near mt_transfer \
                 '{{\"token_id\":\"{}\",\"receiver_id\":\"{}\",\"amount\":\"{}\"}}' \
                 --deposit 1yocto --gas 30",
                from_defuse, q.deposit_address, raw_amount
            );
            anyhow::anyhow!("mt_transfer error: {:#}", e)
        })?
        .into_result()
        .map_err(|e| {
            eprintln!("ERROR: mt_transfer tx failed: {}", e);
            eprintln!(
                "Your {} {} is held inside intents.near under {}.",
                amount, from_sym, sender_id
            );
            anyhow::anyhow!("mt_transfer tx failed: {}", e)
        })?;

    let deposit_tx_hash = mt_result.outcome().transaction_hash.to_string();

    if !json_output {
        println!(
            "  Deposit tx: {}",
            network::explorer_tx_url("mainnet", &deposit_tx_hash).cyan()
        );
    }

    // Step 6: Notify API of deposit
    client.submit_deposit(&deposit_tx_hash, &q.deposit_address).await?;

    // Step 7: Snapshot recipient's destination FT balance, then poll both
    // 1Click status and chain state. Chain settlement is the source of truth:
    // the 1Click status indexer is sometimes lagged or stale even after the
    // solver has emitted the on-chain ft_transfer.
    let to_contract_id: near_api::AccountId = to_contract.parse()?;
    let baseline_out = fetch_ft_balance(&net, &sender_id, &to_contract_id).await;
    let min_out: u128 = q.min_amount_out.as_deref()
        .and_then(|s| s.parse().ok())
        .or_else(|| q.amount_out.parse().ok())
        .unwrap_or(0);

    if !json_output {
        println!("Polling swap status...");
    }

    let deposit_addr = q.deposit_address.clone();
    let outcome = poll_until_settled(
        &client,
        &net,
        &deposit_addr,
        &sender_id,
        &to_contract_id,
        baseline_out,
        min_out,
        json_output,
    )
    .await?;

    let final_status = &outcome.status;

    // Step 8: Output result
    if json_output {
        let chain_amount_out = outcome.chain_delta.map(|d| d.to_string());
        let chain_amount_out_formatted = outcome.chain_delta
            .map(|d| format_amount_with_decimals(&d.to_string(), to_decimals));
        println!(
            "{}",
            serde_json::json!({
                "status": final_status.status,
                "settled_via": outcome.source_label(),
                "deposit_tx_hash": deposit_tx_hash,
                "deposit_address": deposit_addr,
                "correlation_id": quote_resp.correlation_id,
                "from_token": from_defuse,
                "to_token": to_defuse,
                "amount_in": q.amount_in,
                "amount_out": final_status.amount_out().map(|s| s.to_string())
                    .or(chain_amount_out),
                "amount_out_formatted": final_status.amount_out_formatted()
                    .map(|s| s.to_string())
                    .or(chain_amount_out_formatted),
                "tx_hash": final_status.tx_hash(),
                "network": "mainnet",
            })
        );
    } else if outcome.is_settled() {
        println!("{}", "Swap completed!".green().bold());
        if let Some(delta) = outcome.chain_delta {
            println!(
                "  Received: {} {} (on-chain)",
                format_amount_with_decimals(&delta.to_string(), to_decimals),
                to_sym
            );
        } else if let Some(out) = final_status.amount_out_formatted() {
            println!("  Received: {} {}", out, to_sym);
        }
        if let Some(hash) = final_status.tx_hash() {
            println!("  Tx: {}", network::explorer_tx_url("mainnet", hash).cyan());
        }
        if outcome.source == PollSource::Chain {
            println!("  {}", "(1Click status indexer lagged; chain confirms)".dimmed());
        }
    } else {
        println!("{}", "Swap failed.".red().bold());
        if let Some(reason) = final_status.refund_reason() {
            println!("  Reason: {}", reason);
        }
        println!("  Check: nearw swap status {}", deposit_addr);
    }

    Ok(())
}

/// Create a OneClickClient, using config override if present.
/// F1: Enforce HTTPS on the oneclick_api config override URL.
fn make_client() -> Result<OneClickClient> {
    let cfg = crate::config::load_config().unwrap_or_default();

    let client = match cfg.oneclick_api {
        Some(url) => {
            if !url.starts_with("https://") {
                bail!("1Click API URL must use HTTPS on mainnet");
            }
            OneClickClient::with_base_url(url)?
        }
        None => OneClickClient::new()?,
    };

    // JWT: env var takes precedence over config.toml.
    let jwt = std::env::var("ONECLICK_JWT").ok()
        .or(cfg.oneclick_jwt)
        .filter(|s| !s.is_empty());

    Ok(client.with_jwt(jwt))
}

/// F6: Positive mainnet guard -- assert the effective network is mainnet.
fn mainnet_guard(cli_network: Option<&str>) -> Result<()> {
    let effective = cli_network.unwrap_or("mainnet");
    if effective != "mainnet" {
        bail!("NEAR Intents swaps are only available on mainnet");
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum PollSource {
    OneClick,
    Chain,
}

struct PollOutcome {
    status: crate::oneclick::SwapStatus,
    /// On-chain delta in destination FT (raw units), if observed.
    chain_delta: Option<u128>,
    /// Which signal triggered termination.
    source: PollSource,
}

impl PollOutcome {
    fn is_settled(&self) -> bool {
        match self.source {
            PollSource::Chain => self.chain_delta.is_some(),
            PollSource::OneClick => self.status.is_completed(),
        }
    }

    fn source_label(&self) -> &'static str {
        match self.source {
            PollSource::Chain => "chain",
            PollSource::OneClick => "1click",
        }
    }
}

/// Read FT balance for `account` on `ft_contract`. Returns `None` if the
/// account isn't registered on the FT contract (no storage) or any RPC
/// error — both treated as "no baseline available".
async fn fetch_ft_balance(
    net: &near_api::NetworkConfig,
    account: &near_api::AccountId,
    ft_contract: &near_api::AccountId,
) -> Option<u128> {
    use near_api::Tokens;
    Tokens::account(account.clone())
        .ft_balance(ft_contract.clone())
        .fetch_from(net)
        .await
        .ok()
        .map(|b| b.amount())
}

/// Poll until 1Click reports terminal OR chain-side FT balance settles.
/// Chain wins to handle 1Click indexer lag/staleness.
#[allow(clippy::too_many_arguments)]
async fn poll_until_settled(
    client: &OneClickClient,
    net: &near_api::NetworkConfig,
    deposit_address: &str,
    recipient: &near_api::AccountId,
    to_contract: &near_api::AccountId,
    baseline_out: Option<u128>,
    min_out: u128,
    quiet: bool,
) -> Result<PollOutcome> {
    let start = std::time::Instant::now();
    let mut interval_ms = POLL_INITIAL_INTERVAL_MS;
    let mut last_status: Option<crate::oneclick::SwapStatus> = None;

    loop {
        let elapsed = start.elapsed().as_secs();
        if elapsed > POLL_TIMEOUT_SECS {
            bail!(
                "swap timed out after {}s. Check later with: nearw swap status {}",
                POLL_TIMEOUT_SECS,
                deposit_address
            );
        }

        tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;

        // Chain check (fast path — settles even when 1Click is lagged).
        if let (Some(base), Some(now)) = (
            baseline_out,
            fetch_ft_balance(net, recipient, to_contract).await,
        ) {
            if let Some(delta) = now.checked_sub(base) {
                if delta > 0 && delta >= min_out {
                    if !quiet {
                        eprintln!("\r  Status: {} ({}s elapsed) — chain settled",
                            "completed".green(), elapsed);
                    }
                    let status = last_status.unwrap_or(crate::oneclick::SwapStatus {
                        status: "SUCCESS".to_string(),
                        swap_details: None,
                        correlation_id: None,
                    });
                    return Ok(PollOutcome {
                        status,
                        chain_delta: Some(delta),
                        source: PollSource::Chain,
                    });
                }
            }
        }

        // 1Click status check.
        match client.status(deposit_address).await {
            Ok(status) => {
                if !quiet {
                    eprint!("\r  Status: {} ({}s elapsed)",
                        colorize_status(&status.status), elapsed);
                }
                if status.is_terminal() {
                    if !quiet {
                        eprintln!();
                    }
                    return Ok(PollOutcome {
                        status,
                        chain_delta: None,
                        source: PollSource::OneClick,
                    });
                }
                last_status = Some(status);
            }
            Err(e) => {
                // Don't fail the whole swap on a transient status read error;
                // keep polling and let chain check carry us through.
                if !quiet {
                    eprint!("\r  Status: 1click read error ({}s elapsed): {}",
                        elapsed, e);
                }
            }
        }

        // Exponential backoff
        interval_ms = ((interval_ms as f64) * POLL_BACKOFF_FACTOR) as u64;
        if interval_ms > POLL_MAX_INTERVAL_MS {
            interval_ms = POLL_MAX_INTERVAL_MS;
        }
    }
}

/// Colorize swap status string.
fn colorize_status(status: &str) -> String {
    match status {
        "SUCCESS" | "COMPLETED" => status.to_lowercase().green().to_string(),
        "FAILED" | "REFUNDED" => status.to_lowercase().red().to_string(),
        _ => status.to_lowercase().yellow().to_string(),
    }
}

/// Build a QuoteRequest with standard defaults for NEAR-to-NEAR swaps.
///
/// `depositType: INTENTS` — we deposit through the intents.near MT ledger
/// via the two-tx ft_transfer_call + mt_transfer flow.
/// `recipientType: DESTINATION_CHAIN` — solver emits an on-chain ft_transfer
/// to the user's regular FT balance (skips the manual mt_withdraw step).
/// `refundType: ORIGIN_CHAIN` — failed swaps refund to the on-chain origin.
fn build_quote_request(
    from_defuse: &str,
    to_defuse: &str,
    raw_amount: u128,
    sender: &str,
) -> QuoteRequest {
    // Deadline: 30 minutes from now
    let deadline = chrono::Utc::now() + chrono::Duration::minutes(30);
    QuoteRequest {
        origin_asset: from_defuse.to_string(),
        destination_asset: to_defuse.to_string(),
        amount: raw_amount.to_string(),
        swap_type: "EXACT_INPUT".to_string(),
        slippage_tolerance: 100, // 1%
        deposit_type: "INTENTS".to_string(),
        refund_to: sender.to_string(),
        refund_type: "ORIGIN_CHAIN".to_string(),
        recipient: sender.to_string(),
        recipient_type: "DESTINATION_CHAIN".to_string(),
        dry: false,
        deadline: deadline.to_rfc3339(),
    }
}

/// Format a raw amount string with decimals for display (best-effort).
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

/// Get the local timezone abbreviation (e.g. "JST", "PST") from the C library.
/// Falls back to the numeric UTC offset if unavailable.
fn local_tz_abbreviation() -> String {
    use std::ffi::CStr;
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&now, &mut tm);
        if tm.tm_zone.is_null() {
            // Fallback to chrono offset
            return chrono::Local::now().format("%:z").to_string();
        }
        CStr::from_ptr(tm.tm_zone)
            .to_str()
            .unwrap_or("+??")
            .to_string()
    }
}

/// Format an ISO 8601 UTC deadline as: UTC (HH:MM TZ, relative).
/// Example: "2026-04-10T05:50:34Z (14:50 JST, in 28min)"
/// Falls back to raw string if parsing fails.
fn format_deadline(iso: &str) -> String {
    use chrono::{DateTime, Local, Utc};

    let parsed = match DateTime::parse_from_rfc3339(iso) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return iso.to_string(),
    };

    let local = parsed.with_timezone(&Local);
    let tz_abbr = local_tz_abbreviation();
    let local_str = format!("{} {}", local.format("%H:%M"), tz_abbr);

    let now = Utc::now();
    let relative = if parsed > now {
        let diff = parsed - now;
        let total_mins = diff.num_minutes();
        if total_mins >= 60 {
            format!("in {}h {}min", total_mins / 60, total_mins % 60)
        } else {
            format!("in {}min", total_mins)
        }
    } else {
        "expired".to_string()
    };

    let utc_short = parsed.format("%Y-%m-%dT%H:%MZ").to_string();
    format!("{} ({}, {})", utc_short, local_str, relative)
}
