use anyhow::Result;
use solana_trading_bot::copy_trader::{
    CopyTraderConfig, TradeAction, WalletFollower,
};
use tokio::sync::mpsc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Copy Trading Demo");

    // Configure copy trading parameters
    let config = CopyTraderConfig {
        target_wallet: "54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy".to_string(),
        rpc_url: std::env::var("SOLANA_RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string()),
        ws_url: std::env::var("SOLANA_WS_URL")
            .unwrap_or_else(|_| "wss://api.mainnet-beta.solana.com".to_string()),
        
        // Copy settings
        copy_percentage: 0.5,  // Copy at 50% of original trade size
        min_trade_size: 0.1,   // Only copy trades >= 0.1 SOL
        max_position_size: 5.0, // Max 5 SOL per position
        
        // Execution settings
        delay_ms: 0,           // Execute immediately (0 delay)
        copy_buys: true,       // Copy buy trades
        copy_sells: true,      // Copy sell trades
        
        // Filtering
        pump_fun_only: true,   // Only copy pump.fun tokens
        min_confidence: 0.6,   // Only trades with confidence >= 0.6
        
        // Advanced settings
        enable_duplicate_detection: true,
        max_rpc_retries: 3,
        rate_limit_rps: 10,
        helius_api_key: std::env::var("HELIUS_API_KEY").ok(),
        execution_timeout_secs: 30,
        use_jito: false,
        priority_fee_multiplier: 1.2,
    };

    // Validate configuration
    config.validate()?;
    
    info!("Configuration validated successfully");
    info!("Target wallet: {}", config.target_wallet);
    info!("Copy percentage: {}%", config.copy_percentage * 100.0);
    info!("Min trade size: {} SOL", config.min_trade_size);
    info!("Max position size: {} SOL", config.max_position_size);

    // Create wallet follower
    let follower = WalletFollower::new(config.clone())?;
    info!("Wallet follower created");

    // Create channel for trade signals
    let (signal_tx, mut signal_rx) = mpsc::unbounded_channel();

    // Start monitoring in background task
    let monitor_handle = tokio::spawn(async move {
        if let Err(e) = follower.start(signal_tx).await {
            tracing::error!("Follower error: {}", e);
        }
    });

    // Process incoming trade signals
    info!("Listening for trade signals...");
    
    while let Some(signal) = signal_rx.recv().await {
        info!("=" .repeat(80));
        info!("NEW TRADE SIGNAL DETECTED");
        info!("=" .repeat(80));
        info!("Signature: {}", signal.signature);
        info!("Token Mint: {}", signal.token_mint);
        info!("Action: {:?}", signal.action);
        info!("SOL Amount: {:.4} SOL", signal.sol_amount);
        info!("Confidence: {:.2}%", signal.confidence * 100.0);
        info!("DEX: {}", signal.dex_program);
        info!("Timestamp: {}", signal.timestamp);
        info!("Is Pump.fun: {}", signal.is_pump_fun);
        info!("Wallet: {}", signal.wallet);
        
        // Calculate adjusted copy amount
        let copy_amount = signal.adjusted_amount(config.copy_percentage);
        info!("Copy Amount: {:.4} SOL ({}% of original)", 
              copy_amount, config.copy_percentage * 100.0);
        
        // Check if we should execute this trade
        if !config.should_copy_action(signal.action) {
            info!("Skipping: Action type not enabled in config");
            continue;
        }
        
        if !signal.is_pump_fun && config.pump_fun_only {
            info!("Skipping: Not a pump.fun token");
            continue;
        }
        
        // Here you would integrate with your executor
        info!("READY TO EXECUTE COPY TRADE");
        info!("Action: {:?} {:.4} SOL of token {}", 
              signal.action, copy_amount, signal.token_mint);
        
        // Example: Execute the copy trade
        match execute_copy_trade(&signal, copy_amount, &config).await {
            Ok(tx_sig) => {
                info!("✓ Copy trade executed successfully!");
                info!("Transaction: {}", tx_sig);
            }
            Err(e) => {
                tracing::error!("✗ Copy trade failed: {}", e);
            }
        }
        
        info!("=" .repeat(80));
    }

    // Wait for monitor to finish (shouldn't happen unless error)
    monitor_handle.await?;

    Ok(())
}

/// Execute a copy trade
async fn execute_copy_trade(
    signal: &solana_trading_bot::TradeSignal,
    copy_amount: f64,
    config: &CopyTraderConfig,
) -> Result<String> {
    info!("Executing copy trade...");
    
    // Apply execution delay if configured
    if config.delay_ms > 0 {
        info!("Applying delay: {} ms", config.delay_ms);
        tokio::time::sleep(tokio::time::Duration::from_millis(config.delay_ms)).await;
    }
    
    // Here you would integrate with your trading executor
    // This is a placeholder for demonstration
    
    match signal.action {
        TradeAction::Buy => {
            info!("Executing BUY: {:.4} SOL -> {}", copy_amount, signal.token_mint);
            
            // TODO: Integrate with your buy execution logic
            // Example:
            // let executor = TradingExecutor::new(config);
            // let tx_sig = executor.buy(
            //     &signal.token_mint,
            //     copy_amount,
            //     config.use_jito,
            //     config.priority_fee_multiplier,
            // ).await?;
            
            // For demo purposes, simulate execution
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            Ok("SimulatedBuySignature123...".to_string())
        }
        TradeAction::Sell => {
            info!("Executing SELL: Token {} -> {:.4} SOL", signal.token_mint, copy_amount);
            
            // TODO: Integrate with your sell execution logic
            // Example:
            // let executor = TradingExecutor::new(config);
            // let tx_sig = executor.sell(
            //     &signal.token_mint,
            //     signal.token_amount.unwrap_or_default(),
            //     config.use_jito,
            //     config.priority_fee_multiplier,
            // ).await?;
            
            // For demo purposes, simulate execution
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            Ok("SimulatedSellSignature456...".to_string())
        }
    }
}
