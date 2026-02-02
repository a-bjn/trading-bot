use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_client::GetConfirmedSignaturesForAddress2Config,
    rpc_config::RpcTransactionConfig,
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, UiTransactionEncoding,
};
use std::{
    collections::HashSet,
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, RwLock},
    time::{interval, sleep},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::copy_trader::{
    decoder::TransactionDecoder,
    signal::{SignalFilter, TradeSignal},
    CopyTraderConfig,
};

/// Wallet follower for monitoring and copying trades
pub struct WalletFollower {
    config: CopyTraderConfig,
    rpc_client: Arc<RpcClient>,
    decoder: TransactionDecoder,
    signal_filter: Arc<RwLock<SignalFilter>>,
    last_signature: Arc<RwLock<Option<Signature>>>,
    rate_limiter: Arc<RwLock<RateLimiter>>,
}

impl WalletFollower {
    /// Create a new wallet follower
    pub fn new(config: CopyTraderConfig) -> Result<Self> {
        config.validate()?;
        
        let target_wallet = config.get_target_pubkey()?;
        let rpc_client = Arc::new(RpcClient::new(config.rpc_url.clone()));
        let decoder = TransactionDecoder::new(target_wallet);
        let signal_filter = Arc::new(RwLock::new(SignalFilter::new(true, 0.5)));
        let rate_limiter = Arc::new(RwLock::new(RateLimiter::new(config.rate_limit_rps)));
        
        Ok(Self {
            config,
            rpc_client,
            decoder,
            signal_filter,
            last_signature: Arc::new(RwLock::new(None)),
            rate_limiter,
        })
    }
    
    /// Start monitoring the target wallet
    pub async fn start(&self, signal_tx: mpsc::UnboundedSender<TradeSignal>) -> Result<()> {
        info!("Starting wallet follower for {}", self.config.target_wallet);
        
        // Start WebSocket subscription for real-time updates
        let ws_handle = self.start_websocket_monitor(signal_tx.clone());
        
        // Start polling as backup and for historical data
        let poll_handle = self.start_polling_monitor(signal_tx.clone());
        
        // Wait for both tasks
        tokio::select! {
            result = ws_handle => {
                error!("WebSocket monitor stopped: {:?}", result);
                result
            }
            result = poll_handle => {
                error!("Polling monitor stopped: {:?}", result);
                result
            }
        }
    }
    
    /// Start WebSocket monitoring for real-time transaction updates
    async fn start_websocket_monitor(
        &self,
        signal_tx: mpsc::UnboundedSender<TradeSignal>,
    ) -> Result<()> {
        info!("Connecting to WebSocket: {}", self.config.ws_url);
        
        let target_wallet = self.config.get_target_pubkey()?;
        
        loop {
            match self.run_websocket_connection(&target_wallet, &signal_tx).await {
                Ok(_) => {
                    warn!("WebSocket connection closed, reconnecting in 5 seconds...");
                }
                Err(e) => {
                    error!("WebSocket error: {}, reconnecting in 5 seconds...", e);
                }
            }
            
            sleep(Duration::from_secs(5)).await;
        }
    }
    
