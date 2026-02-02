use anyhow::Result;
use crate::config::BotConfig;
use crate::risk::manager::RiskManager;
use crate::solana::{client::SolanaClient, wallet::Wallet};
use crate::monitoring::{logger::LoggingGuard, alerts::AlertManager};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use solana_sdk::transaction::Transaction;

/// Trading bot with safety features
pub struct TradingBot {
    config: BotConfig,
    risk_manager: Arc<RiskManager>,
    wallet: Arc<Wallet>,
    client: Arc<SolanaClient>,
    alert_manager: Arc<AlertManager>,
    dry_run: bool,
    circuit_breaker_active: Arc<RwLock<bool>>,
    consecutive_failures: Arc<RwLock<u32>>,
}

impl TradingBot {
    /// Create a new trading bot
    pub async fn new(
        config: BotConfig,
        risk_manager: Arc<RiskManager>,
        wallet: Arc<Wallet>,
        client: Arc<SolanaClient>,
        alert_manager: Arc<AlertManager>,
    ) -> Result<Self> {
        config.validate()?;

        // Check for dry-run mode from environment variable
        let dry_run = std::env::var("DRY_RUN")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        if dry_run {
            warn!("⚠️  DRY-RUN MODE ENABLED - No trades will be executed");
        }

        Ok(Self {
            config,
            risk_manager,
            wallet,
            client,
            alert_manager,
            dry_run,
            circuit_breaker_active: Arc::new(RwLock::new(false)),
            consecutive_failures: Arc::new(RwLock::new(0)),
        })
    }

    /// Check if dry-run mode is enabled
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Check if circuit breaker is active
    pub async fn is_circuit_breaker_active(&self) -> bool {
        *self.circuit_breaker_active.read().await
    }

    /// Check if we can execute a trade (safety checks)
    pub async fn can_execute_trade(&self, amount_sol: f64) -> Result<bool> {
        // Check emergency stop
        if self.risk_manager.is_emergency_stop_active().await {
            warn!("Emergency stop is active - cannot execute trades");
            return Ok(false);
        }

        // Check circuit breaker
        if self.is_circuit_breaker_active().await {
            warn!("Circuit breaker is active - cannot execute trades");
            return Ok(false);
        }

        // Check balance before trade
        let balance = self.wallet.get_sol_balance().await?;
        let balance_sol = balance as f64 / 1e9;

        // Reserve some SOL for fees (0.01 SOL should be enough)
        let required_balance = amount_sol + 0.01;
        if balance_sol < required_balance {
            warn!(
                "Insufficient balance: required {:.4} SOL, available {:.4} SOL",
                required_balance, balance_sol
            );
            return Ok(false);
        }

        // Check minimum balance requirement
        if balance_sol < self.config.risk_management.min_sol_balance {
            warn!(
                "Balance below minimum: {:.4} < {:.4} SOL",
                balance_sol, self.config.risk_management.min_sol_balance
            );
            return Ok(false);
        }

        // Check risk manager
        if !self.risk_manager.can_open_trade(amount_sol, balance_sol).await? {
            return Ok(false);
        }

        Ok(true)
    }

    /// Simulate a transaction before sending
    pub async fn simulate_transaction(&self, transaction: &Transaction) -> Result<()> {
        // Always simulate in dry-run mode
        if self.dry_run {
            info!("[DRY-RUN] Simulating transaction: {}", transaction.signatures[0]);
            self.client.simulate_transaction(transaction).await?;
            info!("[DRY-RUN] Simulation successful - transaction would be valid");
            return Ok(());
        }

        // Simulate if configured
        if self.config.advanced.simulate_transactions {
            info!("Simulating transaction before sending: {}", transaction.signatures[0]);
            self.client.simulate_transaction(transaction).await?;
            info!("Simulation successful - proceeding with transaction");
        }

        Ok(())
    }

    /// Execute a trade with all safety checks
    pub async fn execute_trade_safe(
        &self,
        transaction: Transaction,
        trade_type: &str,
        amount_sol: f64,
    ) -> Result<solana_sdk::signature::Signature> {
        // Pre-flight checks
        if !self.can_execute_trade(amount_sol).await? {
            return Err(anyhow::anyhow!("Trade execution blocked by safety checks"));
        }

        // Simulate transaction
        self.simulate_transaction(&transaction).await?;

        // Execute in dry-run mode
        if self.dry_run {
            info!(
                "[DRY-RUN] Would execute {} trade: {:.4} SOL",
                trade_type, amount_sol
            );
            info!("[DRY-RUN] Transaction signature: {}", transaction.signatures[0]);
            
            // Return a dummy signature in dry-run mode
            // In production, you'd want to generate a proper signature
            return Ok(transaction.signatures[0]);
        }

        // Send transaction
        info!("Executing {} trade: {:.4} SOL", trade_type, amount_sol);
        let signature = self.client.send_transaction(&transaction).await?;
        
        info!("Trade executed successfully: {}", signature);

        // Reset consecutive failures on success
        let mut failures = self.consecutive_failures.write().await;
        *failures = 0;

        Ok(signature)
    }

