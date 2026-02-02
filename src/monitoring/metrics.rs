use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingMetrics {
    pub total_trades: u64,
    pub successful_trades: u64,
    pub failed_trades: u64,
    pub total_profit_loss: f64,
    pub win_rate: f64,
    pub average_execution_time_ms: u64,
}

pub struct MetricsCollector {
    metrics: TradingMetrics,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: TradingMetrics {
                total_trades: 0,
                successful_trades: 0,
                failed_trades: 0,
                total_profit_loss: 0.0,
                win_rate: 0.0,
                average_execution_time_ms: 0,
            },
        }
    }

    pub fn record_trade(&mut self, success: bool, profit_loss: f64, execution_time_ms: u64) {
        self.metrics.total_trades += 1;
        
        if success {
            self.metrics.successful_trades += 1;
            self.metrics.total_profit_loss += profit_loss;
        } else {
            self.metrics.failed_trades += 1;
        }

        self.metrics.win_rate = (self.metrics.successful_trades as f64 / self.metrics.total_trades as f64) * 100.0;
        
        // Update average execution time
        let total_time = self.metrics.average_execution_time_ms * (self.metrics.total_trades - 1);
        self.metrics.average_execution_time_ms = (total_time + execution_time_ms) / self.metrics.total_trades;
    }

    pub fn get_metrics(&self) -> &TradingMetrics {
        &self.metrics
    }
}
