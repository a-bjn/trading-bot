use anyhow::Result;
use crate::config::RiskManagementConfig;
use crate::trading::Position;
use chrono::{DateTime, Utc, Duration};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{warn, error, info};

/// Risk manager for position tracking and safety controls
pub struct RiskManager {
    config: RiskManagementConfig,
    positions: Arc<RwLock<HashMap<String, Position>>>,
    daily_loss: Arc<RwLock<f64>>,
    daily_trades: Arc<RwLock<u32>>,
    consecutive_losses: Arc<RwLock<u32>>,
    total_exposure: Arc<RwLock<f64>>,
    last_reset_date: Arc<RwLock<Option<DateTime<Utc>>>>,
    emergency_stop: Arc<RwLock<bool>>,
}

#[derive(Debug, Clone)]
pub enum ExitReason {
    StopLoss,
    TakeProfit,
    TrailingStop,
    TimeBased,
    MaxDailyLoss,
    EmergencyExit,
    Manual,
}

#[derive(Debug, Clone)]
pub struct PositionCheckResult {
    pub should_exit: bool,
    pub reason: Option<ExitReason>,
    pub current_price: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_percentage: f64,
}

impl RiskManager {
    /// Create a new risk manager
    pub fn new(config: RiskManagementConfig) -> Self {
        Self {
            config,
            positions: Arc::new(RwLock::new(HashMap::new())),
            daily_loss: Arc::new(RwLock::new(0.0)),
            daily_trades: Arc::new(RwLock::new(0)),
            consecutive_losses: Arc::new(RwLock::new(0)),
            total_exposure: Arc::new(RwLock::new(0.0)),
            last_reset_date: Arc::new(RwLock::new(None)),
            emergency_stop: Arc::new(RwLock::new(false)),
        }
    }

    /// Check if we can open a new trade
    pub async fn can_open_trade(&self, amount_sol: f64, current_balance: f64) -> Result<bool> {
        // Check emergency stop
        if *self.emergency_stop.read().await {
            warn!("Emergency stop is active - cannot open new trades");
            return Ok(false);
        }

        // Reset daily stats if needed
        self.reset_daily_stats_if_needed().await?;

        // Check daily trade limit
        let daily_trades = *self.daily_trades.read().await;
        if daily_trades >= self.config.max_daily_trades {
            warn!("Daily trade limit reached: {}/{}", daily_trades, self.config.max_daily_trades);
            return Ok(false);
        }

        // Check daily loss limit
        let daily_loss = *self.daily_loss.read().await;
        if daily_loss >= self.config.max_daily_loss_sol {
            warn!("Daily loss limit reached: {:.4}/{:.4} SOL", daily_loss, self.config.max_daily_loss_sol);
            return Ok(false);
        }

        // Check consecutive losses (circuit breaker)
        let consecutive_losses = *self.consecutive_losses.read().await;
        if consecutive_losses >= self.config.max_consecutive_losses {
            warn!("Circuit breaker triggered: {} consecutive losses", consecutive_losses);
            return Ok(false);
        }

        // Check minimum balance
        if current_balance < self.config.min_sol_balance {
            warn!("Insufficient balance: {:.4} < {:.4} SOL", current_balance, self.config.min_sol_balance);
            return Ok(false);
        }

        // Check position size limit
        if amount_sol > self.config.max_position_percentage * current_balance / 100.0 {
            warn!("Position size exceeds limit: {:.4} SOL", amount_sol);
            return Ok(false);
        }

        // Check total exposure limit
        let total_exposure = *self.total_exposure.read().await;
        let max_exposure = self.config.max_total_exposure_percentage * current_balance / 100.0;
        if total_exposure + amount_sol > max_exposure {
            warn!("Total exposure limit would be exceeded: {:.4} + {:.4} > {:.4} SOL", 
                  total_exposure, amount_sol, max_exposure);
            return Ok(false);
        }

        Ok(true)
    }

