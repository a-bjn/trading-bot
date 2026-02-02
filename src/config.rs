use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::path::Path;
use solana_sdk::pubkey::Pubkey;
use tracing::{warn, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub rpc: RpcConfig,
    pub copy_trading: CopyTradingConfig,
    pub volume_spike_scalping: VolumeSpikeScalpingConfig,
    pub trading: TradingConfig,
    pub risk_management: RiskManagementConfig,
    pub token_filters: TokenFiltersConfig,
    pub pumpfun: PumpFunConfig,
    pub monitoring: MonitoringConfig,
    pub telegram: TelegramConfig,
    pub performance: PerformanceConfig,
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    pub primary_rpc_url: String,
    pub backup_rpc_urls: Vec<String>,
    pub ws_url: String,
    pub helius_api_key: Option<String>,
    pub max_retries: u32,
    pub timeout_seconds: u64,
    pub rate_limit_per_second: u32,
    pub enable_connection_pooling: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyTradingConfig {
    pub enabled: bool,
    pub target_wallets: Vec<TargetWallet>,
    pub execution: CopyExecutionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetWallet {
    pub address: String,
    pub name: String,
    pub copy_percentage: u8,
    pub max_copy_amount_sol: f64,
    pub min_copy_amount_sol: f64,
    pub copy_sells: bool,
    pub copy_buys: bool,
    pub delay_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyExecutionConfig {
    pub fast_mode: bool,
    pub max_execution_time_seconds: u64,
    pub use_jito: bool,
    pub priority_fee_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSpikeScalpingConfig {
    pub enabled: bool,
    pub spike_threshold_percentage: f64,
    pub detection_window_seconds: u64,
    pub min_base_volume_sol: f64,
    pub min_liquidity_sol: f64,
    pub max_liquidity_sol: f64,
    pub min_holders: u32,
    pub max_market_cap_sol: f64,
    pub entry_amount_sol: f64,
    pub max_position_size_sol: f64,
    pub quick_profit_target_percentage: f64,
    pub quick_profit_time_limit_seconds: u64,
    pub stop_loss_percentage: f64,
    pub trailing_stop_percentage: f64,
    pub max_concurrent_scalp_trades: u32,
    pub cooldown_between_trades_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub default_trade_size_sol: f64,
    pub max_position_size_sol: f64,
    pub min_trade_size_sol: f64,
    pub slippage_bps: u16,
    pub priority_fee_lamports: u64,
    pub max_priority_fee_lamports: u64,
    pub compute_unit_limit: u32,
    pub max_retry_attempts: u32,
    pub transaction_timeout_seconds: u64,
    pub confirmation_commitment: String,
    pub max_concurrent_trades: u32,
    pub max_trades_per_token: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskManagementConfig {
    pub default_stop_loss_percentage: f64,
    pub default_take_profit_percentage: f64,
    pub use_trailing_stop: bool,
    pub trailing_stop_activation_percentage: f64,
    pub trailing_stop_distance_percentage: f64,
    pub max_position_percentage: f64,
    pub max_total_exposure_percentage: f64,
    pub max_daily_loss_sol: f64,
    pub max_daily_trades: u32,
    pub max_consecutive_losses: u32,
    pub min_sol_balance: f64,
    pub emergency_stop_loss_percentage: f64,
    pub panic_sell_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenFiltersConfig {
    pub min_liquidity_sol: f64,
    pub max_liquidity_sol: f64,
    pub min_market_cap_sol: f64,
    pub max_market_cap_sol: f64,
    pub min_holders: u32,
    pub max_holder_concentration_percentage: f64,
    pub min_token_age_minutes: u64,
    pub max_token_age_hours: u64,
    pub check_mint_authority: bool,
    pub check_freeze_authority: bool,
    pub require_immutable_metadata: bool,
    pub blacklisted_tokens: Vec<String>,
    pub blacklisted_creators: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunConfig {
    pub enabled: bool,
    pub program_id: String,
    pub monitor_new_tokens: bool,
    pub monitor_graduated_tokens: bool,
    pub min_initial_liquidity_sol: f64,
    pub max_bonding_curve_progress_percentage: f64,
    pub snipe_new_launches: bool,
    pub target_early_stage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub log_level: String,
    pub log_to_file: bool,
    pub log_rotation_size_mb: u64,
    pub max_log_files: u32,
    pub enable_metrics: bool,
    pub metrics_port: u16,
    pub enable_health_check: bool,
    pub health_check_port: u16,
    pub health_check_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub notify_on_trade: bool,
    pub notify_on_error: bool,
    pub notify_on_profit: bool,
    pub notify_on_loss: bool,
    pub notify_on_volume_spike: bool,
    pub notify_on_copy_trade: bool,
    pub min_profit_notification_sol: f64,
    pub min_loss_notification_sol: f64,
    pub max_messages_per_minute: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub max_rpc_connections: u32,
    pub rpc_timeout_seconds: u64,
    pub ws_reconnect_delay_seconds: u64,
    pub ws_ping_interval_seconds: u64,
    pub cache_token_metadata: bool,
    pub cache_ttl_seconds: u64,
    pub prefetch_token_accounts: bool,
    pub prefetch_program_accounts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    pub use_versioned_transactions: bool,
    pub use_address_lookup_tables: bool,
    pub dynamic_priority_fees: bool,
    pub priority_fee_percentile: u8,
    pub simulate_transactions: bool,
    pub skip_preflight: bool,
    pub use_private_rpc: bool,
    pub use_jito_bundle: bool,
}

impl BotConfig {
    /// Load configuration from YAML file and environment variables
    /// Environment variables take precedence over YAML values
    pub fn load() -> Result<Self> {
        // Load .env file if present
        dotenv::dotenv().ok();

        // Load from YAML file
        let config_path = std::env::var("CONFIG_PATH")
            .unwrap_or_else(|_| "config/bot_config.yaml".to_string());
        
        let mut config = if Path::new(&config_path).exists() {
            info!("Loading configuration from: {}", config_path);
            let config_str = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {}", config_path))?;
            serde_yaml::from_str::<BotConfig>(&config_str)
                .with_context(|| format!("Failed to parse YAML config: {}", config_path))?
        } else {
            warn!("Config file not found: {}, using defaults", config_path);
            Self::default()
        };

        // Override with environment variables
        config.apply_env_overrides();

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        // RPC Configuration
        if let Ok(url) = std::env::var("RPC_URL") {
            self.rpc.primary_rpc_url = url;
        }
        if let Ok(url) = std::env::var("WS_URL") {
            self.rpc.ws_url = url;
        }
        if let Ok(key) = std::env::var("HELIUS_API_KEY") {
            self.rpc.helius_api_key = Some(key);
        }
        if let Ok(val) = std::env::var("RPC_MAX_RETRIES") {
            if let Ok(retries) = val.parse() {
                self.rpc.max_retries = retries;
            }
        }
        if let Ok(val) = std::env::var("RPC_TIMEOUT_SECONDS") {
            if let Ok(timeout) = val.parse() {
                self.rpc.timeout_seconds = timeout;
            }
        }
        if let Ok(val) = std::env::var("RPC_RATE_LIMIT_PER_SECOND") {
            if let Ok(limit) = val.parse() {
                self.rpc.rate_limit_per_second = limit;
            }
        }

        // Copy Trading
        if let Ok(val) = std::env::var("COPY_TRADING_ENABLED") {
            if let Ok(enabled) = val.parse() {
                self.copy_trading.enabled = enabled;
            }
        }

        // Volume Spike Scalping
        if let Ok(val) = std::env::var("VOLUME_SPIKE_ENABLED") {
            if let Ok(enabled) = val.parse() {
                self.volume_spike_scalping.enabled = enabled;
            }
        }

        // Trading
        if let Ok(val) = std::env::var("DEFAULT_TRADE_SIZE_SOL") {
            if let Ok(size) = val.parse() {
                self.trading.default_trade_size_sol = size;
            }
        }
        if let Ok(val) = std::env::var("MAX_POSITION_SIZE_SOL") {
            if let Ok(size) = val.parse() {
                self.trading.max_position_size_sol = size;
            }
        }
        if let Ok(val) = std::env::var("SLIPPAGE_BPS") {
            if let Ok(bps) = val.parse() {
                self.trading.slippage_bps = bps;
            }
        }

        // Risk Management
        if let Ok(val) = std::env::var("MIN_SOL_BALANCE") {
            if let Ok(balance) = val.parse() {
                self.risk_management.min_sol_balance = balance;
            }
        }
        if let Ok(val) = std::env::var("MAX_DAILY_LOSS_SOL") {
            if let Ok(loss) = val.parse() {
                self.risk_management.max_daily_loss_sol = loss;
            }
        }

        // Monitoring
        if let Ok(level) = std::env::var("LOG_LEVEL") {
            self.monitoring.log_level = level;
        }

        // Telegram
        if let Ok(val) = std::env::var("TELEGRAM_ENABLED") {
            if let Ok(enabled) = val.parse() {
                self.telegram.enabled = enabled;
            }
        }
        if let Ok(val) = std::env::var("TELEGRAM_BOT_TOKEN") {
            // Store in a way that can be accessed later
            // For now, we'll add it to monitoring config or create a separate field
        }
    }

    /// Validate all configuration values
    pub fn validate(&self) -> Result<()> {
        // Validate RPC config
        if self.rpc.primary_rpc_url.is_empty() {
            return Err(anyhow::anyhow!("RPC URL cannot be empty"));
        }
        if self.rpc.max_retries == 0 {
            return Err(anyhow::anyhow!("RPC max_retries must be > 0"));
        }
        if self.rpc.rate_limit_per_second == 0 {
            return Err(anyhow::anyhow!("RPC rate_limit_per_second must be > 0"));
        }

        // Validate trading config
        if self.trading.min_trade_size_sol <= 0.0 {
            return Err(anyhow::anyhow!("min_trade_size_sol must be > 0"));
        }
        if self.trading.max_position_size_sol < self.trading.min_trade_size_sol {
            return Err(anyhow::anyhow!("max_position_size_sol must be >= min_trade_size_sol"));
        }
        if self.trading.slippage_bps > 10000 {
            return Err(anyhow::anyhow!("slippage_bps cannot exceed 10000 (100%)"));
        }
        if self.trading.priority_fee_lamports > self.trading.max_priority_fee_lamports {
            return Err(anyhow::anyhow!("priority_fee_lamports must be <= max_priority_fee_lamports"));
        }

        // Validate risk management
        if self.risk_management.min_sol_balance < 0.0 {
            return Err(anyhow::anyhow!("min_sol_balance cannot be negative"));
        }
        if self.risk_management.max_position_percentage > 100.0 {
            return Err(anyhow::anyhow!("max_position_percentage cannot exceed 100%"));
        }
        if self.risk_management.max_total_exposure_percentage > 100.0 {
            return Err(anyhow::anyhow!("max_total_exposure_percentage cannot exceed 100%"));
        }
        if self.risk_management.default_stop_loss_percentage <= 0.0 {
            return Err(anyhow::anyhow!("default_stop_loss_percentage must be > 0"));
        }

        // Validate copy trading
        if self.copy_trading.enabled {
            if self.copy_trading.target_wallets.is_empty() {
                return Err(anyhow::anyhow!("copy_trading.enabled is true but no target_wallets configured"));
            }
            for wallet in &self.copy_trading.target_wallets {
                if wallet.copy_percentage == 0 || wallet.copy_percentage > 100 {
                    return Err(anyhow::anyhow!("copy_percentage must be between 1 and 100"));
                }
                if wallet.min_copy_amount_sol > wallet.max_copy_amount_sol {
                    return Err(anyhow::anyhow!("min_copy_amount_sol must be <= max_copy_amount_sol"));
                }
                // Validate wallet address format
                if wallet.address.parse::<Pubkey>().is_err() {
                    return Err(anyhow::anyhow!("Invalid wallet address format: {}", wallet.address));
                }
            }
        }

        // Validate volume spike scalping
        if self.volume_spike_scalping.enabled {
            if self.volume_spike_scalping.spike_threshold_percentage <= 0.0 {
                return Err(anyhow::anyhow!("spike_threshold_percentage must be > 0"));
            }
            if self.volume_spike_scalping.min_liquidity_sol > self.volume_spike_scalping.max_liquidity_sol {
                return Err(anyhow::anyhow!("min_liquidity_sol must be <= max_liquidity_sol"));
            }
        }

        // Validate token filters
        if self.token_filters.min_liquidity_sol > self.token_filters.max_liquidity_sol {
            return Err(anyhow::anyhow!("token_filters.min_liquidity_sol must be <= max_liquidity_sol"));
        }
        if self.token_filters.min_market_cap_sol > self.token_filters.max_market_cap_sol {
            return Err(anyhow::anyhow!("token_filters.min_market_cap_sol must be <= max_market_cap_sol"));
        }

        Ok(())
    }

    /// Get RPC config for SolanaClient
    pub fn rpc_config(&self) -> crate::solana::client::RpcConfig {
        crate::solana::client::RpcConfig {
            primary_url: self.rpc.primary_rpc_url.clone(),
            backup_urls: self.rpc.backup_rpc_urls.clone(),
            helius_api_key: self.rpc.helius_api_key.clone(),
            max_retries: self.rpc.max_retries,
            timeout_seconds: self.rpc.timeout_seconds,
            rate_limit_per_second: self.rpc.rate_limit_per_second,
            enable_pooling: self.rpc.enable_connection_pooling,
        }
    }

    /// Check if copy trading is enabled
    pub fn is_copy_trading_enabled(&self) -> bool {
        self.copy_trading.enabled
    }

    /// Check if volume spike scalping is enabled
    pub fn is_volume_spike_enabled(&self) -> bool {
        self.volume_spike_scalping.enabled
    }

    /// Get minimum SOL balance required
    pub fn min_sol_balance(&self) -> f64 {
        self.risk_management.min_sol_balance
    }

    /// Get minimum SOL balance in lamports
    pub fn min_sol_balance_lamports(&self) -> u64 {
        (self.risk_management.min_sol_balance * 1e9) as u64
    }

    /// Get max position size in lamports
    pub fn max_position_size_lamports(&self) -> u64 {
        (self.trading.max_position_size_sol * 1e9) as u64
    }

    /// Get default trade size in lamports
    pub fn default_trade_size_lamports(&self) -> u64 {
        (self.trading.default_trade_size_sol * 1e9) as u64
    }
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            rpc: RpcConfig {
                primary_rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
                backup_rpc_urls: vec![],
                ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
                helius_api_key: None,
                max_retries: 3,
                timeout_seconds: 30,
                rate_limit_per_second: 10,
                enable_connection_pooling: true,
            },
            copy_trading: CopyTradingConfig {
                enabled: false,
                target_wallets: vec![],
                execution: CopyExecutionConfig {
                    fast_mode: true,
                    max_execution_time_seconds: 3,
                    use_jito: false,
                    priority_fee_multiplier: 1.5,
                },
            },
            volume_spike_scalping: VolumeSpikeScalpingConfig {
                enabled: false,
                spike_threshold_percentage: 300.0,
                detection_window_seconds: 60,
                min_base_volume_sol: 0.5,
                min_liquidity_sol: 5.0,
                max_liquidity_sol: 50.0,
                min_holders: 10,
                max_market_cap_sol: 100.0,
                entry_amount_sol: 0.1,
                max_position_size_sol: 0.3,
                quick_profit_target_percentage: 20.0,
                quick_profit_time_limit_seconds: 300,
                stop_loss_percentage: 15.0,
                trailing_stop_percentage: 10.0,
                max_concurrent_scalp_trades: 2,
                cooldown_between_trades_seconds: 30,
            },
            trading: TradingConfig {
                default_trade_size_sol: 0.1,
                max_position_size_sol: 0.5,
                min_trade_size_sol: 0.01,
                slippage_bps: 500,
                priority_fee_lamports: 100000,
                max_priority_fee_lamports: 500000,
                compute_unit_limit: 200000,
                max_retry_attempts: 2,
                transaction_timeout_seconds: 30,
                confirmation_commitment: "confirmed".to_string(),
                max_concurrent_trades: 3,
                max_trades_per_token: 1,
            },
            risk_management: RiskManagementConfig {
                default_stop_loss_percentage: 20.0,
                default_take_profit_percentage: 50.0,
                use_trailing_stop: true,
                trailing_stop_activation_percentage: 30.0,
                trailing_stop_distance_percentage: 15.0,
                max_position_percentage: 20.0,
                max_total_exposure_percentage: 60.0,
                max_daily_loss_sol: 1.0,
                max_daily_trades: 50,
                max_consecutive_losses: 5,
                min_sol_balance: 0.05,
                emergency_stop_loss_percentage: 30.0,
                panic_sell_enabled: true,
            },
            token_filters: TokenFiltersConfig {
                min_liquidity_sol: 2.0,
                max_liquidity_sol: 1000.0,
                min_market_cap_sol: 1.0,
                max_market_cap_sol: 500.0,
                min_holders: 5,
                max_holder_concentration_percentage: 30.0,
                min_token_age_minutes: 5,
                max_token_age_hours: 24,
                check_mint_authority: true,
                check_freeze_authority: true,
                require_immutable_metadata: false,
                blacklisted_tokens: vec![],
                blacklisted_creators: vec![],
            },
            pumpfun: PumpFunConfig {
                enabled: false,
                program_id: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P".to_string(),
                monitor_new_tokens: false,
                monitor_graduated_tokens: false,
                min_initial_liquidity_sol: 2.0,
                max_bonding_curve_progress_percentage: 80.0,
                snipe_new_launches: false,
                target_early_stage: true,
            },
            monitoring: MonitoringConfig {
                log_level: "info".to_string(),
                log_to_file: true,
                log_rotation_size_mb: 100,
                max_log_files: 10,
                enable_metrics: true,
                metrics_port: 9090,
                enable_health_check: true,
                health_check_port: 8080,
                health_check_interval_seconds: 60,
            },
            telegram: TelegramConfig {
                enabled: false,
                notify_on_trade: true,
                notify_on_error: true,
                notify_on_profit: true,
                notify_on_loss: true,
                notify_on_volume_spike: true,
                notify_on_copy_trade: true,
                min_profit_notification_sol: 0.01,
                min_loss_notification_sol: 0.01,
                max_messages_per_minute: 10,
            },
            performance: PerformanceConfig {
                max_rpc_connections: 10,
                rpc_timeout_seconds: 10,
                ws_reconnect_delay_seconds: 5,
                ws_ping_interval_seconds: 30,
                cache_token_metadata: true,
                cache_ttl_seconds: 300,
                prefetch_token_accounts: true,
                prefetch_program_accounts: false,
            },
            advanced: AdvancedConfig {
                use_versioned_transactions: true,
                use_address_lookup_tables: false,
                dynamic_priority_fees: true,
                priority_fee_percentile: 75,
                simulate_transactions: true,
                skip_preflight: false,
                use_private_rpc: false,
                use_jito_bundle: false,
            },
        }
    }
}
