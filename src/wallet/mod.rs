/// Wallet management: mnemonic generation, import, key derivation.
use std::sync::Arc;

use anyhow::{bail, Result};
use near_api::signer::{generate_secret_key_from_seed_phrase, generate_seed_phrase, Signer};
use near_api::types::{AccountId, PublicKey};

use crate::network;
use crate::storage;

/// Information about a wallet (returned from generate/import).
pub struct WalletInfo {
    pub name: String,
    pub mnemonic: String,
    pub public_key: PublicKey,
    pub implicit_account_id: String,
}

impl std::fmt::Debug for WalletInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalletInfo")
            .field("name", &self.name)
            .field("mnemonic", &"[REDACTED]")
            .field("public_key", &self.public_key.to_string())
            .field("implicit_account_id", &self.implicit_account_id)
            .finish()
    }
}

/// Loaded wallet with network context.
pub struct Wallet {
    pub name: String,
    mnemonic: String,
    pub public_key: PublicKey,
    pub implicit_account_id: String,
    /// Named account if registered (e.g. "alice.testnet"), otherwise None.
    pub named_account_id: Option<String>,
    pub network: String,
}

impl std::fmt::Debug for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wallet")
            .field("name", &self.name)
            .field("mnemonic", &"[REDACTED]")
            .field("public_key", &self.public_key.to_string())
            .field("implicit_account_id", &self.implicit_account_id)
            .field("named_account_id", &self.named_account_id)
            .field("network", &self.network)
            .finish()
    }
}

impl Wallet {
    /// Create an Arc<Signer> from the wallet's full-access key (mnemonic).
    pub fn signer(&self) -> Result<Arc<Signer>> {
        let signer = Signer::from_seed_phrase(&self.mnemonic, None)?;
        Ok(signer)
    }

    /// Get a signer for a specific contract. Uses function-call key if stored,
    /// otherwise falls back to the full-access key.
    /// Note: FT/NFT transfers require full-access (deposit needed). This is for
    /// contract methods that don't require deposits.
    #[allow(dead_code)]
    pub fn signer_for_contract(&self, contract: &str) -> Result<Arc<Signer>> {
        if let Ok(Some(secret_key_str)) = storage::get_key_for_contract(&self.name, contract) {
            let secret_key: near_api::types::SecretKey = secret_key_str
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid stored secret key for {}", contract))?;
            return Signer::from_secret_key(secret_key)
                .map_err(|e| anyhow::anyhow!("failed to create signer: {}", e));
        }
        self.signer()
    }

    /// Primary account ID: named account if registered, otherwise implicit.
    pub fn account_id(&self) -> Result<AccountId> {
        let id_str = self
            .named_account_id
            .as_deref()
            .unwrap_or(&self.implicit_account_id);
        let id: AccountId = id_str.parse()?;
        Ok(id)
    }

    /// Get the NetworkConfig for this wallet's network.
    pub fn network_config(&self) -> Result<near_api::NetworkConfig> {
        network::get_network_config(&self.network)
    }

}

/// Derive the implicit account ID from a public key.
/// For ed25519 keys, this is the hex encoding of the 32-byte key data.
pub fn public_key_to_implicit_id(pk: &PublicKey) -> String {
    hex::encode(pk.key_data())
}

/// Derive the public key from a seed phrase (synchronous).
/// Public for use by resolve_recipient in utils.
pub fn public_key_from_mnemonic(seed_phrase: &str) -> Result<PublicKey> {
    let secret_key = generate_secret_key_from_seed_phrase(seed_phrase.to_string())?;
    Ok(secret_key.public_key())
}

/// Generate a new wallet: create seed phrase, derive keys, store.
/// Sets as default if this is the first wallet.
pub fn generate_wallet(name: &str, network: &str) -> Result<WalletInfo> {
    if storage::wallet_exists(name)? {
        bail!("wallet '{}' already exists", name);
    }

    let (seed_phrase, public_key) = generate_seed_phrase()?;
    let implicit_account_id = public_key_to_implicit_id(&public_key);

    storage::store_mnemonic(&seed_phrase, name)?;
    storage::store_network(name, network)?;

    // Set as default if this is the first wallet
    if storage::list_wallets()?.len() == 1 {
        storage::set_default_wallet(name)?;
    }

    Ok(WalletInfo {
        name: name.to_string(),
        mnemonic: seed_phrase,
        public_key,
        implicit_account_id,
    })
}

/// Import a wallet from a mnemonic phrase.
/// Trims whitespace and lowercases, validates by deriving a key.
/// Sets as default if this is the first wallet.
pub fn import_wallet(name: &str, mnemonic: &str, network: &str) -> Result<WalletInfo> {
    if storage::wallet_exists(name)? {
        bail!("wallet '{}' already exists", name);
    }

    let trimmed = mnemonic.trim().to_lowercase();

    // Validate and derive public key (synchronous -- no signer needed)
    let public_key = public_key_from_mnemonic(&trimmed)?;
    let implicit_account_id = public_key_to_implicit_id(&public_key);

    storage::store_mnemonic(&trimmed, name)?;
    storage::store_network(name, network)?;

    // Set as default if this is the first wallet
    if storage::list_wallets()?.len() == 1 {
        storage::set_default_wallet(name)?;
    }

    Ok(WalletInfo {
        name: name.to_string(),
        mnemonic: trimmed,
        public_key,
        implicit_account_id,
    })
}