    /// Add a new position
    pub async fn add_position(&self, position: Position) -> Result<()> {
        let mut positions = self.positions.write().await;
        let mut total_exposure = self.total_exposure.write().await;
        
        // Calculate stop loss and take profit if not set
        let mut pos = position.clone();
        
        if pos.stop_loss.is_none() {
            pos.stop_loss = Some(pos.entry_price * (1.0 - self.config.default_stop_loss_percentage / 100.0));
        }
        
        if pos.take_profit.is_none() {
            pos.take_profit = Some(pos.entry_price * (1.0 + self.config.default_take_profit_percentage / 100.0));
        }

        *total_exposure += pos.amount;
        positions.insert(pos.token_mint.clone(), pos);

        let mut daily_trades = self.daily_trades.write().await;
        *daily_trades += 1;

        info!("Position added: {} - {:.4} SOL at {:.8}", 
              position.token_mint, position.amount, position.entry_price);

        Ok(())
    }

    /// Get a position by token mint
    pub async fn get_position(&self, token_mint: &str) -> Option<Position> {
        let positions = self.positions.read().await;
        positions.get(token_mint).cloned()
    }

    /// Get all positions
    pub async fn get_all_positions(&self) -> Vec<Position> {
        let positions = self.positions.read().await;
        positions.values().cloned().collect()
    }

    /// Check if a position should be closed
    pub async fn should_close_position(&self, token_mint: &str, current_price: f64) -> PositionCheckResult {
        let positions = self.positions.read().await;
        
        let position = match positions.get(token_mint) {
            Some(p) => p.clone(),
            None => {
                return PositionCheckResult {
                    should_exit: false,
                    reason: None,
                    current_price,
                    unrealized_pnl: 0.0,
                    unrealized_pnl_percentage: 0.0,
                };
            }
        };

        drop(positions);

        let unrealized_pnl = (current_price - position.entry_price) * position.amount;
        let unrealized_pnl_percentage = ((current_price - position.entry_price) / position.entry_price) * 100.0;

        // Check emergency stop loss
        if let Some(emergency_stop_price) = self.get_emergency_stop_price(&position) {
            if current_price <= emergency_stop_price {
                return PositionCheckResult {
                    should_exit: true,
                    reason: Some(ExitReason::EmergencyExit),
                    current_price,
                    unrealized_pnl,
                    unrealized_pnl_percentage,
                };
            }
        }

        // Check stop loss
        if let Some(stop_loss) = position.stop_loss {
            if current_price <= stop_loss {
                return PositionCheckResult {
                    should_exit: true,
                    reason: Some(ExitReason::StopLoss),
                    current_price,
                    unrealized_pnl,
                    unrealized_pnl_percentage,
                };
            }
        }

        // Check take profit
        if let Some(take_profit) = position.take_profit {
            if current_price >= take_profit {
                return PositionCheckResult {
                    should_exit: true,
                    reason: Some(ExitReason::TakeProfit),
                    current_price,
                    unrealized_pnl,
                    unrealized_pnl_percentage,
                };
            }
        }

        // Check trailing stop
        if self.config.use_trailing_stop {
            let profit_percentage = ((current_price - position.entry_price) / position.entry_price) * 100.0;
            
            if profit_percentage >= self.config.trailing_stop_activation_percentage {
                let trailing_stop_price = position.highest_price * 
                    (1.0 - self.config.trailing_stop_distance_percentage / 100.0);
                
                if current_price <= trailing_stop_price {
                    return PositionCheckResult {
                        should_exit: true,
                        reason: Some(ExitReason::TrailingStop),
                        current_price,
                        unrealized_pnl,
                        unrealized_pnl_percentage,
                    };
                }
            }
        }

        // Check time-based exit (if position is held too long)
        // This would need to be configured separately, but we'll add a basic check
        let position_age = Utc::now() - position.entry_time;
        // Default max hold time: 24 hours (can be configured)
        let max_hold_duration = Duration::hours(24);
        
        if position_age > max_hold_duration {
            return PositionCheckResult {
                should_exit: true,
                reason: Some(ExitReason::TimeBased),
                current_price,
                unrealized_pnl,
                unrealized_pnl_percentage,
            };
        }

        // Update highest price for trailing stop
        if current_price > position.highest_price {
            let mut positions = self.positions.write().await;
            if let Some(pos) = positions.get_mut(token_mint) {
                pos.highest_price = current_price;
            }
        }

        PositionCheckResult {
            should_exit: false,
            reason: None,
            current_price,
            unrealized_pnl,
            unrealized_pnl_percentage,
        }
    }

