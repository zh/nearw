/// Filesystem wallet storage at ~/.nearw/.
///
/// Storage layout:
///   ~/.nearw/
///   ├── config.toml               # Optional TOML config
///   ├── default                    # Contains name of default wallet
///   └── wallets/                   # 0700 permissions
///       ├── savings                # Mnemonic for "savings" wallet (0600)
///       ├── savings.net            # Network for "savings" wallet
///       ├── savings.account        # Named account for "savings" wallet
///       ├── savings.keys           # JSON: contract → secret key (0600)
///       └── trading                # Mnemonic for "trading" wallet (0600)
use anyhow::{Context, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

/// Reserved names that cannot be used as wallet names.
const RESERVED_NAMES: &[&str] = &["default", "config"];

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("invalid wallet name '{name}': must be alphanumeric, hyphens, or underscores (max 64 chars)")]
    InvalidWalletName { name: String },
    #[error("wallet '{name}' already exists")]
    WalletExists { name: String },
    #[error("wallet '{name}' not found")]
    WalletNotFound { name: String },
    #[error("no default wallet set -- use --name or create a wallet first")]
    NoDefaultWallet,
}

thread_local! {
    /// Thread-local override for base directory (used in tests).
    static BASE_DIR_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Set a base directory override for the current thread (for testing).
#[cfg(test)]
pub fn set_base_dir_override(path: Option<PathBuf>) {
    BASE_DIR_OVERRIDE.with(|cell| {
        *cell.borrow_mut() = path;
    });
}

/// Get base directory: thread-local override > NEARW_HOME env var > ~/.nearw.
pub(crate) fn base_dir() -> Result<PathBuf> {
    let override_path = BASE_DIR_OVERRIDE.with(|cell| cell.borrow().clone());
    if let Some(path) = override_path {
        return Ok(path);
    }
    if let Ok(override_dir) = std::env::var("NEARW_HOME") {
        return Ok(PathBuf::from(override_dir));
    }
    dirs::home_dir()
        .map(|h| h.join(".nearw"))
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))
}

/// Ensure a directory has 0700 permissions. Fixes if wrong.
#[cfg(unix)]
fn ensure_dir_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path)?;
    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o700 {
        eprintln!(
            "WARNING: fixing permissions on {} (was {:o}, setting to 0700)",
            path.display(),
            mode
        );
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(path, perms)
            .context("failed to fix directory permissions")?;
    }
    Ok(())
}

/// Write content to a file and set 0600 permissions (Unix).
fn write_secret_file(path: &std::path::Path, content: &str) -> Result<()> {
    std::fs::write(path, content).context("failed to write secret file")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(path, perms).context("failed to set file permissions")?;
    }
    Ok(())
}

/// Get or create the nearw directory.
fn nearw_dir() -> Result<PathBuf> {
    let dir = base_dir()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).context("failed to create nearw directory")?;
    }
    #[cfg(unix)]
    ensure_dir_permissions(&dir)?;
    Ok(dir)
}

/// Get or create the wallets subdirectory with 0700 permissions.
pub(crate) fn wallets_dir() -> Result<PathBuf> {
    let dir = nearw_dir()?.join("wallets");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).context("failed to create wallets directory")?;
    }
    #[cfg(unix)]
    ensure_dir_permissions(&dir)?;
    Ok(dir)
}

/// Validate wallet name: alphanumeric + hyphens + underscores, max 64 chars.
/// Rejects reserved names ("default", "config") and names starting with ".".
pub fn validate_wallet_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(StorageError::InvalidWalletName {
            name: name.to_string(),
        }
        .into());
    }
    if name.starts_with('.') {
        return Err(StorageError::InvalidWalletName {
            name: name.to_string(),
        }
        .into());
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(StorageError::InvalidWalletName {
            name: name.to_string(),
        }
        .into());
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(StorageError::InvalidWalletName {
            name: name.to_string(),
        }
        .into());
    }
    Ok(())
}

/// Store a mnemonic under a wallet name.
/// File permissions are set to 0o600 on Unix.
/// Returns an error if the wallet already exists.
pub fn store_mnemonic(mnemonic: &str, name: &str) -> Result<()> {
    validate_wallet_name(name)?;
    let path = wallets_dir()?.join(name);
    if path.exists() {
        return Err(StorageError::WalletExists {
            name: name.to_string(),
        }
        .into());
    }

    let content = format!("{}\n", mnemonic.trim());
    write_secret_file(&path, &content)?;

    Ok(())
}

/// Get mnemonic by wallet name. Returns Ok(None) if not found.
pub fn get_mnemonic(name: &str) -> Result<Option<String>> {
    validate_wallet_name(name)?;
    let path = wallets_dir()?.join(name);
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read mnemonic file")),
    }
}