/// Load a wallet by name (explicit or default), resolving network.
pub fn load_wallet(name: Option<&str>, cli_network: Option<&str>) -> Result<Wallet> {
    let resolved_name = storage::resolve_wallet_name(name)?;
    let mnemonic = storage::get_mnemonic(&resolved_name)?
        .ok_or_else(|| anyhow::anyhow!("wallet '{}' mnemonic file is empty", resolved_name))?;

    let network = storage::resolve_network(Some(&resolved_name), cli_network);
    let named_account_id = storage::get_account_id(&resolved_name)?;

    // Derive public key from mnemonic (synchronous)
    let public_key = public_key_from_mnemonic(&mnemonic)?;
    let implicit_account_id = public_key_to_implicit_id(&public_key);

    Ok(Wallet {
        name: resolved_name,
        mnemonic,
        public_key,
        implicit_account_id,
        named_account_id,
        network,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_temp_home() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        storage::set_base_dir_override(Some(tmp.path().to_path_buf()));
        tmp
    }

    #[test]
    fn test_generate_creates_12_words() {
        let _tmp = setup_temp_home();
        let info = generate_wallet("test", "testnet").unwrap();
        let words: Vec<&str> = info.mnemonic.split_whitespace().collect();
        assert_eq!(words.len(), 12);
    }

    #[test]
    fn test_generate_stores_and_sets_default() {
        let _tmp = setup_temp_home();
        let _info = generate_wallet("first", "testnet").unwrap();
        assert!(storage::wallet_exists("first").unwrap());
        assert_eq!(
            storage::get_default_wallet().unwrap(),
            Some("first".to_string())
        );
    }

    #[test]
    fn test_generate_duplicate_fails() {
        let _tmp = setup_temp_home();
        generate_wallet("dup", "testnet").unwrap();
        let result = generate_wallet("dup", "testnet");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_import_valid_known_phrase() {
        let _tmp = setup_temp_home();
        let phrase =
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let info = import_wallet("imported", phrase, "testnet").unwrap();
        assert_eq!(info.name, "imported");
        assert!(!info.implicit_account_id.is_empty());
        assert_eq!(info.implicit_account_id.len(), 64);
    }

    #[test]
    fn test_import_trims_and_lowercases() {
        let _tmp = setup_temp_home();
        let phrase =
            "  Abandon Abandon Abandon Abandon Abandon Abandon Abandon Abandon Abandon Abandon Abandon About  ";
        let info = import_wallet("trimmed", phrase, "testnet").unwrap();
        assert_eq!(
            info.mnemonic,
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"
        );
    }

    #[test]
    fn test_import_invalid_phrase() {
        let _tmp = setup_temp_home();
        let result = import_wallet("bad", "not a valid mnemonic phrase at all", "testnet");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_wallet_explicit() {
        let _tmp = setup_temp_home();
        let info = generate_wallet("loadme", "testnet").unwrap();
        let wallet = load_wallet(Some("loadme"), None).unwrap();
        assert_eq!(wallet.name, "loadme");
        assert_eq!(wallet.implicit_account_id, info.implicit_account_id);
    }

    #[test]
    fn test_load_wallet_default() {
        let _tmp = setup_temp_home();
        let info = generate_wallet("default-test", "testnet").unwrap();
        let wallet = load_wallet(None, None).unwrap();
        assert_eq!(wallet.implicit_account_id, info.implicit_account_id);
    }

    #[test]
    fn test_load_wallet_no_default() {
        let _tmp = setup_temp_home();
        let result = load_wallet(None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_two_generates_different() {
        let _tmp = setup_temp_home();
        let info1 = generate_wallet("first", "testnet").unwrap();
        let info2 = generate_wallet("second", "testnet").unwrap();
        assert_ne!(info1.mnemonic, info2.mnemonic);
    }

    #[test]
    fn test_implicit_id_is_64_hex() {
        let _tmp = setup_temp_home();
        let info = generate_wallet("hextest", "testnet").unwrap();
        assert_eq!(info.implicit_account_id.len(), 64);
        assert!(info
            .implicit_account_id
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_signer_creation() {
        let _tmp = setup_temp_home();
        let _info = generate_wallet("signer-test", "testnet").unwrap();
        let wallet = load_wallet(Some("signer-test"), None).unwrap();
        let signer = wallet.signer();
        assert!(signer.is_ok());
    }

    #[test]
    fn test_multi_wallet_isolation() {
        let _tmp = setup_temp_home();
        let info_a = generate_wallet("a", "testnet").unwrap();
        let _info_b = generate_wallet("b", "testnet").unwrap();
        let loaded = load_wallet(Some("a"), None).unwrap();
        assert_eq!(loaded.implicit_account_id, info_a.implicit_account_id);
    }

    #[test]
    fn test_wallet_debug_redacts_mnemonic() {
        let _tmp = setup_temp_home();
        let info = generate_wallet("dbg-test", "testnet").unwrap();
        let wallet = load_wallet(Some("dbg-test"), None).unwrap();
        let debug_str = format!("{:?}", wallet);
        assert!(debug_str.contains("[REDACTED]"));
        // Mnemonic words must not appear in debug output
        for word in info.mnemonic.split_whitespace() {
            assert!(!debug_str.contains(word), "debug output leaked mnemonic word: {}", word);
        }
    }
}
