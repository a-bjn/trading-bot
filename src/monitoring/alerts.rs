use anyhow::{Result, Context};
use crate::config::TelegramConfig;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use tracing::{error, warn, info};

/// Alert manager for Telegram notifications
pub struct AlertManager {
    config: TelegramConfig,
    bot_token: Option<String>,
    chat_id: Option<String>,
    client: reqwest::Client,
    message_queue: Arc<Mutex<Vec<QueuedMessage>>>,
    last_message_time: Arc<Mutex<Option<DateTime<Utc>>>>,
    message_count: Arc<Mutex<u32>>,
}

#[derive(Debug, Clone)]
struct QueuedMessage {
    text: String,
    priority: MessagePriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MessagePriority {
    Low,
    Normal,
    High,
    Critical,
}

impl AlertManager {
    /// Create a new alert manager
    pub fn new(config: TelegramConfig) -> Result<Self> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
        let chat_id = std::env::var("TELEGRAM_CHAT_ID").ok();

        if config.enabled {
            if bot_token.is_none() || chat_id.is_none() {
                warn!("Telegram alerts enabled but TELEGRAM_BOT_TOKEN or TELEGRAM_CHAT_ID not set");
            }
        }

        Ok(Self {
            config,
            bot_token,
            chat_id,
            client: reqwest::Client::new(),
            message_queue: Arc::new(Mutex::new(Vec::new())),
            last_message_time: Arc::new(Mutex::new(None)),
            message_count: Arc::new(Mutex::new(0)),
        })
    }

    /// Check if Telegram is properly configured
    pub fn is_configured(&self) -> bool {
        self.config.enabled && self.bot_token.is_some() && self.chat_id.is_some()
    }

    /// Send a trade alert (entry/exit)
    pub async fn send_trade_alert(&self, message: &str) -> Result<()> {
        if !self.config.enabled || !self.config.notify_on_trade {
            return Ok(());
        }

        self.send_message(message, MessagePriority::Normal).await
    }

    /// Send an error alert
    pub async fn send_error_alert(&self, error: &str) -> Result<()> {
        if !self.config.enabled || !self.config.notify_on_error {
            return Ok(());
        }

        let formatted = format!("🚨 ERROR: {}", error);
        self.send_message(&formatted, MessagePriority::Critical).await
    }

    /// Send a profit milestone alert
    pub async fn send_profit_alert(&self, profit_sol: f64, message: &str) -> Result<()> {
        if !self.config.enabled || !self.config.notify_on_profit {
            return Ok(());
        }

        if profit_sol.abs() < self.config.min_profit_notification_sol {
            return Ok(());
        }

        let emoji = if profit_sol > 0.0 { "💰" } else { "📉" };
        let formatted = format!("{} {} Profit: {:.4} SOL\n{}", emoji, emoji, profit_sol, message);
        self.send_message(&formatted, MessagePriority::High).await
    }

    /// Send a loss alert
    pub async fn send_loss_alert(&self, loss_sol: f64, message: &str) -> Result<()> {
        if !self.config.enabled || !self.config.notify_on_loss {
            return Ok(());
        }

        if loss_sol.abs() < self.config.min_loss_notification_sol {
            return Ok(());
        }

        let formatted = format!("⚠️ Loss: {:.4} SOL\n{}", loss_sol, message);
        self.send_message(&formatted, MessagePriority::High).await
    }

    /// Send a volume spike alert
    pub async fn send_volume_spike_alert(&self, token_mint: &str, spike_percentage: f64) -> Result<()> {
        if !self.config.enabled || !self.config.notify_on_volume_spike {
            return Ok(());
        }

        let message = format!(
            "🚀 Volume Spike Detected!\nToken: {}\nSpike: {:.2}%",
            token_mint, spike_percentage
        );
        self.send_message(&message, MessagePriority::Normal).await
    }

    /// Send a copy trade alert
    pub async fn send_copy_trade_alert(&self, message: &str) -> Result<()> {
        if !self.config.enabled || !self.config.notify_on_copy_trade {
            return Ok(());
        }

        let formatted = format!("📋 Copy Trade: {}", message);
        self.send_message(&formatted, MessagePriority::Normal).await
    }

    /// Send a daily summary report
    pub async fn send_daily_summary(
        &self,
        total_trades: u32,
        successful_trades: u32,
        total_profit_loss: f64,
        win_rate: f64,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let emoji = if total_profit_loss > 0.0 { "📊" } else { "📉" };
        let message = format!(
            "{} Daily Summary\n\n\
            Total Trades: {}\n\
            Successful: {}\n\
            Win Rate: {:.2}%\n\
            P&L: {:.4} SOL",
            emoji, total_trades, successful_trades, win_rate, total_profit_loss
        );

        self.send_message(&message, MessagePriority::Normal).await
    }