/// Store the network for a wallet (e.g. "mainnet" or "testnet").
pub fn store_network(name: &str, network: &str) -> Result<()> {
    let path = wallets_dir()?.join(format!("{}.net", name));
    std::fs::write(&path, network).context("failed to write network file")?;
    Ok(())
}

/// Get the stored network for a wallet. Returns None if not set.
pub fn get_network(name: &str) -> Result<Option<String>> {
    let path = wallets_dir()?.join(format!("{}.net", name));
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read network file")),
    }
}

/// Store a named account ID for a wallet (e.g. "alice.testnet").
/// This overrides the implicit account as the wallet's primary account.
pub fn store_account_id(name: &str, account_id: &str) -> Result<()> {
    let path = wallets_dir()?.join(format!("{}.account", name));
    std::fs::write(&path, account_id).context("failed to write account ID file")?;
    Ok(())
}

/// Clear the named account ID for a wallet.
pub fn clear_account_id(name: &str) -> Result<()> {
    let path = wallets_dir()?.join(format!("{}.account", name));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("failed to clear account ID file")),
    }
}

/// Get the stored named account ID for a wallet. Returns None if not set.
pub fn get_account_id(name: &str) -> Result<Option<String>> {
    let path = wallets_dir()?.join(format!("{}.account", name));
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read account ID file")),
    }
}

/// Load the contract → secret key map for a wallet. Returns empty map if not found.
pub fn load_keys(name: &str) -> Result<HashMap<String, String>> {
    let path = wallets_dir()?.join(format!("{}.keys", name));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let keys: HashMap<String, String> =
                serde_json::from_str(&content).context("failed to parse keys file")?;
            Ok(keys)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read keys file")),
    }
}

/// Save a secret key for a contract in the wallet's keys file.
pub fn store_key(wallet_name: &str, contract: &str, secret_key: &str) -> Result<()> {
    let mut keys = load_keys(wallet_name)?;
    keys.insert(contract.to_string(), secret_key.to_string());
    let path = wallets_dir()?.join(format!("{}.keys", wallet_name));
    let content = serde_json::to_string_pretty(&keys)?;
    write_secret_file(&path, &content)?;

    Ok(())
}

/// Remove a key for a contract from the wallet's keys file.
#[allow(dead_code)]
pub fn remove_key(wallet_name: &str, contract: &str) -> Result<()> {
    let mut keys = load_keys(wallet_name)?;
    keys.remove(contract);
    if keys.is_empty() {
        let path = wallets_dir()?.join(format!("{}.keys", wallet_name));
        let _ = std::fs::remove_file(path);
    } else {
        let path = wallets_dir()?.join(format!("{}.keys", wallet_name));
        let content = serde_json::to_string_pretty(&keys)?;
        write_secret_file(&path, &content)?;
    }
    Ok(())
}

/// Get the secret key for a specific contract. Returns None if not stored.
#[allow(dead_code)]
pub fn get_key_for_contract(wallet_name: &str, contract: &str) -> Result<Option<String>> {
    let keys = load_keys(wallet_name)?;
    Ok(keys.get(contract).cloned())
}

/// Resolve the network for a wallet.
/// CLI flag overrides stored value; default is "mainnet".
pub fn resolve_network(wallet_name: Option<&str>, cli_network: Option<&str>) -> String {
    if let Some(net) = cli_network {
        return net.to_string();
    }
    let name = wallet_name
        .map(|n| n.to_string())
        .or_else(|| get_default_wallet().ok().flatten())
        .unwrap_or_default();
    if name.is_empty() {
        return "mainnet".to_string();
    }
    get_network(&name)
        .unwrap_or(None)
        .unwrap_or_else(|| "mainnet".to_string())
}

/// Delete a wallet file and its network metadata. Clears default if this was the default wallet.
pub fn delete_wallet(name: &str) -> Result<()> {
    validate_wallet_name(name)?;
    let dir = wallets_dir()?;

    // Delete mnemonic file
    let path = dir.join(name);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(anyhow::Error::new(e).context("failed to delete wallet file")),
    }

    // Delete sidecar files
    let net_path = dir.join(format!("{}.net", name));
    let _ = std::fs::remove_file(net_path);
    let account_path = dir.join(format!("{}.account", name));
    let _ = std::fs::remove_file(account_path);
    let keys_path = dir.join(format!("{}.keys", name));
    let _ = std::fs::remove_file(keys_path);

    // Clear default if this was the default wallet
    if let Ok(Some(default_name)) = get_default_wallet() {
        if default_name == name {
            clear_default_wallet()?;
        }
    }

    Ok(())
}

