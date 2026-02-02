use anyhow::Result;
use crate::config::CopyTradingConfig;

pub struct CopyTradeStrategy {
    config: CopyTradingConfig,
}

impl CopyTradeStrategy {
    pub fn new(config: CopyTradingConfig) -> Self {
        Self { config }
    }

    pub async fn process_target_transaction(&self, signature: &str) -> Result<()> {
        // TODO: Implement transaction processing
        // - Parse transaction
        // - Determine if it's a trade
        // - Calculate copy amount
        // - Execute copy trade
        Ok(())
    }
}
