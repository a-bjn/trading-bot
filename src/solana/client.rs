use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcSendTransactionConfig, RpcSimulateTransactionConfig};
use solana_sdk::{
    account::Account,
    commitment_config::{CommitmentConfig, CommitmentLevel},
    hash::Hash,
    pubkey::Pubkey,
    signature::{Signature, Signer},
    transaction::Transaction,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::sleep;
use tracing::{info, warn, error, debug};

use crate::solana::error::{Result, SolanaError};

/// RPC configuration
#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub primary_url: String,
    pub backup_urls: Vec<String>,
    pub helius_api_key: Option<String>,
    pub max_retries: u32,
    pub timeout_seconds: u64,
    pub rate_limit_per_second: u32,
    pub enable_pooling: bool,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            primary_url: "https://api.mainnet-beta.solana.com".to_string(),
            backup_urls: vec![],
            helius_api_key: None,
            max_retries: 3,
            timeout_seconds: 30,
            rate_limit_per_second: 10,
            enable_pooling: true,
        }
    }
}

impl RpcConfig {
    pub fn from_env() -> Self {
        let helius_key = std::env::var("HELIUS_API_KEY").ok();
        let primary_url = if let Some(ref key) = helius_key {
            format!("https://mainnet.helius-rpc.com/?api-key={}", key)
        } else {
            "https://api.mainnet-beta.solana.com".to_string()
        };

        Self {
            primary_url,
            helius_api_key: helius_key,
            ..Default::default()
        }
    }

    pub fn with_helius(api_key: String) -> Self {
        Self {
            primary_url: format!("https://mainnet.helius-rpc.com/?api-key={}", api_key),
            helius_api_key: Some(api_key),
            ..Default::default()
        }
    }
}

/// Rate limiter for RPC requests
struct RateLimiter {
    semaphore: Semaphore,
    tokens_per_second: u32,
    last_refill: Mutex<Instant>,
}

impl RateLimiter {
    fn new(tokens_per_second: u32) -> Self {
        Self {
            semaphore: Semaphore::new(tokens_per_second as usize),
            tokens_per_second,
            last_refill: Mutex::new(Instant::now()),
        }
    }

    async fn acquire(&self) {
        // Refill tokens if a second has passed
        let mut last_refill = self.last_refill.lock().await;
        if last_refill.elapsed() >= Duration::from_secs(1) {
            // Add available permits (up to the max)
            let available = self.semaphore.available_permits();
            let to_add = (self.tokens_per_second as usize).saturating_sub(available);
            if to_add > 0 {
                self.semaphore.add_permits(to_add);
            }
            *last_refill = Instant::now();
        }
        drop(last_refill);

        // Acquire a permit
        let _ = self.semaphore.acquire().await.unwrap();
    }
}

/// Enhanced Solana RPC client with retry logic and rate limiting
pub struct SolanaClient {
    primary_client: RpcClient,
    backup_clients: Vec<RpcClient>,
    config: RpcConfig,
    rate_limiter: Arc<RateLimiter>,
    current_backup_index: Arc<Mutex<usize>>,
}

