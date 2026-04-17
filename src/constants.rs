/// 1 NEAR = 10^24 yoctoNEAR.
pub const YOCTO_PER_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

/// Well-known token aliases for the 1Click swap API.
/// Maps human-friendly names to nep141:contract identifiers (mainnet).
/// Returns (defuse_asset_id, contract_id, decimals).
///
/// Contract addresses verified against mainnet ft_metadata (F4):
/// - NEAR/WNEAR: wrap.near -- Wrapped NEAR, decimals 24
/// - USDT: usdt.tether-token.near -- Tether USD, decimals 6
/// - USDC: 17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1 -- USD Coin, decimals 6
/// - AURORA: aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near -- Aurora, decimals 18
/// - WBTC: 2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near -- Wrapped BTC, decimals 8
/// - ETH/WETH: aurora -- Ether, decimals 18
pub fn token_alias(name: &str) -> Option<(&'static str, &'static str, u8)> {
    match name.to_uppercase().as_str() {
        "NEAR" | "WNEAR" | "WRAP" => {
            Some(("nep141:wrap.near", "wrap.near", 24))
        }
        "USDT" => {
            Some(("nep141:usdt.tether-token.near", "usdt.tether-token.near", 6))
        }
        "USDC" => {
            Some(("nep141:17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1", "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1", 6))
        }
        "AURORA" | "AOA" => {
            Some(("nep141:aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near", "aaaaaa20d9e0e2461697782ef11675f668207961.factory.bridge.near", 18))
        }
        "WBTC" => {
            Some(("nep141:2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near", "2260fac5e5542a773aa44fbcfedf7c193bc2c599.factory.bridge.near", 8))
        }
        "WETH" | "ETH" => {
            Some(("nep141:aurora", "aurora", 18))
        }
        _ => None,
    }
}