/// Set the default wallet name.
pub fn set_default_wallet(name: &str) -> Result<()> {
    validate_wallet_name(name)?;
    let path = nearw_dir()?.join("default");
    let content = format!("{}\n", name);
    std::fs::write(&path, content).context("failed to write default wallet file")?;
    Ok(())
}

/// Get the default wallet name. Returns Ok(None) if no default is set.
pub fn get_default_wallet() -> Result<Option<String>> {
    let path = nearw_dir()?.join("default");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let trimmed = content.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read default wallet file")),
    }
}

/// Remove the default file.
pub fn clear_default_wallet() -> Result<()> {
    let path = nearw_dir()?.join("default");
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(anyhow::Error::new(e).context("failed to clear default wallet")),
    }
}

/// List all wallet names (filenames in wallets/, excluding .net sidecar files).
pub fn list_wallets() -> Result<Vec<String>> {
    let dir = wallets_dir()?;
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir).context("failed to read wallets directory")? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".net")
                    || name.ends_with(".account")
                    || name.ends_with(".keys")
                {
                    continue;
                }
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Check if a wallet exists.
pub fn wallet_exists(name: &str) -> Result<bool> {
    validate_wallet_name(name)?;
    let path = wallets_dir()?.join(name);
    Ok(path.exists())
}

/// Find wallet by its named account (e.g. "alice.testnet" → wallet that owns it).
fn find_wallet_by_account(account: &str) -> Result<Option<String>> {
    for name in list_wallets()? {
        if let Ok(Some(stored)) = get_account_id(&name) {
            if stored == account {
                return Ok(Some(name));
            }
        }
    }
    Ok(None)
}

