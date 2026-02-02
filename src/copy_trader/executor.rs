use anyhow::{Context, Result};
use solana_sdk::signature::Signature;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::{CopyTradingConfig, CopyExecutionConfig};
use crate::copy_trader::signal::{TradeAction, TradeSignal};
use crate::solana::{
    client::SolanaClient,
    wallet::Wallet,
};
use crate::trading::executor::TradeExecutor;

/// Copy trade executor that executes trades based on signals from target wallets
pub struct CopyTradeExecutor {
    config: CopyTradingConfig,
    trade_executor: Arc<TradeExecutor>,
    wallet: Arc<Wallet>,
}

impl CopyTradeExecutor {
    /// Create a new copy trade executor
    pub fn new(
        config: CopyTradingConfig,
        trade_executor: Arc<TradeExecutor>,
        wallet: Arc<Wallet>,
    ) -> Self {
        Self {
            config,
            trade_executor,
            wallet,
        }
    }

    /// Execute a copy trade from a trade signal
    /// 
    /// # Arguments
    /// * `signal` - The trade signal to copy
    /// 
    /// # Returns
    /// * Transaction signature if successful
    pub async fn execute_copy_trade(&self, signal: &TradeSignal) -> Result<Option<Signature>> {
        // Find matching wallet config
        let wallet_config = self.config.target_wallets.iter()
            .find(|w| w.address == signal.wallet.to_string());

        let wallet_config = match wallet_config {
            Some(config) => config,
            None => {
                debug!("No wallet config found for wallet: {}", signal.wallet);
                return Ok(None);
            }
        };

        // Check if we should copy this action
        let should_copy = match signal.action {
            TradeAction::Buy => wallet_config.copy_buys,
            TradeAction::Sell => wallet_config.copy_sells,
        };

        if !should_copy {
            debug!("Action {:?} not enabled for wallet {}", signal.action, signal.wallet);
            return Ok(None);
        }

        // Calculate copy amount
        let copy_amount = self.calculate_copy_amount(signal.sol_amount, &wallet_config.address);

        if copy_amount == 0.0 {
            warn!("Copy amount is 0 for wallet {}", wallet_config.address);
            return Ok(None);
        }

        info!(
            "Executing copy trade: {:?} {} SOL for token {} (original: {} SOL, {}%)",
            signal.action,
            copy_amount,
            signal.token_mint,
            signal.sol_amount,
            wallet_config.copy_percentage
        );

        // Apply delay if configured
        if wallet_config.delay_ms > 0 {
            debug!("Applying delay of {}ms", wallet_config.delay_ms);
            sleep(Duration::from_millis(wallet_config.delay_ms)).await;
        }

        // Track execution time
        let start_time = Instant::now();

        // Execute the trade
        let result = match signal.action {
            TradeAction::Buy => {
                self.trade_executor
                    .execute_buy_pubkey(&signal.token_mint, copy_amount)
                    .await
            }
            TradeAction::Sell => {
                // For sells, we need to calculate token amount
                // If we have token_amount in signal, use it proportionally
                let token_amount = if let Some(original_tokens) = signal.token_amount {
                    // Calculate proportional token amount
                    let ratio = copy_amount / signal.sol_amount;
                    (original_tokens as f64 * ratio) / 1_000_000.0 // Convert from base units to tokens
                } else {
                    // Fallback: estimate based on SOL amount (rough approximation)
                    // This is not ideal, but we don't have token amount
                    warn!("Token amount not available in signal, using SOL amount as estimate");
                    copy_amount * 1000.0 // Rough estimate: assume 1000 tokens per SOL
                };

                self.trade_executor
                    .execute_sell_pubkey(&signal.token_mint, token_amount)
                    .await
            }
        };

        let execution_time = start_time.elapsed();

        match result {
            Ok(signature) => {
                info!(
                    "Copy trade executed successfully: {} in {:?}",
                    signature, execution_time
                );
                Ok(Some(signature))
            }
            Err(e) => {
                error!(
                    "Copy trade execution failed after {:?}: {}",
                    execution_time, e
                );
                Err(e)
            }
        }
    }

    /// Execute copy trade with explicit parameters (legacy method for compatibility)
    pub async fn execute_copy_trade_legacy(
        &self,
        token_mint: &str,
        is_buy: bool,
        original_amount: f64,
        target_wallet: &str,
    ) -> Result<Option<Signature>> {
        // Find wallet config
        let wallet_config = self.config.target_wallets.iter()
            .find(|w| w.address == target_wallet);

        let wallet_config = match wallet_config {
            Some(config) => config,
            None => {
                debug!("No wallet config found for wallet: {}", target_wallet);
                return Ok(None);
            }
        };

        // Check if we should copy this action
        let should_copy = if is_buy {
            wallet_config.copy_buys
        } else {
            wallet_config.copy_sells
        };

        if !should_copy {
            debug!("Action {} not enabled for wallet {}", is_buy, target_wallet);
            return Ok(None);
        }

        // Calculate copy amount
        let copy_amount = self.calculate_copy_amount(original_amount, target_wallet);

        if copy_amount == 0.0 {
            warn!("Copy amount is 0 for wallet {}", target_wallet);
            return Ok(None);
        }

        // Apply delay if configured
        if wallet_config.delay_ms > 0 {
            debug!("Applying delay of {}ms", wallet_config.delay_ms);
            sleep(Duration::from_millis(wallet_config.delay_ms)).await;
        }

        // Track execution time
        let start_time = Instant::now();

        // Execute the trade
        let result = if is_buy {
            self.trade_executor.execute_buy(token_mint, copy_amount).await
        } else {
            // For sells, estimate token amount (not ideal without token amount)
            let estimated_tokens = copy_amount * 1000.0; // Rough estimate
            self.trade_executor.execute_sell(token_mint, estimated_tokens).await
        };

        let execution_time = start_time.elapsed();

        match result {
            Ok(signature) => {
                info!(
                    "Copy trade executed successfully: {} in {:?}",
                    signature, execution_time
                );
                Ok(Some(signature))
            }
            Err(e) => {
                error!(
                    "Copy trade execution failed after {:?}: {}",
                    execution_time, e
                );
                Err(e)
            }
        }
    }

    /// Calculate copy amount based on config
    fn calculate_copy_amount(&self, original_amount: f64, target_wallet: &str) -> f64 {
        // Find wallet config
        for wallet in &self.config.target_wallets {
            if wallet.address == target_wallet {
                let copy_amount = original_amount * (wallet.copy_percentage as f64 / 100.0);
                return copy_amount.clamp(wallet.min_copy_amount_sol, wallet.max_copy_amount_sol);
            }
        }
        0.0
    }

    /// Get execution config
    pub fn execution_config(&self) -> &CopyExecutionConfig {
        &self.config.execution
    }

    /// Check if fast mode is enabled
    pub fn is_fast_mode(&self) -> bool {
        self.config.execution.fast_mode
    }

    /// Check if Jito is enabled
    pub fn use_jito(&self) -> bool {
        self.config.execution.use_jito
    }
}