    /// Internal method to send a message with rate limiting
    async fn send_message(&self, text: &str, priority: MessagePriority) -> Result<()> {
        if !self.is_configured() {
            // Log to console if Telegram is not configured
            info!("Telegram alert (not sent): {}", text);
            return Ok(());
        }

        // Check rate limiting
        if !self.check_rate_limit().await {
            // Queue message if rate limited (only for high priority)
            if priority == MessagePriority::Critical || priority == MessagePriority::High {
                let mut queue = self.message_queue.lock().await;
                queue.push(QueuedMessage {
                    text: text.to_string(),
                    priority,
                });
                warn!("Message queued due to rate limit: {}", text);
            }
            return Ok(());
        }

        // Send message via Telegram API
        self.send_telegram_message(text).await
    }

    /// Check rate limiting (max messages per minute)
    async fn check_rate_limit(&self) -> bool {
        let mut count = self.message_count.lock().await;
        let mut last_time = self.last_message_time.lock().await;

        let now = Utc::now();

        // Reset counter if a minute has passed
        if let Some(last) = *last_time {
            if (now - last).num_seconds() >= 60 {
                *count = 0;
            }
        }

        // Check if we've exceeded the limit
        if *count >= self.config.max_messages_per_minute {
            return false;
        }

        // Increment counter and update time
        *count += 1;
        *last_time = Some(now);

        true
    }

    /// Send message via Telegram Bot API
    async fn send_telegram_message(&self, text: &str) -> Result<()> {
        let bot_token = self.bot_token.as_ref()
            .context("Telegram bot token not set")?;
        let chat_id = self.chat_id.as_ref()
            .context("Telegram chat ID not set")?;

        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);

        let payload = json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_notification": false,
        });

        let response = self.client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .context("Failed to send Telegram message")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Telegram API error: {} - {}", status, body);
            return Err(anyhow::anyhow!("Telegram API returned error: {}", status));
        }

        Ok(())
    }

    /// Process queued messages (should be called periodically)
    pub async fn process_queue(&self) -> Result<()> {
        let mut queue = self.message_queue.lock().await;
        
        // Process up to 5 messages at a time
        let mut processed = 0;
        while processed < 5 && !queue.is_empty() {
            if self.check_rate_limit().await {
                let msg = queue.remove(0);
                if let Err(e) = self.send_telegram_message(&msg.text).await {
                    error!("Failed to send queued message: {}", e);
                    // Put it back at the end
                    queue.push(msg);
                    break;
                }
                processed += 1;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Format a trade entry alert
    pub fn format_trade_entry(token_mint: &str, amount_sol: f64, entry_price: f64) -> String {
        format!(
            "🟢 Trade Entry\n\
            Token: <code>{}</code>\n\
            Amount: {:.4} SOL\n\
            Entry Price: {:.8} SOL",
            token_mint, amount_sol, entry_price
        )
    }

    /// Format a trade exit alert
    pub fn format_trade_exit(
        token_mint: &str,
        amount_sol: f64,
        entry_price: f64,
        exit_price: f64,
        profit_loss_sol: f64,
        profit_loss_percentage: f64,
    ) -> String {
        let emoji = if profit_loss_sol > 0.0 { "🟢" } else { "🔴" };
        format!(
            "{} Trade Exit\n\
            Token: <code>{}</code>\n\
            Amount: {:.4} SOL\n\
            Entry: {:.8} SOL\n\
            Exit: {:.8} SOL\n\
            P&L: {:.4} SOL ({:.2}%)",
            emoji, token_mint, amount_sol, entry_price, exit_price, profit_loss_sol, profit_loss_percentage
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alert_manager_creation() {
        let config = TelegramConfig {
            enabled: false,
            notify_on_trade: true,
            notify_on_error: true,
            notify_on_profit: true,
            notify_on_loss: true,
            notify_on_volume_spike: true,
            notify_on_copy_trade: true,
            min_profit_notification_sol: 0.1,
            min_loss_notification_sol: 0.1,
            max_messages_per_minute: 10,
        };

        let manager = AlertManager::new(config).unwrap();
        assert!(!manager.is_configured());
    }

    #[test]
    fn test_format_trade_entry() {
        let msg = AlertManager::format_trade_entry("ABC123", 1.0, 0.001);
        assert!(msg.contains("Trade Entry"));
        assert!(msg.contains("ABC123"));
    }

    #[test]
    fn test_format_trade_exit() {
        let msg = AlertManager::format_trade_exit("ABC123", 1.0, 0.001, 0.002, 1.0, 100.0);
        assert!(msg.contains("Trade Exit"));
        assert!(msg.contains("ABC123"));
    }
}
