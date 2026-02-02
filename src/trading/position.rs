use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub token_mint: String,
    pub entry_price: f64,
    pub amount: f64,
    pub entry_time: DateTime<Utc>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub highest_price: f64,
}

pub struct PositionManager {
    positions: Vec<Position>,
}

impl PositionManager {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
        }
    }

    pub fn add_position(&mut self, position: Position) {
        self.positions.push(position);
    }

    pub fn get_position(&self, token_mint: &str) -> Option<&Position> {
        self.positions.iter().find(|p| p.token_mint == token_mint)
    }

    pub fn remove_position(&mut self, token_mint: &str) {
        self.positions.retain(|p| p.token_mint != token_mint);
    }

    pub fn update_position_price(&mut self, token_mint: &str, current_price: f64) {
        if let Some(position) = self.positions.iter_mut().find(|p| p.token_mint == token_mint) {
            if current_price > position.highest_price {
                position.highest_price = current_price;
            }
        }
    }
}
