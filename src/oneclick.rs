#![allow(dead_code)]

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Base URL for the 1Click Swap API.
const DEFAULT_API_BASE: &str = "https://1click.chaindefuser.com";

// -- Token list --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Token identifier in defuse format, e.g. "nep141:wrap.near"
    #[serde(rename = "assetId")]
    pub asset_id: String,
    /// Ticker symbol
    pub symbol: Option<String>,
    /// Decimal places
    pub decimals: Option<u8>,
    /// Chain/network for this token
    pub blockchain: Option<String>,
    /// Contract address on the native chain
    #[serde(rename = "contractAddress")]
    pub contract_address: Option<String>,
    /// Current price in USD
    pub price: Option<f64>,
}

// -- Quote --

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteRequest {
    /// Source token in defuse format: "nep141:wrap.near"
    pub origin_asset: String,
    /// Destination token in defuse format: "nep141:usdt.tether-token.near"
    pub destination_asset: String,
    /// Amount in smallest unit (string)
    pub amount: String,
    /// "EXACT_INPUT" or "EXACT_OUTPUT"
    pub swap_type: String,
    /// Slippage tolerance in basis points (100 = 1%)
    pub slippage_tolerance: u32,
    /// Deposit method: "INTENTS" for NEAR-native tokens
    pub deposit_type: String,
    /// Refund address (sender account)
    pub refund_to: String,
    /// Refund method: "INTENTS" for NEAR-native
    pub refund_type: String,
    /// Recipient address
    pub recipient: String,
    /// Recipient method: "INTENTS" for NEAR-native
    pub recipient_type: String,
    /// Whether this is a dry run (no actual quote)
    pub dry: bool,
    /// Deadline as ISO 8601 timestamp
    pub deadline: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteResponseInner {
    /// Unique deposit address for this swap (64-char hex implicit account)
    pub deposit_address: String,
    /// Amount to deposit (smallest units)
    pub amount_in: String,
    /// Human-readable amount in
    #[serde(default)]
    pub amount_in_formatted: Option<String>,
    /// Expected output amount (smallest units)
    pub amount_out: String,
    /// Human-readable expected output
    #[serde(default)]
    pub amount_out_formatted: Option<String>,
    /// Minimum output after slippage
    #[serde(default)]
    pub min_amount_out: Option<String>,
    /// Quote deadline (ISO 8601)
    #[serde(default)]
    pub deadline: Option<String>,
    /// Estimated time in seconds
    #[serde(default)]
    pub time_estimate: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuoteResponse {
    pub quote: QuoteResponseInner,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(rename = "correlationId", default)]
    pub correlation_id: Option<String>,
}

// -- Deposit submit --

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepositSubmitRequest {
    /// Transaction hash of the ft_transfer_call deposit
    pub tx_hash: String,
    /// Deposit address from the quote
    pub deposit_address: String,
}

// -- Status --

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapDetails {
    #[serde(default)]
    pub amount_out: Option<String>,
    #[serde(default)]
    pub amount_out_formatted: Option<String>,
    #[serde(default)]
    pub near_tx_hashes: Vec<String>,
    #[serde(default)]
    pub destination_chain_tx_hashes: Vec<String>,
    #[serde(default)]
    pub refund_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapStatus {
    /// Status: PENDING_DEPOSIT, DEPOSITED, SWAPPING, COMPLETED, FAILED, REFUNDED
    pub status: String,
    #[serde(default)]
    pub swap_details: Option<SwapDetails>,
    #[serde(default)]
    pub correlation_id: Option<String>,
}

impl SwapStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "COMPLETED" | "FAILED" | "REFUNDED")
    }

    pub fn is_completed(&self) -> bool {
        self.status == "COMPLETED"
    }

    /// Get output tx hash if available.
    pub fn tx_hash(&self) -> Option<&str> {
        self.swap_details.as_ref()
            .and_then(|d| d.near_tx_hashes.first().or(d.destination_chain_tx_hashes.first()))
            .map(|s| s.as_str())
    }

    /// Get output amount if available.
    pub fn amount_out(&self) -> Option<&str> {
        self.swap_details.as_ref()
            .and_then(|d| d.amount_out.as_deref())
    }

    /// Get formatted output amount if available.
    pub fn amount_out_formatted(&self) -> Option<&str> {
        self.swap_details.as_ref()
            .and_then(|d| d.amount_out_formatted.as_deref())
    }

    /// Get refund reason if available.
    pub fn refund_reason(&self) -> Option<&str> {
        self.swap_details.as_ref()
            .and_then(|d| d.refund_reason.as_deref())
    }
}

