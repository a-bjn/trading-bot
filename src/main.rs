use anyhow::{Result, Context};
use solana_trading_bot::config::BotConfig;
use solana_trading_bot::solana::{Wallet, SolanaClient};
use std::sync::Arc;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging first
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    info!("Starting Solana Copy Trading Bot");

    // Load configuration
    let config = BotConfig::load()
        .context("Failed to load configuration")?;
    
    info!("Configuration loaded successfully");
    info!("Copy Trading: {}", if config.is_copy_trading_enabled() { "ENABLED" } else { "DISABLED" });
    info!("Volume Spike Scalping: {}", if config.is_volume_spike_enabled() { "ENABLED" } else { "DISABLED" });
    info!("RPC URL: {}", config.rpc.primary_rpc_url);

    // Create Solana client
    let rpc_config = config.rpc_config();
    let client = Arc::new(SolanaClient::new(rpc_config));
    info!("Solana client initialized");

    // Load wallet
    let wallet = Wallet::load(client.clone())
        .context("Failed to load wallet. Set WALLET_PRIVATE_KEY or WALLET_KEYFILE")?;
    
    info!("Wallet loaded: {}", wallet.pubkey());

    // Verify wallet has sufficient balance
    let min_balance_lamports = config.min_sol_balance_lamports();
    let balance = wallet.get_sol_balance().await
        .context("Failed to check wallet balance")?;
    
    let balance_sol = balance as f64 / 1e9;
    info!("Wallet balance: {:.4} SOL ({} lamports)", balance_sol, balance);

    if balance < min_balance_lamports {
        return Err(anyhow::anyhow!(
            "Insufficient balance: required {:.4} SOL, available {:.4} SOL",
            config.min_sol_balance(),
            balance_sol
        ));
    }

    // Safety check: Warn if balance is low
    if balance < min_balance_lamports * 2 {
        warn!("Warning: Wallet balance is low. Consider adding more SOL for trading.");
    }

    // Create shutdown signal channel
    let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);
    let shutdown_rx2 = shutdown_tx.subscribe();

    // Setup graceful shutdown handler
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Shutdown signal received (Ctrl+C)");
                let _ = shutdown_tx_clone.send(());
            }
            Err(err) => {
                error!("Failed to listen for shutdown signal: {}", err);
            }
        }
    });

    // Create shared components
    let wallet_arc = Arc::new(wallet);
    let risk_manager = Arc::new(
        solana_trading_bot::risk::manager::RiskManager::new(
            config.risk_management.clone(),
        )
    );
    
    let jito_client = None; // TODO: Initialize if config.advanced.use_jito_bundle
    let trade_executor = Arc::new(
        solana_trading_bot::trading::executor::TradeExecutor::new(
            wallet_arc.clone(),
            client.clone(),
            jito_client,
            config.trading.clone(),
        )
    );

    let mut tasks = vec![];

    // Initialize copy trading system
    if config.is_copy_trading_enabled() {
        info!("Initializing copy trading system...");
        if config.copy_trading.target_wallets.is_empty() {
            warn!("Copy trading enabled but no target wallets configured");
        } else {
            info!("Monitoring {} target wallet(s)", config.copy_trading.target_wallets.len());
            
            // Start copy trading for each target wallet
            for wallet_config in &config.copy_trading.target_wallets {
                info!("  - {} ({}) - Copy {}%", wallet_config.name, wallet_config.address, wallet_config.copy_percentage);
                
                // Convert to CopyTraderConfig
                let copy_config = solana_trading_bot::copy_trader::CopyTraderConfig {
                    target_wallet: wallet_config.address.clone(),
                    rpc_url: config.rpc.primary_rpc_url.clone(),
                    ws_url: config.rpc.ws_url.clone(),
                    copy_percentage: wallet_config.copy_percentage as f64 / 100.0,
                    min_trade_size: wallet_config.min_copy_amount_sol,
                    max_position_size: wallet_config.max_copy_amount_sol,
                    delay_ms: wallet_config.delay_ms,
                    copy_buys: wallet_config.copy_buys,
                    copy_sells: wallet_config.copy_sells,
                    pump_fun_only: true,
                    min_confidence: 0.5,
                    enable_duplicate_detection: true,
                    max_rpc_retries: config.rpc.max_retries as u64,
                    rate_limit_rps: config.rpc.rate_limit_per_second,
                    helius_api_key: config.rpc.helius_api_key.clone(),
                    execution_timeout_secs: config.copy_trading.execution.max_execution_time_seconds,
                    use_jito: config.copy_trading.execution.use_jito,
                    priority_fee_multiplier: config.copy_trading.execution.priority_fee_multiplier,
                };
                
                let (signal_tx, mut signal_rx) = tokio::sync::mpsc::unbounded_channel();
                
                // Create follower
                let follower = solana_trading_bot::copy_trader::WalletFollower::new(copy_config.clone())?;
                
                // Create copy executor
                let copy_executor = Arc::new(
                    solana_trading_bot::copy_trader::CopyTradeExecutor::new(
                        config.copy_trading.clone(),
                        trade_executor.clone(),
                        wallet_arc.clone(),
                    )
                );
                
                // Spawn follower task
                let mut follower_shutdown = shutdown_rx2.resubscribe();
                tasks.push(tokio::spawn(async move {
                    tokio::select! {
                        result = follower.start(signal_tx) => {
                            if let Err(e) = result {
                                error!("Copy trading follower error: {}", e);
                            }
                        }
                        _ = follower_shutdown.recv() => {
                            info!("Copy trading follower shutting down");
                        }
                    }
                }));
                
                // Spawn signal processor task
                let mut processor_shutdown = shutdown_rx2.resubscribe();
                tasks.push(tokio::spawn(async move {
                    loop {
                        tokio::select! {
                            Some(signal) = signal_rx.recv() => {
                                info!("Received trade signal: {:?} {} SOL for token {}", 
                                      signal.action, signal.sol_amount, signal.token_mint);
                                
                                if let Err(e) = copy_executor.execute_copy_trade(&signal).await {
                                    error!("Failed to execute copy trade: {}", e);
                                }
                            }
                            _ = processor_shutdown.recv() => {
                                info!("Signal processor shutting down");
                                break;
                            }
                        }
                    }
                }));
            }
        }
    }

    // Initialize volume spike scalping system
    if config.is_volume_spike_enabled() {
        info!("Initializing volume spike scalping system...");
        info!("  Spike threshold: {}%", config.volume_spike_scalping.spike_threshold_percentage);
        info!("  Entry amount: {} SOL", config.volume_spike_scalping.entry_amount_sol);
        
        // TODO: Implement volume spike detection and trading
        // For now, just log that it's not implemented
        warn!("Volume spike scalping is enabled but not fully implemented yet");
    }

    // Main event loop
    info!("Bot is running. Press Ctrl+C to stop.");
    
    // Wait for shutdown signal
    let mut shutdown_rx_final = shutdown_rx;
    shutdown_rx_final.recv().await.ok();

    info!("Shutting down gracefully...");
    
    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    info!("Shutdown complete");
    Ok(())
}
