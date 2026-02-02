use futures::{stream::StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream, MaybeTlsStream};
use tracing::{info, warn, error, debug};

use crate::solana::error::{Result, SolanaError};

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum SubscriptionType {
    #[serde(rename = "logsSubscribe")]
    Logs {
        mentions: Vec<String>,
        commitment: String,
    },
    #[serde(rename = "accountSubscribe")]
    Account {
        account: String,
        commitment: String,
        encoding: String,
    },
    #[serde(rename = "signatureSubscribe")]
    Signature {
        signature: String,
        commitment: String,
    },
    #[serde(rename = "slotSubscribe")]
    Slot,
    #[serde(rename = "programSubscribe")]
    Program {
        program_id: String,
        commitment: String,
        encoding: String,
    },
}

/// WebSocket notification wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: NotificationParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationParams {
    pub subscription: u64,
    pub result: Value,
}

/// Subscription handler trait
#[async_trait::async_trait]
pub trait SubscriptionHandler: Send + Sync {
    async fn handle_notification(&self, notification: Value) -> Result<()>;
}

/// Log subscription handler
pub struct LogHandler<F>
where
    F: Fn(Value) -> futures::future::BoxFuture<'static, Result<()>> + Send + Sync,
{
    callback: F,
}

impl<F> LogHandler<F>
where
    F: Fn(Value) -> futures::future::BoxFuture<'static, Result<()>> + Send + Sync,
{
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

#[async_trait::async_trait]
impl<F> SubscriptionHandler for LogHandler<F>
where
    F: Fn(Value) -> futures::future::BoxFuture<'static, Result<()>> + Send + Sync,
{
    async fn handle_notification(&self, notification: Value) -> Result<()> {
        (self.callback)(notification).await
    }
}

/// WebSocket manager for handling multiple subscriptions
pub struct WebSocketManager {
    ws_url: String,
    subscriptions: Arc<RwLock<HashMap<u64, (SubscriptionType, Arc<dyn SubscriptionHandler>)>>>,
    next_request_id: Arc<RwLock<u64>>,
    reconnect_attempts: Arc<Mutex<u32>>,
    max_reconnect_attempts: u32,
    reconnect_delay_ms: u64,
    ping_interval_seconds: u64,
}

impl WebSocketManager {
    /// Create a new WebSocket manager
    pub fn new(ws_url: String) -> Self {
        Self {
            ws_url,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            next_request_id: Arc::new(RwLock::new(1)),
            reconnect_attempts: Arc::new(Mutex::new(0)),
            max_reconnect_attempts: 10,
            reconnect_delay_ms: 1000,
            ping_interval_seconds: 30,
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        ws_url: String,
        max_reconnect_attempts: u32,
        reconnect_delay_ms: u64,
        ping_interval_seconds: u64,
    ) -> Self {
        Self {
            ws_url,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            next_request_id: Arc::new(RwLock::new(1)),
            reconnect_attempts: Arc::new(Mutex::new(0)),
            max_reconnect_attempts,
            reconnect_delay_ms,
            ping_interval_seconds,
        }
    }

    /// Connect to WebSocket and start listening
    pub async fn connect(&self) -> Result<()> {
        info!("Connecting to WebSocket: {}", self.ws_url);

        loop {
            let mut attempts = self.reconnect_attempts.lock().await;
            
            match self.try_connect().await {
                Ok(_) => {
                    info!("WebSocket connected successfully");
                    *attempts = 0;
                    drop(attempts);
                    
                    // Connection will be maintained by the handler
                    // Return when connection is established
                    return Ok(());
                }
                Err(e) => {
                    *attempts += 1;
                    let current_attempt = *attempts;
                    
                    if current_attempt >= self.max_reconnect_attempts {
                        error!("Max reconnection attempts reached");
                        return Err(e);
                    }

                    warn!(
                        "Connection failed (attempt {}/{}): {}",
                        current_attempt, self.max_reconnect_attempts, e
                    );

                    let delay = self.reconnect_delay_ms * (2_u64.pow(current_attempt.min(5)));
                    drop(attempts);
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }

    /// Connect and maintain connection with auto-reconnect
    pub async fn connect_and_maintain(&self) -> Result<()> {
        loop {
            if let Err(e) = self.connect().await {
                error!("WebSocket connection lost: {}", e);
                
                // Wait before reconnecting
                tokio::time::sleep(tokio::time::Duration::from_millis(self.reconnect_delay_ms)).await;
                info!("Attempting to reconnect...");
            }
        }
    }

    async fn try_connect(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.ws_url)
            .await
            .map_err(|e| SolanaError::WebSocketError(format!("Connection failed: {}", e)))?;

        self.handle_connection(ws_stream).await
    }

    async fn handle_connection(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    ) -> Result<()> {
        let (mut write, mut read) = ws_stream.split();

        // Resubscribe to all existing subscriptions
        let subscriptions = self.subscriptions.read().await;
        for (sub_id, (sub_type, _)) in subscriptions.iter() {
            debug!("Resubscribing: {:?}", sub_type);
            let request = self.build_subscription_request(sub_type).await?;
            write
                .send(Message::Text(request))
                .await
                .map_err(|e| SolanaError::WebSocketError(format!("Failed to send: {}", e)))?;
        }
        drop(subscriptions);

        // Spawn ping task
        let ping_interval = tokio::time::Duration::from_secs(self.ping_interval_seconds);
        let (ping_tx, mut ping_rx) = mpsc::channel::<()>(1);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(ping_interval);
            loop {
                interval.tick().await;
                if ping_tx.send(()).await.is_err() {
                    break;
                }
            }
        });

        // Message handling loop
        loop {
            tokio::select! {
                Some(message) = read.next() => {
                    match message {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = self.handle_message(&text).await {
                                error!("Error handling message: {}", e);
                            }
                        }
                        Ok(Message::Ping(data)) => {
                            write
                                .send(Message::Pong(data))
                                .await
                                .map_err(|e| SolanaError::WebSocketError(format!("Failed to send pong: {}", e)))?;
                        }
                        Ok(Message::Pong(_)) => {
                            debug!("Received pong");
                        }
                        Ok(Message::Close(_)) => {
                            info!("WebSocket closed by server");
                            return Err(SolanaError::WebSocketError("Connection closed".to_string()));
                        }
                        Err(e) => {
                            error!("WebSocket error: {}", e);
                            return Err(SolanaError::WebSocketError(format!("Read error: {}", e)));
                        }
                        _ => {}
                    }
                }
                Some(_) = ping_rx.recv() => {
                    // Send periodic ping to keep connection alive
                    if let Err(e) = write.send(Message::Ping(vec![])).await {
                        error!("Failed to send ping: {}", e);
                        return Err(SolanaError::WebSocketError(format!("Failed to send ping: {}", e)));
                    }
                    debug!("Sent ping");
                }
            }
        }
    }

    async fn handle_message(&self, text: &str) -> Result<()> {
        debug!("Received message: {}", text);

        let value: Value = serde_json::from_str(text)
            .map_err(|e| SolanaError::ParseError(format!("Failed to parse JSON: {}", e)))?;

        // Check if it's a notification
        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
            if method.ends_with("Notification") {
                if let Some(params) = value.get("params") {
                    if let Some(subscription) = params.get("subscription").and_then(|s| s.as_u64()) {
                        return self.route_notification(subscription, params.clone()).await;
                    }
                }
            }
        }

        // Check if it's a subscription response
        if let Some(result) = value.get("result") {
            if let Some(sub_id) = result.as_u64() {
                info!("Subscription confirmed: {}", sub_id);
            }
        }

        Ok(())
    }

    async fn route_notification(&self, subscription_id: u64, params: Value) -> Result<()> {
        let subscriptions = self.subscriptions.read().await;
        
        if let Some((_, handler)) = subscriptions.get(&subscription_id) {
            let handler = Arc::clone(handler);
            drop(subscriptions);

            if let Some(result) = params.get("result") {
                handler.handle_notification(result.clone()).await?;
            }
        } else {
            warn!("No handler found for subscription: {}", subscription_id);
        }

        Ok(())
    }

    async fn build_subscription_request(&self, sub_type: &SubscriptionType) -> Result<String> {
        let mut request_id = self.next_request_id.write().await;
        let id = *request_id;
        *request_id += 1;
        drop(request_id);

        let request = match sub_type {
            SubscriptionType::Logs { mentions, commitment } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "logsSubscribe",
                "params": [
                    {
                        "mentions": mentions
                    },
                    {
                        "commitment": commitment
                    }
                ]
            }),
            SubscriptionType::Account { account, commitment, encoding } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "accountSubscribe",
                "params": [
                    account,
                    {
                        "encoding": encoding,
                        "commitment": commitment
                    }
                ]
            }),
            SubscriptionType::Signature { signature, commitment } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "signatureSubscribe",
                "params": [
                    signature,
                    {
                        "commitment": commitment
                    }
                ]
            }),
            SubscriptionType::Slot => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "slotSubscribe"
            }),
            SubscriptionType::Program { program_id, commitment, encoding } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "programSubscribe",
                "params": [
                    program_id,
                    {
                        "encoding": encoding,
                        "commitment": commitment
                    }
                ]
            }),
        };

        Ok(serde_json::to_string(&request)?)
    }

    /// Subscribe to logs for a specific program
    pub async fn subscribe_logs<H>(
        &self,
        program_id: &Pubkey,
        handler: H,
    ) -> Result<u64>
    where
        H: SubscriptionHandler + 'static,
    {
        let sub_type = SubscriptionType::Logs {
            mentions: vec![program_id.to_string()],
            commitment: "confirmed".to_string(),
        };

        let sub_id = self.add_subscription(sub_type, Arc::new(handler)).await;
        info!("Subscribed to logs for program: {} (subscription: {})", program_id, sub_id);
        Ok(sub_id)
    }

    /// Subscribe to account updates
    pub async fn subscribe_account<H>(
        &self,
        account: &Pubkey,
        handler: H,
    ) -> Result<u64>
    where
        H: SubscriptionHandler + 'static,
    {
        let sub_type = SubscriptionType::Account {
            account: account.to_string(),
            commitment: "confirmed".to_string(),
            encoding: "base64".to_string(),
        };

        let sub_id = self.add_subscription(sub_type, Arc::new(handler)).await;
        info!("Subscribed to account: {} (subscription: {})", account, sub_id);
        Ok(sub_id)
    }

    /// Subscribe to signature confirmations
    pub async fn subscribe_signature<H>(
        &self,
        signature: &Signature,
        handler: H,
    ) -> Result<u64>
    where
        H: SubscriptionHandler + 'static,
    {
        let sub_type = SubscriptionType::Signature {
            signature: signature.to_string(),
            commitment: "confirmed".to_string(),
        };

        let sub_id = self.add_subscription(sub_type, Arc::new(handler)).await;
        info!("Subscribed to signature: {} (subscription: {})", signature, sub_id);
        Ok(sub_id)
    }

    /// Subscribe to program account updates
    pub async fn subscribe_program<H>(
        &self,
        program_id: &Pubkey,
        handler: H,
    ) -> Result<u64>
    where
        H: SubscriptionHandler + 'static,
    {
        let sub_type = SubscriptionType::Program {
            program_id: program_id.to_string(),
            commitment: "confirmed".to_string(),
            encoding: "base64".to_string(),
        };

        let sub_id = self.add_subscription(sub_type, Arc::new(handler)).await;
        info!("Subscribed to program: {} (subscription: {})", program_id, sub_id);
        Ok(sub_id)
    }

    async fn add_subscription(
        &self,
        sub_type: SubscriptionType,
        handler: Arc<dyn SubscriptionHandler>,
    ) -> u64 {
        let sub_id = {
            let mut request_id = self.next_request_id.write().await;
            let id = *request_id;
            *request_id += 1;
            id
        };

        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.insert(sub_id, (sub_type, handler));
        
        sub_id
    }

    /// Unsubscribe from a subscription
    pub async fn unsubscribe(&self, subscription_id: u64) -> Result<()> {
        let mut subscriptions = self.subscriptions.write().await;
        if subscriptions.remove(&subscription_id).is_some() {
            info!("Unsubscribed: {}", subscription_id);
            Ok(())
        } else {
            Err(SolanaError::WebSocketError(format!(
                "Subscription not found: {}",
                subscription_id
            )))
        }
    }

    /// Get number of active subscriptions
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_manager_creation() {
        let manager = WebSocketManager::new("wss://api.mainnet-beta.solana.com".to_string());
        assert_eq!(manager.reconnect_attempts, 0);
    }
}
