use reqwest::Client;
use serde::{Deserialize, Serialize};
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Signature,
    system_instruction,
    transaction::Transaction,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn, error, debug};

use crate::solana::error::{Result, SolanaError};
use crate::solana::client::SolanaClient;
use crate::solana::wallet::Wallet;

/// Jito block engine URLs
pub const JITO_MAINNET_URL: &str = "https://mainnet.block-engine.jito.wtf";
pub const JITO_NY_URL: &str = "https://ny.mainnet.block-engine.jito.wtf";
pub const JITO_AMSTERDAM_URL: &str = "https://amsterdam.mainnet.block-engine.jito.wtf";
pub const JITO_FRANKFURT_URL: &str = "https://frankfurt.mainnet.block-engine.jito.wtf";
pub const JITO_TOKYO_URL: &str = "https://tokyo.mainnet.block-engine.jito.wtf";

/// Jito tip accounts (random selection for load balancing)
pub const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

/// Jito bundle status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleStatus {
    Pending,
    Accepted,
    Rejected,
    Failed,
}

/// Jito bundle response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleResponse {
    pub jsonrpc: String,
    pub result: String,
    pub id: u64,
}

/// Jito bundle status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleStatusResponse {
    pub jsonrpc: String,
    pub result: BundleStatusResult,
    pub id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleStatusResult {
    pub context: StatusContext,
    pub value: Vec<BundleStatusValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusContext {
    pub slot: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleStatusValue {
    pub bundle_id: String,
    pub transactions: Vec<String>,
    pub slot: u64,
    pub confirmation_status: String,
    pub err: Option<serde_json::Value>,
}

/// Jito client for bundle submission
pub struct JitoClient {
    client: Client,
    block_engine_url: String,
    solana_client: Arc<SolanaClient>,
    tip_amount: u64,
    fallback_enabled: bool,
}

impl JitoClient {
    /// Create a new Jito client
    pub fn new(
        block_engine_url: String,
        solana_client: Arc<SolanaClient>,
        tip_amount: u64,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            client,
            block_engine_url,
            solana_client,
            tip_amount,
            fallback_enabled: true,
        }
    }

    /// Create with default mainnet URL
    pub fn new_mainnet(solana_client: Arc<SolanaClient>, tip_amount: u64) -> Self {
        Self::new(JITO_MAINNET_URL.to_string(), solana_client, tip_amount)
    }

    /// Enable or disable fallback to regular RPC
    pub fn set_fallback(&mut self, enabled: bool) {
        self.fallback_enabled = enabled;
    }

    /// Get a random tip account
    pub fn get_random_tip_account() -> Result<Pubkey> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..JITO_TIP_ACCOUNTS.len());
        
        JITO_TIP_ACCOUNTS[index]
            .parse()
            .map_err(|e| SolanaError::JitoError(format!("Invalid tip account: {}", e)))
    }

    /// Build a tip transaction
    pub fn build_tip_instruction(
        &self,
        from: &Pubkey,
        tip_account: &Pubkey,
    ) -> Instruction {
        system_instruction::transfer(from, tip_account, self.tip_amount)
    }

    /// Create a tip transaction
    pub async fn create_tip_transaction(
        &self,
        wallet: &Wallet,
    ) -> Result<Transaction> {
        let tip_account = Self::get_random_tip_account()?;
        let tip_instruction = self.build_tip_instruction(&wallet.pubkey(), &tip_account);

        let recent_blockhash = self.solana_client.get_latest_blockhash().await?;
        
        let mut transaction = Transaction::new_with_payer(
            &[tip_instruction],
            Some(&wallet.pubkey()),
        );
        
        transaction.message.recent_blockhash = recent_blockhash;
        
        Ok(transaction)
    }

    /// Submit a bundle of transactions
    pub async fn send_bundle(
        &self,
        transactions: &[Transaction],
    ) -> Result<String> {
        info!("Submitting bundle with {} transactions to Jito", transactions.len());

        // Serialize transactions to base58
        let encoded_txs: Vec<String> = transactions
            .iter()
            .map(|tx| {
                let serialized = bincode::serialize(tx)
                    .map_err(|e| SolanaError::SerializationError(format!("Failed to serialize: {}", e)))?;
                Ok(bs58::encode(serialized).into_string())
            })
            .collect::<Result<Vec<String>>>()?;

        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendBundle",
            "params": [encoded_txs]
        });

        debug!("Jito request: {}", serde_json::to_string_pretty(&request_body).unwrap());

        let response = self
            .client
            .post(format!("{}/api/v1/bundles", self.block_engine_url))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SolanaError::NetworkError(format!("Jito request failed: {}", e)))?;

        let status = response.status();
        let text = response.text().await.map_err(|e| {
            SolanaError::NetworkError(format!("Failed to read response: {}", e))
        })?;

        if !status.is_success() {
            error!("Jito bundle submission failed: {} - {}", status, text);
            return Err(SolanaError::BundleError(format!("HTTP {}: {}", status, text)));
        }

        debug!("Jito response: {}", text);

        let bundle_response: BundleResponse = serde_json::from_str(&text)
            .map_err(|e| SolanaError::ParseError(format!("Failed to parse response: {}", e)))?;

        info!("Bundle submitted successfully: {}", bundle_response.result);
        Ok(bundle_response.result)
    }

    /// Send bundle with automatic tip transaction
    pub async fn send_bundle_with_tip(
        &self,
        wallet: &Wallet,
        mut transactions: Vec<Transaction>,
    ) -> Result<String> {
        // Create and sign tip transaction
        let mut tip_tx = self.create_tip_transaction(wallet).await?;
        wallet.sign_transaction(&mut tip_tx)?;

        // Add tip transaction to the end of the bundle
        transactions.push(tip_tx);

        self.send_bundle(&transactions).await
    }

    /// Check bundle status
    pub async fn get_bundle_status(&self, bundle_id: &str) -> Result<BundleStatusResponse> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBundleStatuses",
            "params": [[bundle_id]]
        });

        let response = self
            .client
            .post(format!("{}/api/v1/bundles", self.block_engine_url))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| SolanaError::NetworkError(format!("Status request failed: {}", e)))?;

        let status_response: BundleStatusResponse = response
            .json()
            .await
            .map_err(|e| SolanaError::ParseError(format!("Failed to parse status: {}", e)))?;

        Ok(status_response)
    }

    /// Send bundle with retry and fallback
    pub async fn send_bundle_with_fallback(
        &self,
        wallet: &Wallet,
        transactions: Vec<Transaction>,
        max_retries: u32,
    ) -> Result<Vec<Signature>> {
        let mut last_error = None;

        // Try Jito first
        for attempt in 0..max_retries {
            match self.send_bundle_with_tip(wallet, transactions.clone()).await {
                Ok(bundle_id) => {
                    info!("Bundle submitted via Jito: {}", bundle_id);
                    
                    // Wait a bit and check status
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    
                    match self.get_bundle_status(&bundle_id).await {
                        Ok(status) => {
                            debug!("Bundle status: {:?}", status);
                            // Return transaction signatures
                            let signatures: Vec<Signature> = transactions
                                .iter()
                                .filter_map(|tx| tx.signatures.first().copied())
                                .collect();
                            return Ok(signatures);
                        }
                        Err(e) => {
                            warn!("Failed to get bundle status: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Jito submission failed (attempt {}/{}): {}", attempt + 1, max_retries, e);
                    last_error = Some(e);
                    
                    if attempt < max_retries - 1 {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        }

        // Fallback to regular RPC if enabled
        if self.fallback_enabled {
            warn!("Falling back to regular RPC submission");
            
            let mut signatures = Vec::new();
            for tx in &transactions {
                match self.solana_client.send_transaction(tx).await {
                    Ok(sig) => signatures.push(sig),
                    Err(e) => {
                        error!("Fallback transaction failed: {}", e);
                        return Err(e);
                    }
                }
            }
            
            return Ok(signatures);
        }

        Err(last_error.unwrap_or_else(|| {
            SolanaError::BundleError("All bundle submission attempts failed".to_string())
        }))
    }

    /// Get tip amount in lamports
    pub fn tip_amount(&self) -> u64 {
        self.tip_amount
    }

    /// Set tip amount
    pub fn set_tip_amount(&mut self, lamports: u64) {
        self.tip_amount = lamports;
    }

    /// Get block engine URL
    pub fn block_engine_url(&self) -> &str {
        &self.block_engine_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tip_account_parsing() {
        for account in JITO_TIP_ACCOUNTS.iter() {
            assert!(account.parse::<Pubkey>().is_ok());
        }
    }

    #[test]
    fn test_random_tip_account() {
        let account = JitoClient::get_random_tip_account().unwrap();
        assert!(JITO_TIP_ACCOUNTS.iter().any(|a| a.parse::<Pubkey>().unwrap() == account));
    }
}