impl SolanaClient {
    /// Create a new Solana client with configuration
    pub fn new(config: RpcConfig) -> Self {
        let primary_client = RpcClient::new(config.primary_url.clone());
        
        let backup_clients = config
            .backup_urls
            .iter()
            .map(|url| RpcClient::new(url.clone()))
            .collect();

        let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit_per_second));

        info!("Solana client initialized with primary RPC: {}", config.primary_url);
        
        Self {
            primary_client,
            backup_clients,
            config,
            rate_limiter,
            current_backup_index: Arc::new(Mutex::new(0)),
        }
    }

    /// Get the primary RPC client
    pub fn rpc(&self) -> &RpcClient {
        &self.primary_client
    }

    /// Execute with retry logic and exponential backoff
    async fn execute_with_retry<F, T, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut attempt = 0;
        let mut last_error = None;

        while attempt < self.config.max_retries {
            // Apply rate limiting
            self.rate_limiter.acquire().await;

            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempt += 1;
                    last_error = Some(e);

                    if attempt < self.config.max_retries {
                        // Exponential backoff: 100ms, 200ms, 400ms, etc.
                        let delay_ms = 100 * (2_u64.pow(attempt - 1));
                        debug!("Retry attempt {}/{} after {}ms", attempt, self.config.max_retries, delay_ms);
                        sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            SolanaError::RpcError("All retry attempts failed".to_string())
        }))
    }

    /// Try backup RPC if primary fails
    async fn try_backup<F, T>(&self, operation: F) -> Result<T>
    where
        F: Fn(&RpcClient) -> std::result::Result<T, solana_client::client_error::ClientError>,
    {
        if self.backup_clients.is_empty() {
            return Err(SolanaError::RpcError("No backup RPCs available".to_string()));
        }

        let mut index = self.current_backup_index.lock().await;
        let backup = &self.backup_clients[*index % self.backup_clients.len()];
        *index = (*index + 1) % self.backup_clients.len();
        drop(index);

        operation(backup).map_err(|e| e.into())
    }

    /// Get latest blockhash with retry
    pub async fn get_latest_blockhash(&self) -> Result<Hash> {
        self.execute_with_retry(|| async {
            self.primary_client
                .get_latest_blockhash()
                .map_err(|e: solana_client::client_error::ClientError| -> SolanaError { e.into() })
        })
        .await
    }

    /// Get account info with retry
    pub async fn get_account(&self, pubkey: &Pubkey) -> Result<Account> {
        self.execute_with_retry(|| async {
            self.primary_client
                .get_account(pubkey)
                .map_err(|e: solana_client::client_error::ClientError| -> SolanaError { e.into() })
        })
        .await
    }

    /// Get account info, returning None if account doesn't exist
    pub async fn get_account_optional(&self, pubkey: &Pubkey) -> Result<Option<Account>> {
        match self.get_account(pubkey).await {
            Ok(account) => Ok(Some(account)),
            Err(SolanaError::AccountNotFound(_)) => Ok(None),
            Err(e) => {
                // Check if it's an account not found error from RPC
                if e.to_string().contains("AccountNotFound") || e.to_string().contains("could not find account") {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Get multiple accounts with retry
    pub async fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>> {
        self.execute_with_retry(|| async {
            self.primary_client
                .get_multiple_accounts(pubkeys)
                .map_err(|e: solana_client::client_error::ClientError| -> SolanaError { e.into() })
        })
        .await
    }

    /// Get balance with retry
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.execute_with_retry(|| async {
            self.primary_client
                .get_balance(pubkey)
                .map_err(|e: solana_client::client_error::ClientError| -> SolanaError { e.into() })
        })
        .await
    }

    /// Get token accounts by owner
    pub async fn get_token_accounts_by_owner(
        &self,
        owner: &Pubkey,
        mint: Option<&Pubkey>,
    ) -> Result<Vec<(Pubkey, Account)>> {
        use solana_account_decoder::UiAccountEncoding;
        use solana_client::rpc_config::RpcAccountInfoConfig;

        self.execute_with_retry(|| async {
            let config = RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            };

            let filter = if let Some(mint) = mint {
                solana_client::rpc_filter::RpcFilterType::Memcmp(
                    solana_client::rpc_filter::Memcmp::new_base58_encoded(0, mint.as_ref()),
                )
            } else {
                return Err(SolanaError::RpcError("Mint filter required".to_string()));
            };

            let token_program = spl_token::id();
            
            self.primary_client
                .get_program_accounts_with_config(
                    &token_program,
                    solana_client::rpc_config::RpcProgramAccountsConfig {
                        filters: Some(vec![filter]),
                        account_config: config,
                        ..Default::default()
                    },
                )
                .map_err(|e: solana_client::client_error::ClientError| -> SolanaError { e.into() })
        })
        .await
    }

    /// Send transaction with retry
    pub async fn send_transaction(&self, transaction: &Transaction) -> Result<Signature> {
        let config = RpcSendTransactionConfig {
            skip_preflight: false,
            preflight_commitment: Some(CommitmentLevel::Confirmed),
            ..Default::default()
        };

        self.execute_with_retry(|| async {
            let result = match self.primary_client.send_transaction_with_config(transaction, config) {
                Ok(sig) => Ok(sig),
                Err(_) => {
                    self.try_backup(|client| {
                        client.send_transaction_with_config(transaction, config)
                    }).await
                }
            };

            result.map_err(|e: SolanaError| {
                error!("Transaction failed: {}", e);
                e
            })
        })
        .await
    }

    /// Send transaction and confirm
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
        timeout: Duration,
    ) -> Result<Signature> {
        let signature = self.send_transaction(transaction).await?;
        
        info!("Transaction sent: {}", signature);
        
        // Poll for confirmation
        self.confirm_transaction(&signature, timeout).await?;
        
        Ok(signature)
    }

    /// Simulate transaction
    pub async fn simulate_transaction(&self, transaction: &Transaction) -> Result<()> {
        self.execute_with_retry(|| async {
            let config = RpcSimulateTransactionConfig {
                commitment: Some(CommitmentConfig::confirmed()),
                ..Default::default()
            };

            let result = self
                .primary_client
                .simulate_transaction_with_config(transaction, config)
                .map_err(|e| SolanaError::SimulationError(format!("Simulation failed: {}", e)))?;

            if let Some(err) = result.value.err {
                return Err(SolanaError::SimulationError(format!("Transaction would fail: {:?}", err)));
            }

            debug!("Simulation successful. Compute units: {:?}", result.value.units_consumed);
            Ok(())
        })
        .await
    }

    /// Confirm transaction with polling
    pub async fn confirm_transaction(
        &self,
        signature: &Signature,
        timeout: Duration,
    ) -> Result<()> {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(500);

        while start.elapsed() < timeout {
            // Apply rate limiting
            self.rate_limiter.acquire().await;

            match self.primary_client.get_signature_status(signature) {
                Ok(Some(Ok(()))) => {
                    info!("Transaction confirmed: {}", signature);
                    return Ok(());
                }
                Ok(Some(Err(e))) => {
                    error!("Transaction failed: {:?}", e);
                    return Err(SolanaError::TransactionError(format!("Transaction failed: {:?}", e)));
                }
                Ok(None) => {
                    debug!("Transaction still pending: {}", signature);
                }
                Err(e) => {
                    warn!("Error checking transaction status: {}", e);
                }
            }

            sleep(poll_interval).await;
        }

        Err(SolanaError::TimeoutError(timeout.as_millis() as u64))
    }

    /// Get recent prioritization fees (Helius enhanced API)
    pub async fn get_recent_prioritization_fees(&self, accounts: &[Pubkey]) -> Result<Vec<u64>> {
        if self.config.helius_api_key.is_none() {
            // Fallback to default fees
            return Ok(vec![100_000; accounts.len()]);
        }

        self.execute_with_retry(|| async {
            // Use Helius enhanced API for better fee estimation
            let response = self
                .primary_client
                .get_recent_prioritization_fees(accounts)
                .map_err(|e: solana_client::client_error::ClientError| -> SolanaError { 
                    SolanaError::RpcError(format!("Failed to get fees: {}", e))
                })?;

            let fees: Vec<u64> = response
                .iter()
                .map(|fee| fee.prioritization_fee)
                .collect();

            debug!("Recent prioritization fees: {:?}", fees);
            Ok(fees)
        })
        .await
    }

    /// Get recommended prioritization fee
    pub async fn get_recommended_fee(&self, accounts: &[Pubkey]) -> Result<u64> {
        let fees = self.get_recent_prioritization_fees(accounts).await?;
        
        if fees.is_empty() {
            return Ok(100_000); // Default fee
        }

        // Use 75th percentile
        let mut sorted_fees = fees.clone();
        sorted_fees.sort_unstable();
        let index = (sorted_fees.len() * 3) / 4;
        
        Ok(sorted_fees[index])
    }

    /// Get configuration
    pub fn config(&self) -> &RpcConfig {
        &self.config
    }

    /// Check if using Helius
    pub fn is_helius(&self) -> bool {
        self.config.helius_api_key.is_some()
    }
}

// Thread-safe cloning
impl Clone for SolanaClient {
    fn clone(&self) -> Self {
        Self::new(self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_config_default() {
        let config = RpcConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.rate_limit_per_second, 10);
    }

    #[test]
    fn test_helius_config() {
        let config = RpcConfig::with_helius("test-key".to_string());
        assert!(config.primary_url.contains("helius"));
        assert_eq!(config.helius_api_key, Some("test-key".to_string()));
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(5);
        
        // Should be able to acquire immediately
        let start = Instant::now();
        for _ in 0..5 {
            limiter.acquire().await;
        }
        let elapsed = start.elapsed();
        
        // Should complete quickly (within 100ms)
        assert!(elapsed < Duration::from_millis(100));
    }
}