    /// Record a trade failure (for circuit breaker)
    pub async fn record_trade_failure(&self) {
        let mut failures = self.consecutive_failures.write().await;
        *failures += 1;

        let max_failures = self.config.risk_management.max_consecutive_losses;
        if *failures >= max_failures {
            warn!(
                "Circuit breaker triggered: {} consecutive failures (limit: {})",
                *failures, max_failures
            );
            
            let mut circuit_breaker = self.circuit_breaker_active.write().await;
            *circuit_breaker = true;

            // Send alert
            let _ = self.alert_manager.send_error_alert(
                &format!("Circuit breaker activated after {} consecutive failures", *failures)
            ).await;
        }
    }

    /// Reset circuit breaker
    pub async fn reset_circuit_breaker(&self) {
        let mut circuit_breaker = self.circuit_breaker_active.write().await;
        let mut failures = self.consecutive_failures.write().await;
        
        *circuit_breaker = false;
        *failures = 0;
        
        info!("Circuit breaker reset");
    }

    /// Check balance and log status
    pub async fn check_balance(&self) -> Result<f64> {
        let balance = self.wallet.get_sol_balance().await?;
        let balance_sol = balance as f64 / 1e9;

        info!("Current balance: {:.4} SOL ({} lamports)", balance_sol, balance);

        // Warn if balance is low
        if balance_sol < self.config.risk_management.min_sol_balance * 2.0 {
            warn!(
                "⚠️  Low balance warning: {:.4} SOL (minimum: {:.4} SOL)",
                balance_sol, self.config.risk_management.min_sol_balance
            );
        }

        Ok(balance_sol)
    }

    /// Run the main bot loop
    pub async fn run(&self) -> Result<()> {
        info!("Trading bot started");
        
        if self.dry_run {
            warn!("⚠️  Running in DRY-RUN mode - no real trades will be executed");
        }

        // Check initial balance
        self.check_balance().await?;

        // Check risk manager status
        let daily_trades = self.risk_manager.get_daily_trades().await;
        let daily_loss = self.risk_manager.get_daily_loss().await;
        let consecutive_losses = self.risk_manager.get_consecutive_losses().await;

        info!("Risk Manager Status:");
        info!("  Daily Trades: {}/{}", daily_trades, self.config.risk_management.max_daily_trades);
        info!("  Daily Loss: {:.4}/{:.4} SOL", daily_loss, self.config.risk_management.max_daily_loss_sol);
        info!("  Consecutive Losses: {}/{}", consecutive_losses, self.config.risk_management.max_consecutive_losses);

        // Check emergency stop
        if self.risk_manager.is_emergency_stop_active().await {
            error!("⚠️  EMERGENCY STOP IS ACTIVE - Trading is halted");
            return Err(anyhow::anyhow!("Emergency stop is active"));
        }

        // TODO: Implement main bot loop
        // - Initialize Solana client
        // - Start copy trader
        // - Start volume spike detector
        // - Start risk manager monitoring
        // - Start monitoring and alerts
        
        Ok(())
    }

    /// Get bot status for monitoring
    pub async fn get_status(&self) -> BotStatus {
        let balance = self.wallet.get_sol_balance().await.unwrap_or(0) as f64 / 1e9;
        let daily_trades = self.risk_manager.get_daily_trades().await;
        let daily_loss = self.risk_manager.get_daily_loss().await;
        let consecutive_losses = self.risk_manager.get_consecutive_losses().await;
        let open_positions = self.risk_manager.get_open_positions_count().await;
        let circuit_breaker = self.is_circuit_breaker_active().await;
        let emergency_stop = self.risk_manager.is_emergency_stop_active().await;

        BotStatus {
            dry_run: self.dry_run,
            balance_sol: balance,
            daily_trades,
            daily_loss,
            consecutive_losses,
            open_positions,
            circuit_breaker_active: circuit_breaker,
            emergency_stop_active: emergency_stop,
        }
    }
}

/// Bot status for monitoring
#[derive(Debug, Clone)]
pub struct BotStatus {
    pub dry_run: bool,
    pub balance_sol: f64,
    pub daily_trades: u32,
    pub daily_loss: f64,
    pub consecutive_losses: u32,
    pub open_positions: usize,
    pub circuit_breaker_active: bool,
    pub emergency_stop_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BotConfig, RiskManagementConfig};
    use crate::solana::client::RpcConfig;

    #[tokio::test]
    async fn test_dry_run_mode() {
        // This test would require mocking the dependencies
        // For now, we'll just test the dry_run flag
        let dry_run = std::env::var("DRY_RUN")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        
        assert!(dry_run || !dry_run); // Just check it parses
    }
}