/// Resolve wallet name: explicit name > account lookup > default > error.
/// Accepts both local wallet names ("mywallet") and NEAR account names ("alice.testnet").
pub fn resolve_wallet_name(name: Option<&str>) -> Result<String> {
    if let Some(n) = name {
        // If it contains a dot, it's a NEAR account name — find the owning wallet
        if n.contains('.') {
            return match find_wallet_by_account(n)? {
                Some(wallet) => Ok(wallet),
                None => Err(anyhow::anyhow!(
                    "no wallet found for account '{}'. Check with: nearw wallet list",
                    n
                )),
            };
        }

        validate_wallet_name(n)?;
        if !wallet_exists(n)? {
            return Err(StorageError::WalletNotFound {
                name: n.to_string(),
            }
            .into());
        }
        return Ok(n.to_string());
    }

    match get_default_wallet()? {
        Some(default_name) => {
            if !wallet_exists(&default_name)? {
                return Err(StorageError::WalletNotFound { name: default_name }.into());
            }
            Ok(default_name)
        }
        None => Err(StorageError::NoDefaultWallet.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up a temp directory and configure the base dir override.
    /// Returns the TempDir guard (must be kept alive for the test).
    fn setup_temp_home() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        set_base_dir_override(Some(tmp.path().to_path_buf()));
        tmp
    }

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_wallet_name("my-wallet").is_ok());
        assert!(validate_wallet_name("test_1").is_ok());
        assert!(validate_wallet_name("ABC123").is_ok());
        assert!(validate_wallet_name("a").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(validate_wallet_name("../etc").is_err());
        assert!(validate_wallet_name("").is_err());
        assert!(validate_wallet_name("a b").is_err());
        assert!(validate_wallet_name("a/b").is_err());
        assert!(validate_wallet_name("a.b").is_err());
        assert!(validate_wallet_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn test_validate_name_rejects_reserved() {
        assert!(validate_wallet_name("default").is_err());
        assert!(validate_wallet_name("config").is_err());
    }

    #[test]
    fn test_validate_name_rejects_dot_prefix() {
        assert!(validate_wallet_name(".hidden").is_err());
        assert!(validate_wallet_name(".").is_err());
    }

    #[test]
    fn test_store_and_get_mnemonic() {
        let _tmp = setup_temp_home();
        store_mnemonic("test mnemonic phrase", "testwallet").unwrap();
        let result = get_mnemonic("testwallet").unwrap();
        assert_eq!(result, Some("test mnemonic phrase".to_string()));
    }

    #[test]
    fn test_get_mnemonic_not_found() {
        let _tmp = setup_temp_home();
        let result = get_mnemonic("nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_store_mnemonic_already_exists() {
        let _tmp = setup_temp_home();
        store_mnemonic("first", "dupwallet").unwrap();
        let result = store_mnemonic("second", "dupwallet");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("already exists"));
        // Verify original mnemonic is unchanged
        let mnemonic = get_mnemonic("dupwallet").unwrap();
        assert_eq!(mnemonic, Some("first".to_string()));
    }

    #[test]
    fn test_delete_wallet() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "todelete").unwrap();
        assert!(wallet_exists("todelete").unwrap());
        delete_wallet("todelete").unwrap();
        assert!(!wallet_exists("todelete").unwrap());
    }

    #[test]
    fn test_delete_wallet_not_found() {
        let _tmp = setup_temp_home();
        let result = delete_wallet("doesnotexist");
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_wallet_clears_default() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "mywallet").unwrap();
        set_default_wallet("mywallet").unwrap();
        assert_eq!(get_default_wallet().unwrap(), Some("mywallet".to_string()));
        delete_wallet("mywallet").unwrap();
        assert_eq!(get_default_wallet().unwrap(), None);
    }

    #[test]
    fn test_set_and_get_default() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "default-test").unwrap();
        set_default_wallet("default-test").unwrap();
        let result = get_default_wallet().unwrap();
        assert_eq!(result, Some("default-test".to_string()));
    }

    #[test]
    fn test_get_default_not_set() {
        let _tmp = setup_temp_home();
        let result = get_default_wallet().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_clear_default() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "clearme").unwrap();
        set_default_wallet("clearme").unwrap();
        clear_default_wallet().unwrap();
        assert_eq!(get_default_wallet().unwrap(), None);
    }

    #[test]
    fn test_list_wallets() {
        let _tmp = setup_temp_home();
        store_mnemonic("a", "alpha").unwrap();
        store_mnemonic("b", "beta").unwrap();
        let wallets = list_wallets().unwrap();
        assert_eq!(wallets, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn test_list_wallets_empty() {
        let _tmp = setup_temp_home();
        let wallets = list_wallets().unwrap();
        assert!(wallets.is_empty());
    }

    #[test]
    fn test_wallet_exists() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "exists-test").unwrap();
        assert!(wallet_exists("exists-test").unwrap());
        assert!(!wallet_exists("does-not-exist").unwrap());
    }

    #[test]
    fn test_resolve_wallet_name_explicit() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "explicit").unwrap();
        let name = resolve_wallet_name(Some("explicit")).unwrap();
        assert_eq!(name, "explicit");
    }

    #[test]
    fn test_resolve_wallet_name_default() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "default-w").unwrap();
        set_default_wallet("default-w").unwrap();
        let name = resolve_wallet_name(None).unwrap();
        assert_eq!(name, "default-w");
    }

    #[test]
    fn test_resolve_wallet_name_no_default() {
        let _tmp = setup_temp_home();
        let result = resolve_wallet_name(None);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no default wallet"));
    }

    #[cfg(unix)]
    #[test]
    fn test_file_permissions_unix() {
        use std::os::unix::fs::PermissionsExt;

        let _tmp = setup_temp_home();
        store_mnemonic("secret mnemonic", "perms-test").unwrap();
        let path = wallets_dir().unwrap().join("perms-test");
        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn test_dir_permissions_repaired() {
        use std::os::unix::fs::PermissionsExt;

        let _tmp = setup_temp_home();
        // Create wallets dir with bad permissions
        let dir = wallets_dir().unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);

        // Next access should repair
        let dir2 = wallets_dir().unwrap();
        let mode2 = std::fs::metadata(&dir2).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode2, 0o700);
    }

    #[test]
    fn test_store_and_get_network() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "net-test").unwrap();
        store_network("net-test", "mainnet").unwrap();
        assert_eq!(
            get_network("net-test").unwrap(),
            Some("mainnet".to_string())
        );

        store_network("net-test", "testnet").unwrap();
        assert_eq!(
            get_network("net-test").unwrap(),
            Some("testnet".to_string())
        );
    }

    #[test]
    fn test_resolve_network_cli_override() {
        let _tmp = setup_temp_home();
        store_mnemonic("test", "net-resolve").unwrap();
        store_network("net-resolve", "mainnet").unwrap();
        set_default_wallet("net-resolve").unwrap();

        // CLI flag should override stored value
        let net = resolve_network(Some("net-resolve"), Some("testnet"));
        assert_eq!(net, "testnet");

        // Without CLI flag, should use stored
        let net = resolve_network(Some("net-resolve"), None);
        assert_eq!(net, "mainnet");
    }

    #[test]
    fn test_list_wallets_skips_net_files() {
        let _tmp = setup_temp_home();
        store_mnemonic("a", "alpha").unwrap();
        store_network("alpha", "testnet").unwrap();
        let wallets = list_wallets().unwrap();
        assert_eq!(wallets, vec!["alpha".to_string()]);
    }
}