    /// Remove a position (after closing)
    pub async fn remove_position(&self, token_mint: &str, profit_loss: f64) -> Result<()> {
        let mut positions = self.positions.write().await;
        let mut total_exposure = self.total_exposure.write().await;
        let mut daily_loss = self.daily_loss.write().await;
        let mut consecutive_losses = self.consecutive_losses.write().await;

        if let Some(position) = positions.remove(token_mint) {
            *total_exposure -= position.amount;

            // Record trade result
            if profit_loss < 0.0 {
                *daily_loss += profit_loss.abs();
                *consecutive_losses += 1;
            } else {
                *consecutive_losses = 0;
            }

            info!("Position removed: {} - P&L: {:.4} SOL", token_mint, profit_loss);
        }

        Ok(())
    }

    /// Update position price (for monitoring)
    pub async fn update_position_price(&self, token_mint: &str, current_price: f64) {
        let mut positions = self.positions.write().await;
        if let Some(position) = positions.get_mut(token_mint) {
            if current_price > position.highest_price {
                position.highest_price = current_price;
            }
        }
    }

    /// Record trade result
    pub async fn record_trade_result(&self, profit_loss: f64) {
        let mut daily_loss = self.daily_loss.write().await;
        let mut consecutive_losses = self.consecutive_losses.write().await;

        if profit_loss < 0.0 {
            *daily_loss += profit_loss.abs();
            *consecutive_losses += 1;
        } else {
            *consecutive_losses = 0;
        }
    }

    /// Reset daily statistics
    pub async fn reset_daily_stats(&self) {
        let mut daily_loss = self.daily_loss.write().await;
        let mut daily_trades = self.daily_trades.write().await;
        let mut consecutive_losses = self.consecutive_losses.write().await;
        let mut last_reset = self.last_reset_date.write().await;

        *daily_loss = 0.0;
        *daily_trades = 0;
        *consecutive_losses = 0;
        *last_reset = Some(Utc::now());

        info!("Daily statistics reset");
    }

    /// Reset daily stats if a new day has started
    async fn reset_daily_stats_if_needed(&self) -> Result<()> {
        let mut last_reset = self.last_reset_date.write().await;
        
        let now = Utc::now();
        let should_reset = match *last_reset {
            Some(last) => {
                // Reset if it's a new day
                now.date_naive() != last.date_naive()
            }
            None => true, // First run
        };

        if should_reset {
            drop(last_reset);
            self.reset_daily_stats().await;
        }

        Ok(())
    }

    /// Get emergency stop price for a position
    fn get_emergency_stop_price(&self, position: &Position) -> Option<f64> {
        if self.config.panic_sell_enabled {
            Some(position.entry_price * (1.0 - self.config.emergency_stop_loss_percentage / 100.0))
        } else {
            None
        }
    }

    /// Enable emergency stop (stop all trading)
    pub async fn enable_emergency_stop(&self) {
        let mut emergency_stop = self.emergency_stop.write().await;
        *emergency_stop = true;
        error!("EMERGENCY STOP ENABLED - All trading halted");
    }

    /// Disable emergency stop
    pub async fn disable_emergency_stop(&self) {
        let mut emergency_stop = self.emergency_stop.write().await;
        *emergency_stop = false;
        info!("Emergency stop disabled");
    }

    /// Check if emergency stop is active
    pub async fn is_emergency_stop_active(&self) -> bool {
        *self.emergency_stop.read().await
    }

    /// Get current daily loss
    pub async fn get_daily_loss(&self) -> f64 {
        *self.daily_loss.read().await
    }

    /// Get current daily trades count
    pub async fn get_daily_trades(&self) -> u32 {
        *self.daily_trades.read().await
    }

    /// Get consecutive losses count
    pub async fn get_consecutive_losses(&self) -> u32 {
        *self.consecutive_losses.read().await
    }

