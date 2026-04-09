use anyhow::Result;
use owo_colors::OwoColorize;

use crate::cli::utils;
use crate::wallet;

/// Show receive address and QR code.
pub async fn run(wallet_name: Option<&str>, cli_network: Option<&str>, no_qr: bool) -> Result<()> {
    let w = wallet::load_wallet(wallet_name, cli_network)?;

    println!("{}", "Receive Address".bold());
    println!();

    // Show named account if registered, plus implicit address
    if let Some(ref named) = w.named_account_id {
        println!("  Account:  {}", named.cyan());
        println!("  Implicit: {}", w.implicit_account_id.dimmed());
        println!("  Network:  {}", w.network);
        println!();
        println!("Send NEAR to either address above.");

        if !no_qr {
            println!();
            utils::print_qr(named)?;
        }
    } else {
        println!("  Account: {}", w.implicit_account_id.cyan());
        println!("  Network: {}", w.network);
        println!();
        println!("Send NEAR to the address above to activate this account.");

        if !no_qr {
            println!();
            utils::print_qr(&w.implicit_account_id)?;
        }
    }

    Ok(())
}
