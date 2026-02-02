use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionLimits {
    pub max_position_size: f64,
    pub max_total_exposure: f64,
    pub max_concurrent_trades: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyLimits {
    pub max_loss: f64,
    pub max_trades: u32,
    pub current_loss: f64,
    pub current_trades: u32,
}

impl DailyLimits {
    pub fn new(max_loss: f64, max_trades: u32) -> Self {
        Self {
            max_loss,
            max_trades,
            current_loss: 0.0,
            current_trades: 0,
        }
    }

    pub fn can_trade(&self) -> bool {
        self.current_trades < self.max_trades && self.current_loss < self.max_loss
    }

    pub fn reset(&mut self) {
        self.current_loss = 0.0;
        self.current_trades = 0;
    }
}