    /// Get total exposure
    pub async fn get_total_exposure(&self) -> f64 {
        *self.total_exposure.read().await
    }

    /// Get number of open positions
    pub async fn get_open_positions_count(&self) -> usize {
        self.positions.read().await.len()
    }

    /// Force close all positions (emergency exit)
    pub async fn force_close_all_positions(&self) -> Vec<String> {
        let positions = self.positions.read().await;
        let token_mints: Vec<String> = positions.keys().cloned().collect();
        drop(positions);

        info!("Force closing {} positions", token_mints.len());
        token_mints
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_can_open_trade() {
        let config = RiskManagementConfig {
            default_stop_loss_percentage: 10.0,
            default_take_profit_percentage: 20.0,
            use_trailing_stop: false,
            trailing_stop_activation_percentage: 5.0,
            trailing_stop_distance_percentage: 2.0,
            max_position_percentage: 50.0,
            max_total_exposure_percentage: 100.0,
            max_daily_loss_sol: 1.0,
            max_daily_trades: 10,
            max_consecutive_losses: 3,
            min_sol_balance: 0.1,
            emergency_stop_loss_percentage: 50.0,
            panic_sell_enabled: false,
        };

        let manager = RiskManager::new(config);
        let can_trade = manager.can_open_trade(0.5, 10.0).await.unwrap();
        assert!(can_trade);
    }

    #[tokio::test]
    async fn test_daily_limit() {
        let config = RiskManagementConfig {
            default_stop_loss_percentage: 10.0,
            default_take_profit_percentage: 20.0,
            use_trailing_stop: false,
            trailing_stop_activation_percentage: 5.0,
            trailing_stop_distance_percentage: 2.0,
            max_position_percentage: 50.0,
            max_total_exposure_percentage: 100.0,
            max_daily_loss_sol: 1.0,
            max_daily_trades: 2,
            max_consecutive_losses: 3,
            min_sol_balance: 0.1,
            emergency_stop_loss_percentage: 50.0,
            panic_sell_enabled: false,
        };

        let manager = RiskManager::new(config);
        
        // Add two trades
        let position1 = Position {
            token_mint: "test1".to_string(),
            entry_price: 0.001,
            amount: 1.0,
            entry_time: Utc::now(),
            stop_loss: None,
            take_profit: None,
            highest_price: 0.001,
        };
        manager.add_position(position1).await.unwrap();
        
        let position2 = Position {
            token_mint: "test2".to_string(),
            entry_price: 0.001,
            amount: 1.0,
            entry_time: Utc::now(),
            stop_loss: None,
            take_profit: None,
            highest_price: 0.001,
        };
        manager.add_position(position2).await.unwrap();

        // Should not be able to open another trade
        let can_trade = manager.can_open_trade(0.5, 10.0).await.unwrap();
        assert!(!can_trade);
    }

    #[tokio::test]
    async fn test_stop_loss() {
        let config = RiskManagementConfig {
            default_stop_loss_percentage: 10.0,
            default_take_profit_percentage: 20.0,
            use_trailing_stop: false,
            trailing_stop_activation_percentage: 5.0,
            trailing_stop_distance_percentage: 2.0,
            max_position_percentage: 50.0,
            max_total_exposure_percentage: 100.0,
            max_daily_loss_sol: 1.0,
            max_daily_trades: 10,
            max_consecutive_losses: 3,
            min_sol_balance: 0.1,
            emergency_stop_loss_percentage: 50.0,
            panic_sell_enabled: false,
        };

        let manager = RiskManager::new(config);
        
        let position = Position {
            token_mint: "test".to_string(),
            entry_price: 1.0,
            amount: 1.0,
            entry_time: Utc::now(),
            stop_loss: Some(0.9), // 10% stop loss
            take_profit: None,
            highest_price: 1.0,
        };
        manager.add_position(position).await.unwrap();

        // Price below stop loss
        let result = manager.should_close_position("test", 0.85).await;
        assert!(result.should_exit);
        assert_eq!(result.reason, Some(ExitReason::StopLoss));
    }
}
