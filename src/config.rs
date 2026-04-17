use anyhow::Result;
use serde::Deserialize;

/// Optional RPC endpoint overrides per network.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct RpcConfig {
    pub mainnet: Option<Vec<String>>,
    pub testnet: Option<Vec<String>>,
}

/// Top-level configuration loaded from ~/.nearw/config.toml.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct NearwConfig {
    #[serde(default)]
    pub rpc: Option<RpcConfig>,
    /// Override for 1Click API base URL (default: https://1click.chaindefuser.com)
    #[serde(default)]
    pub oneclick_api: Option<String>,
    /// JWT token for 1Click API (0% platform fee). Also checked via ONECLICK_JWT env var.
    #[serde(default)]
    pub oneclick_jwt: Option<String>,
}

/// Load config from ~/.nearw/config.toml. Returns defaults if file is missing.
pub fn load_config() -> Result<NearwConfig> {
    let base = crate::storage::base_dir()?;
    let path = base.join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let config: NearwConfig = toml::from_str(&content)?;
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(NearwConfig::default()),
        Err(e) => Err(anyhow::Error::new(e).context("failed to read config.toml")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;

    fn setup_temp_home() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        storage::set_base_dir_override(Some(tmp.path().to_path_buf()));
        tmp
    }

    #[test]
    fn test_default_config_empty() {
        let _tmp = setup_temp_home();
        let config = load_config().unwrap();
        assert!(config.rpc.is_none());
    }

    #[test]
    fn test_parse_full_config() {
        let tmp = setup_temp_home();
        let config_content = r#"
[rpc]
mainnet = ["https://rpc.mainnet.near.org"]
testnet = ["https://rpc.testnet.near.org"]
"#;
        std::fs::write(tmp.path().join("config.toml"), config_content).unwrap();
        let config = load_config().unwrap();
        let rpc = config.rpc.unwrap();
        assert_eq!(rpc.mainnet.unwrap().len(), 1);
        assert_eq!(rpc.testnet.unwrap().len(), 1);
    }

    #[test]
    fn test_parse_partial_config() {
        let tmp = setup_temp_home();
        let config_content = r#"
[rpc]
mainnet = ["https://custom-rpc.example.com"]
"#;
        std::fs::write(tmp.path().join("config.toml"), config_content).unwrap();
        let config = load_config().unwrap();
        let rpc = config.rpc.unwrap();
        assert!(rpc.mainnet.is_some());
        assert!(rpc.testnet.is_none());
    }
}