/// 1Click API client.
pub struct OneClickClient {
    base_url: String,
    client: reqwest::Client,
    /// Optional JWT for authenticated requests (0% platform fee).
    jwt: Option<String>,
}

/// Build a reqwest client with a 30-second timeout (F8: prevent indefinite hangs).
fn build_http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

impl OneClickClient {
    /// Create a client with the default API base URL.
    pub fn new() -> Result<Self> {
        Ok(Self {
            base_url: DEFAULT_API_BASE.to_string(),
            client: build_http_client()?,
            jwt: None,
        })
    }

    /// Create a client with a custom API base URL (for config override).
    pub fn with_base_url(base_url: String) -> Result<Self> {
        Ok(Self {
            base_url,
            client: build_http_client()?,
            jwt: None,
        })
    }

    /// Set JWT token for authenticated requests (eliminates 0.2% platform fee).
    pub fn with_jwt(mut self, jwt: Option<String>) -> Self {
        self.jwt = jwt;
        self
    }

    /// Attach JWT auth header to a request builder, if configured.
    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.jwt {
            Some(token) => req.header("Authorization", format!("Bearer {token}")),
            None => req,
        }
    }

    /// Fetch the list of supported tokens.
    pub async fn tokens(&self) -> Result<Vec<TokenInfo>> {
        let url = format!("{}/v0/tokens", self.base_url);
        let resp = self.auth(self.client.get(&url)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("1Click API /v0/tokens returned {}: {}", status, body);
        }
        let tokens: Vec<TokenInfo> = resp.json().await?;
        Ok(tokens)
    }

    /// Request a swap quote.
    pub async fn quote(&self, req: &QuoteRequest) -> Result<QuoteResponse> {
        let url = format!("{}/v0/quote", self.base_url);
        let resp = self.auth(self.client.post(&url).json(req)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("1Click API /v0/quote returned {}: {}", status, body);
        }
        let quote: QuoteResponse = resp.json().await?;
        Ok(quote)
    }

    /// Notify the API that a deposit transaction was sent.
    pub async fn submit_deposit(&self, tx_hash: &str, deposit_address: &str) -> Result<()> {
        let url = format!("{}/v0/deposit/submit", self.base_url);
        let req = DepositSubmitRequest {
            tx_hash: tx_hash.to_string(),
            deposit_address: deposit_address.to_string(),
        };
        let resp = self.auth(self.client.post(&url).json(&req)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("1Click API /v0/deposit/submit returned {}: {}", status, body);
        }
        Ok(())
    }

    /// Poll swap status by deposit address.
    /// F5: URL-encode query parameters to avoid injection/breakage.
    pub async fn status(&self, deposit_address: &str) -> Result<SwapStatus> {
        let base = reqwest::Url::parse(&format!("{}/v0/status", self.base_url))
            .map_err(|e| anyhow::anyhow!("invalid API base URL: {}", e))?;
        let url = reqwest::Url::parse_with_params(base.as_str(), &[("depositAddress", deposit_address)])
            .map_err(|e| anyhow::anyhow!("failed to build status URL: {}", e))?;

        let resp = self.auth(self.client.get(url)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("1Click API /v0/status returned {}: {}", status, body);
        }
        let status: SwapStatus = resp.json().await?;
        Ok(status)
    }
}
