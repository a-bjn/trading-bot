pub mod decoder;
pub mod executor;
pub mod follower;
pub mod monitor;
pub mod signal;

pub use decoder::TransactionDecoder;
pub use executor::CopyTradeExecutor;
pub use follower::WalletFollower;
pub use monitor::WalletMonitor;
pub use signal::{FilterResult, SignalFilter, TradeAction, TradeSignal};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Configuration for copy trading system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyTraderConfig {
    /// Target wallet address to monitor
    pub target_wallet: String,
    
    /// RPC endpoint URL
    pub rpc_url: String,
    
    /// WebSocket endpoint URL
    pub ws_url: String,
    
    /// Copy percentage (0.0 to 2.0, where 1.0 = 100% same size)
    pub copy_percentage: f64,
    
    /// Minimum trade size to copy (in SOL)
    pub min_trade_size: f64,
    
    /// Maximum position size limit (in SOL)
    pub max_position_size: f64,
    
    /// Delay in milliseconds before executing copy trade (0 = instant)
    pub delay_ms: u64,
    
    /// Only copy buy trades
    pub copy_buys: bool,
    
    /// Only copy sell trades
    pub copy_sells: bool,
    
    /// Filter to only pump.fun tokens
    pub pump_fun_only: bool,
    
    /// Minimum confidence score to accept signals (0.0 to 1.0)
    pub min_confidence: f64,
    
    /// Enable duplicate transaction detection
    pub enable_duplicate_detection: bool,
    
    /// Maximum RPC retry attempts
    pub max_rpc_retries: u64,
    
    /// Rate limit for RPC calls (requests per second, 0 = no limit)
    pub rate_limit_rps: u32,
    
    /// Optional Helius API key for enhanced transactions
    pub helius_api_key: Option<String>,
    
    /// Copy trade execution timeout (seconds)
    pub execution_timeout_secs: u64,
    
    /// Use Jito for faster execution
    pub use_jito: bool,
    
    /// Priority fee multiplier
    pub priority_fee_multiplier: f64,
}

impl CopyTraderConfig {
    /// Create a new configuration with defaults
    pub fn new(target_wallet: String, rpc_url: String, ws_url: String) -> Self {
        Self {
            target_wallet,
            rpc_url,
            ws_url,
            copy_percentage: 1.0,
            min_trade_size: 0.1,
            max_position_size: 10.0,
            delay_ms: 0,
            copy_buys: true,
            copy_sells: true,
            pump_fun_only: true,
            min_confidence: 0.5,
            enable_duplicate_detection: true,
            max_rpc_retries: 3,
            rate_limit_rps: 10,
            helius_api_key: None,
            execution_timeout_secs: 30,
            use_jito: false,
            priority_fee_multiplier: 1.0,
        }
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.target_wallet.is_empty() {
            return Err(anyhow!("Target wallet address is required"));
        }
        
        // Validate wallet address format
        Pubkey::from_str(&self.target_wallet)
            .map_err(|_| anyhow!("Invalid target wallet address"))?;
        
        if self.rpc_url.is_empty() {
            return Err(anyhow!("RPC URL is required"));
        }
        
        if self.ws_url.is_empty() {
            return Err(anyhow!("WebSocket URL is required"));
        }
        
        if self.copy_percentage < 0.0 || self.copy_percentage > 2.0 {
            return Err(anyhow!("Copy percentage must be between 0.0 and 2.0"));
        }
        
        if self.min_trade_size < 0.0 {
            return Err(anyhow!("Minimum trade size cannot be negative"));
        }
        
        if self.max_position_size < self.min_trade_size {
            return Err(anyhow!(
                "Maximum position size must be greater than minimum trade size"
            ));
        }
        
        if self.min_confidence < 0.0 || self.min_confidence > 1.0 {
            return Err(anyhow!("Minimum confidence must be between 0.0 and 1.0"));
        }
        
        if !self.copy_buys && !self.copy_sells {
            return Err(anyhow!("At least one of copy_buys or copy_sells must be enabled"));
        }
        
        Ok(())
    }
    
    /// Get target wallet as Pubkey
    pub fn get_target_pubkey(&self) -> Result<Pubkey> {
        Pubkey::from_str(&self.target_wallet)
            .map_err(|e| anyhow!("Failed to parse target wallet: {}", e))
    }
    
    /// Check if we should copy this action
    pub fn should_copy_action(&self, action: TradeAction) -> bool {
        match action {
            TradeAction::Buy => self.copy_buys,
            TradeAction::Sell => self.copy_sells,
        }
    }
}

impl Default for CopyTraderConfig {
    fn default() -> Self {
        Self::new(
            "54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy".to_string(),
            "https://api.mainnet-beta.solana.com".to_string(),
            "wss://api.mainnet-beta.solana.com".to_string(),
        )
    }
}

/// Program IDs for DEX protocols
pub mod program_ids {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;
    
    /// DFlow program ID
    pub fn dflow() -> Pubkey {
        Pubkey::from_str("DFLOWxy1cXLiE2zFbvEcuJQ27SamqcXTqiLYXGGvBmf")
            .expect("Valid DFlow program ID")
    }
    
    /// OkxDex program ID
    pub fn okx_dex() -> Pubkey {
        Pubkey::from_str("OKXDEXt9gZxLEJSbvYxJ5VyyhWQ3iqWcJq7rJmQWqHf")
            .expect("Valid OkxDex program ID")
    }
    
    /// Pump.fun program ID
    pub fn pump_fun() -> Pubkey {
        Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
            .expect("Valid Pump.fun program ID")
    }
    
    /// Raydium AMM program ID
    pub fn raydium() -> Pubkey {
        Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8")
            .expect("Valid Raydium program ID")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_validation() {
        let config = CopyTraderConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_copy_percentage() {
        let mut config = CopyTraderConfig::default();
        config.copy_percentage = 3.0;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_invalid_min_confidence() {
        let mut config = CopyTraderConfig::default();
        config.min_confidence = 1.5;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_should_copy_action() {
        let mut config = CopyTraderConfig::default();
        config.copy_buys = true;
        config.copy_sells = false;
        
        assert!(config.should_copy_action(TradeAction::Buy));
        assert!(!config.should_copy_action(TradeAction::Sell));
    }
    
    #[test]
    fn test_get_target_pubkey() {
        let config = CopyTraderConfig::default();
        let pubkey = config.get_target_pubkey();
        assert!(pubkey.is_ok());
    }
    
    #[test]
    fn test_program_ids() {
        let dflow = program_ids::dflow();
        let okx = program_ids::okx_dex();
        let pump_fun = program_ids::pump_fun();
        
        assert_ne!(dflow, Pubkey::default());
        assert_ne!(okx, Pubkey::default());
        assert_ne!(pump_fun, Pubkey::default());
    }
}