    /// Run a single WebSocket connection
    async fn run_websocket_connection(
        &self,
        target_wallet: &Pubkey,
        signal_tx: &mpsc::UnboundedSender<TradeSignal>,
    ) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.config.ws_url)
            .await
            .map_err(|e| anyhow!("WebSocket connection failed: {}", e))?;
        
        let (mut write, mut read) = ws_stream.split();
        
        // Subscribe to account notifications
        let subscribe_msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "accountSubscribe",
            "params": [
                target_wallet.to_string(),
                {
                    "encoding": "jsonParsed",
                    "commitment": "confirmed"
                }
            ]
        });
        
        write.send(Message::Text(subscribe_msg.to_string())).await?;
        info!("Subscribed to wallet notifications via WebSocket");
        
        // Also subscribe to signature notifications
        let sig_subscribe_msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "signatureSubscribe",
            "params": [
                target_wallet.to_string(),
                {
                    "commitment": "confirmed"
                }
            ]
        });
        
        write.send(Message::Text(sig_subscribe_msg.to_string())).await?;
        
        // Process incoming messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.handle_websocket_message(&text, signal_tx).await {
                        warn!("Error handling WebSocket message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed by server");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    /// Handle incoming WebSocket message
    async fn handle_websocket_message(
        &self,
        message: &str,
        signal_tx: &mpsc::UnboundedSender<TradeSignal>,
    ) -> Result<()> {
        #[derive(Deserialize)]
        struct WsResponse {
            method: Option<String>,
            params: Option<serde_json::Value>,
        }
        
        let response: WsResponse = serde_json::from_str(message)?;
        
        if let Some(method) = response.method {
            if method == "accountNotification" || method == "signatureNotification" {
                // When we get a notification, fetch the latest transactions
                if let Err(e) = self.fetch_and_process_latest_transactions(signal_tx).await {
                    warn!("Error processing notification: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Start polling monitor as backup
    async fn start_polling_monitor(
        &self,
        signal_tx: mpsc::UnboundedSender<TradeSignal>,
    ) -> Result<()> {
        info!("Starting polling monitor (backup)");
        
        let mut poll_interval = interval(Duration::from_secs(5));
        
        loop {
            poll_interval.tick().await;
            
            if let Err(e) = self.fetch_and_process_latest_transactions(&signal_tx).await {
                warn!("Polling error: {}", e);
            }
        }
    }
    
    /// Fetch and process latest transactions
    async fn fetch_and_process_latest_transactions(
        &self,
        signal_tx: &mpsc::UnboundedSender<TradeSignal>,
    ) -> Result<()> {
        // Rate limiting
        self.rate_limiter.write().await.wait().await;
        
        let target_wallet = self.config.get_target_pubkey()?;
        
        // Get the last processed signature
        let before = self.last_signature.read().await.clone();
        
        // Fetch recent signatures
        let config = GetConfirmedSignaturesForAddress2Config {
            before,
            until: None,
            limit: Some(10),
            commitment: Some(CommitmentConfig::confirmed()),
        };
        
        let signatures = self
            .fetch_signatures_with_retry(&target_wallet, config)
            .await?;
        
        if signatures.is_empty() {
            debug!("No new transactions");
            return Ok(());
        }
        
        info!("Found {} new transactions", signatures.len());
        
        // Process each signature (in reverse to get chronological order)
        for sig_info in signatures.iter().rev() {
            if let Err(e) = self.process_signature(&sig_info.signature, signal_tx).await {
                warn!("Error processing signature {}: {}", sig_info.signature, e);
            }
        }
        
        // Update last signature
        if let Some(latest) = signatures.first() {
            if let Ok(sig) = Signature::from_str(&latest.signature) {
                *self.last_signature.write().await = Some(sig);
            }
        }
        
        Ok(())
    }
    
    /// Fetch signatures with retry logic
    async fn fetch_signatures_with_retry(
        &self,
        wallet: &Pubkey,
        config: GetConfirmedSignaturesForAddress2Config,
    ) -> Result<Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature>> {
        let mut retries = 0;
        
        loop {
            let config_clone = GetConfirmedSignaturesForAddress2Config {
                before: config.before,
                until: config.until,
                limit: config.limit,
                commitment: config.commitment,
            };
            
            match self.rpc_client.get_signatures_for_address_with_config(wallet, config_clone).await {
                Ok(sigs) => return Ok(sigs),
                Err(e) => {
                    retries += 1;
                    if retries >= self.config.max_rpc_retries {
                        return Err(anyhow!("Failed to fetch signatures after {} retries: {}", retries, e));
                    }
                    
                    warn!("RPC error (retry {}/{}): {}", retries, self.config.max_rpc_retries, e);
                    sleep(Duration::from_millis(500 * retries as u64)).await;
                }
            }
        }
    }
    
    /// Process a single transaction signature
    async fn process_signature(
        &self,
        signature_str: &str,
        signal_tx: &mpsc::UnboundedSender<TradeSignal>,
    ) -> Result<()> {
        // Rate limiting
        self.rate_limiter.write().await.wait().await;
        
        // Check if already processed (duplicate detection)
        if self.config.enable_duplicate_detection {
            if self.signal_filter.read().await.has_seen(signature_str) {
                debug!("Skipping duplicate signature: {}", signature_str);
                return Ok(());
            }
        }
        
        let signature = Signature::from_str(signature_str)?;
        
        // Fetch transaction details
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };
        
        let transaction = self
            .fetch_transaction_with_retry(&signature, config)
            .await?;
        
        // Decode transaction
        if let Some(signal) = self.decoder.decode_transaction(&transaction, signature_str)? {
            // Filter signal
            let mut filter = self.signal_filter.write().await;
            let filter_result = filter.filter(&signal);
            drop(filter);
            
            if filter_result.is_accepted() {
                // Check minimum trade size
                if signal.meets_minimum(self.config.min_trade_size) {
                    // Check maximum position size
                    if !signal.exceeds_max_position(
                        self.config.copy_percentage,
                        self.config.max_position_size,
                    ) {
                        info!(
                            "New trade signal: {:?} {} SOL of token {}",
                            signal.action, signal.sol_amount, signal.token_mint
                        );
                        
                        signal_tx.send(signal)?;
                    } else {
                        debug!("Signal exceeds max position size");
                    }
                } else {
                    debug!("Signal below minimum trade size");
                }
            } else {
                debug!("Signal filtered: {:?}", filter_result);
            }
        }
        
        Ok(())
    }
    
    /// Fetch transaction with retry logic
    async fn fetch_transaction_with_retry(
        &self,
        signature: &Signature,
        config: RpcTransactionConfig,
    ) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
        let mut retries = 0;
        
        loop {
            match self.rpc_client.get_transaction_with_config(signature, config).await {
                Ok(tx) => return Ok(tx),
                Err(e) => {
                    retries += 1;
                    if retries >= self.config.max_rpc_retries {
                        return Err(anyhow!("Failed to fetch transaction after {} retries: {}", retries, e));
                    }
                    
                    warn!("RPC error fetching transaction (retry {}/{}): {}", retries, self.config.max_rpc_retries, e);
                    sleep(Duration::from_millis(500 * retries as u64)).await;
                }
            }
        }
    }
    
    /// Fetch using Helius enhanced transactions API (if API key provided)
    #[allow(dead_code)]
    async fn fetch_helius_transactions(
        &self,
        wallet: &Pubkey,
    ) -> Result<Vec<HeliusTransaction>> {
        let api_key = self.config.helius_api_key.as_ref()
            .ok_or_else(|| anyhow!("Helius API key not configured"))?;
        
        let url = format!(
            "https://api.helius.xyz/v0/addresses/{}/transactions?api-key={}",
            wallet, api_key
        );
        
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        
        if !response.status().is_success() {
            return Err(anyhow!("Helius API error: {}", response.status()));
        }
        
        let transactions: Vec<HeliusTransaction> = response.json().await?;
        Ok(transactions)
    }
}

/// Rate limiter for RPC calls
struct RateLimiter {
    requests_per_second: u32,
    last_request: Option<tokio::time::Instant>,
}

impl RateLimiter {
    fn new(requests_per_second: u32) -> Self {
        Self {
            requests_per_second,
            last_request: None,
        }
    }
    
    async fn wait(&mut self) {
        if self.requests_per_second == 0 {
            return;
        }
        
        let min_interval = Duration::from_secs(1) / self.requests_per_second;
        
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < min_interval {
                sleep(min_interval - elapsed).await;
            }
        }
        
        self.last_request = Some(tokio::time::Instant::now());
    }
}

/// Helius transaction structure (simplified)
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HeliusTransaction {
    signature: String,
    timestamp: i64,
    #[serde(rename = "type")]
    tx_type: String,
    source: String,
    fee: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(10);
        assert_eq!(limiter.requests_per_second, 10);
    }
    
    #[tokio::test]
    async fn test_wallet_follower_creation() {
        let config = CopyTraderConfig::default();
        let follower = WalletFollower::new(config);
        assert!(follower.is_ok());
    }
}
