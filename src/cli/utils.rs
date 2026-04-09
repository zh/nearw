/// Shared CLI utility functions.
use anyhow::{Context, Result};
use near_api::NearToken;
use qrcode::QrCode;

use crate::constants::YOCTO_PER_NEAR;

/// Format yoctoNEAR as a human-readable NEAR amount.
/// Shows up to 6 decimal places. Dust amounts shown as yoctoNEAR.
pub fn format_near(yocto: u128) -> String {
    if yocto == 0 {
        return "0 NEAR".to_string();
    }

    let whole = yocto / YOCTO_PER_NEAR;
    let frac = yocto % YOCTO_PER_NEAR;

    if frac == 0 {
        return format!("{} NEAR", whole);
    }

    // Convert fractional part to string with 24 digits, truncate to 6
    let frac_str = format!("{:024}", frac);
    let trimmed = frac_str[..6].trim_end_matches('0');

    if trimmed.is_empty() {
        // Amount is < 0.000001 NEAR — show as yoctoNEAR
        if whole == 0 {
            return format!("{} yocto", yocto);
        }
        format!("{} NEAR", whole)
    } else {
        format!("{}.{} NEAR", whole, trimmed)
    }
}

/// Format a NearToken value as a human-readable string.
pub fn format_near_token(token: &NearToken) -> String {
    format_near(token.as_yoctonear())
}

/// Format a fungible token amount given raw amount, decimals, and symbol.
pub fn format_ft(amount: u128, decimals: u8, symbol: &str) -> String {
    if amount == 0 {
        return format!("0 {}", symbol);
    }

    if decimals == 0 {
        return format!("{} {}", amount, symbol);
    }

    let divisor = 10u128.pow(decimals as u32);
    let whole = amount / divisor;
    let frac = amount % divisor;

    if frac == 0 {
        format!("{} {}", whole, symbol)
    } else {
        let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{} {}", whole, trimmed, symbol)
    }
}

/// Truncate a 64-char hex implicit account ID for display.
/// Named accounts are returned unchanged.
pub fn short_account_id(id: &str) -> String {
    if id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("{}...{}", &id[..8], &id[id.len() - 8..])
    } else {
        id.to_string()
    }
}

/// Print QR code to terminal using half-block chars.
/// Ported from cashr's QR implementation.
pub fn print_qr(data: &str) -> Result<()> {
    let code = QrCode::new(data.as_bytes()).context("failed to generate QR code")?;
    let width = code.width();
    let modules = code.to_colors();
    let quiet = 1_i32;
    let total = width as i32 + quiet * 2;

    let is_dark = |r: i32, c: i32| -> bool {
        let mr = r - quiet;
        let mc = c - quiet;
        if mr < 0 || mc < 0 || mr >= width as i32 || mc >= width as i32 {
            return false;
        }
        modules[mr as usize * width + mc as usize] == qrcode::Color::Dark
    };

    for row in (0..total).step_by(2) {
        print!("   ");
        for col in 0..total {
            let top = is_dark(row, col);
            let bot = is_dark(row + 1, col);
            match (top, bot) {
                (false, false) => print!("\x1b[107m \x1b[0m"),
                (true, true) => print!("\x1b[40m \x1b[0m"),
                (false, true) => print!("\x1b[107;30m\u{2584}\x1b[0m"),
                (true, false) => print!("\x1b[40;97m\u{2584}\x1b[0m"),
            };
        }
        println!();
    }
    Ok(())
}

/// Resolve a recipient name to a full NEAR account ID.
/// 1. Already contains a dot (e.g. "alice.near") → used as-is
/// 2. 64-char hex → used as-is (implicit account)
/// 3. Matches a local wallet name → use that wallet's account (named or implicit)
/// 4. Bare name (e.g. "alice") → appends network suffix (.near or .testnet)
pub fn resolve_recipient(name: &str, network: &str) -> Result<near_api::AccountId> {
    let is_implicit = name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit());

    if is_implicit || name.contains('.') {
        return name
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid account ID: {}", name));
    }

    // Check if it's a local wallet name
    if let Ok(true) = crate::storage::wallet_exists(name) {
        // Use the wallet's named account if available, otherwise implicit
        if let Ok(Some(account)) = crate::storage::get_account_id(name) {
            return account
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid account ID: {}", account));
        }
        // Fall back to wallet's implicit account
        if let Ok(Some(mnemonic)) = crate::storage::get_mnemonic(name) {
            let pk = crate::wallet::public_key_from_mnemonic(&mnemonic)?;
            let implicit = crate::wallet::public_key_to_implicit_id(&pk);
            return implicit
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid account ID: {}", implicit));
        }
    }

    // Bare name → append network suffix
    let suffix = match network {
        "mainnet" => ".near",
        "testnet" => ".testnet",
        _ => "",
    };
    let full_name = format!("{}{}", name, suffix);

    full_name
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid account ID: {}", full_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_near_whole() {
        assert_eq!(format_near(YOCTO_PER_NEAR), "1 NEAR");
    }

    #[test]
    fn test_format_near_fractional() {
        let amount = YOCTO_PER_NEAR + YOCTO_PER_NEAR / 2; // 1.5 NEAR
        assert_eq!(format_near(amount), "1.5 NEAR");
    }

    #[test]
    fn test_format_near_small() {
        let amount = YOCTO_PER_NEAR / 1000; // 0.001 NEAR
        assert_eq!(format_near(amount), "0.001 NEAR");
    }

    #[test]
    fn test_format_near_zero() {
        assert_eq!(format_near(0), "0 NEAR");
    }

    #[test]
    fn test_format_near_large() {
        assert_eq!(format_near(1000 * YOCTO_PER_NEAR), "1000 NEAR");
    }

    #[test]
    fn test_format_ft() {
        assert_eq!(format_ft(1_000_000, 6, "USDT"), "1 USDT");
    }

    #[test]
    fn test_format_ft_fractional() {
        assert_eq!(format_ft(1_500_000, 6, "USDT"), "1.5 USDT");
    }

    #[test]
    fn test_short_account_id_named() {
        assert_eq!(short_account_id("alice.near"), "alice.near");
    }

    #[test]
    fn test_short_account_id_implicit() {
        let hex = "a".repeat(64);
        assert_eq!(short_account_id(&hex), "aaaaaaaa...aaaaaaaa");
    }

    #[test]
    fn test_format_near_dust() {
        // 1 yoctoNEAR — shown as yocto, not "0 NEAR"
        assert_eq!(format_near(1), "1 yocto");
        assert_eq!(format_near(9), "9 yocto");
    }

    #[test]
    fn test_format_near_very_small() {
        // 0.0000001 NEAR (1e17 yoctoNEAR) — below 6-digit threshold, shown as yocto
        let amount = YOCTO_PER_NEAR / 10_000_000;
        let result = format_near(amount);
        assert_eq!(result, format!("{} yocto", amount));
    }

    #[test]
    fn test_print_qr_no_panic() {
        // Just verify it doesn't panic
        assert!(print_qr("test data").is_ok());
    }
}
