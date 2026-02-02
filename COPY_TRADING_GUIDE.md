# Wallet Copy Trading System - Complete Guide

## Overview

This copy trading system monitors a target Solana wallet in real-time and automatically replicates their trades on pump.fun tokens. It uses both WebSocket subscriptions and RPC polling for maximum reliability.

## Architecture

### Core Components

```
┌─────────────────┐
│ WalletFollower  │  - Main orchestrator
│                 │  - Manages WebSocket & polling
│                 │  - Coordinates other components
└────────┬────────┘
         │
         ├──────────────┐
         │              │
         ▼              ▼
┌─────────────────┐  ┌─────────────────┐
│ TransactionDecoder│  │  SignalFilter  │
│                 │  │                 │
│ - Decode txs    │  │ - Filter signals│
│ - Parse swaps   │  │ - Detect dupes  │
│ - Extract data  │  │ - Validate      │
└─────────────────┘  └─────────────────┘
         │
         ▼
┌─────────────────┐
│   TradeSignal   │
│                 │
│ - Token mint    │
│ - SOL amount    │
│ - Buy/Sell      │
│ - Confidence    │
│ - Timestamp     │
└─────────────────┘
         │
         ▼
┌─────────────────┐
│  Your Executor  │  - Execute copy trade
│                 │  - Apply position sizing
│                 │  - Risk management
└─────────────────┘
```

## Module Breakdown

### 1. `follower.rs` - Real-time Transaction Monitoring

**Purpose**: Monitor target wallet and detect new transactions

**Key Features**:
- **Dual monitoring strategy**:
  - WebSocket subscription for instant notifications
  - Polling as backup (every 5 seconds)
- **Automatic reconnection** with exponential backoff
- **Rate limiting** to prevent RPC throttling
- **Duplicate detection** to avoid processing same transaction twice

**Main Functions**:

```rust
pub async fn start(&self, signal_tx: mpsc::UnboundedSender<TradeSignal>) -> Result<()>
```
Starts monitoring the target wallet. Sends `TradeSignal` through the channel when trades detected.

**How It Works**:

1. **WebSocket Monitor**:
   - Connects to Solana WebSocket endpoint
   - Subscribes to account notifications for target wallet
   - On notification → fetches latest transactions
   - Auto-reconnects if connection drops

2. **Polling Monitor** (Backup):
   - Calls `getSignaturesForAddress` every 5 seconds
   - Fetches new signatures since last check
   - Processes each transaction in chronological order

3. **Transaction Processing**:
   ```
   New Signature → Fetch Full Transaction → Decode → 
   Filter → Validate → Send Signal
   ```

**Rate Limiting**:
- Configurable requests per second (`rate_limit_rps`)
- Prevents RPC bans from excessive calls
- Automatically spaces out requests

### 2. `decoder.rs` - Transaction Decoding

**Purpose**: Parse Solana transactions to extract swap details

**Supported DEX Programs**:
- **DFlow**: Aggregator used by many wallets
- **OkxDex**: OKX's on-chain DEX
- *(Extensible for more DEXs)*

**Key Functions**:

```rust
pub fn decode_transaction(
    &self,
    tx: &EncodedConfirmedTransactionWithStatusMeta,
    signature: &str,
) -> Result<Option<TradeSignal>>
```

**Decoding Process**:

1. **Verify Involvement**: Check if target wallet is in transaction
2. **Identify DEX**: Match program IDs in instructions
3. **Parse Instruction Data**:
   - Decode base58/base64 encoded data
   - Extract discriminator (instruction type)
   - Parse amounts (u64 little-endian)
4. **Extract Swap Details**:
   - Token mint address
   - SOL amount (convert lamports → SOL)
   - Direction (buy vs sell)
5. **Determine if Pump.fun**: Check token metadata

**Instruction Format** (DFlow/OkxDex):
```
Bytes 0-7:   Discriminator (instruction type)
Bytes 8-15:  Amount In (u64, little-endian)
Bytes 16-23: Minimum Amount Out (u64, little-endian)
```

**Account Extraction**:
```
Account[0]: User wallet
Account[1]: Token mint
Account[2]: SOL account
Account[3+]: Pool/program accounts
```

### 3. `signal.rs` - Trade Signal Processing

**Purpose**: Convert decoded transactions into validated trade signals

**TradeSignal Structure**:

```rust
pub struct TradeSignal {
    pub signature: String,           // Transaction signature
    pub token_mint: Pubkey,          // Token being traded
    pub sol_amount: f64,             // SOL amount (not lamports)
    pub action: TradeAction,         // Buy or Sell
    pub timestamp: DateTime<Utc>,    // When trade occurred
    pub confidence: f64,             // 0.0 to 1.0
    pub dex_program: String,         // "DFlow", "OkxDex", etc.
    pub token_amount: Option<u64>,   // Token amount if available
    pub is_pump_fun: bool,           // Is it a pump.fun token
    pub wallet: Pubkey,              // Wallet that made the trade
}
```

**Confidence Scoring**:

Automatically calculated based on trade size:

| SOL Amount | Confidence |
|------------|-----------|
| ≥ 10 SOL   | 1.0 (100%) |
| ≥ 5 SOL    | 0.9 (90%)  |
| ≥ 1 SOL    | 0.8 (80%)  |
| ≥ 0.5 SOL  | 0.7 (70%)  |
| ≥ 0.1 SOL  | 0.6 (60%)  |
| < 0.1 SOL  | 0.5 (50%)  |

**Signal Filtering**:

```rust
pub struct SignalFilter {
    pump_fun_only: bool,           // Only pump.fun tokens
    min_confidence: f64,           // Minimum confidence threshold
    seen_signatures: HashSet<String>, // Duplicate detection
}
```

**Filter Results**:
- `Accepted` - Signal passed all filters
- `Duplicate` - Already processed this transaction
- `Filtered(reason)` - Didn't meet criteria
- `Invalid(reason)` - Failed validation

**Validation Checks**:
1. ✓ Valid signature format
2. ✓ SOL amount > 0
3. ✓ Confidence score > 0
4. ✓ Not a duplicate
5. ✓ Meets minimum confidence threshold
6. ✓ Is pump.fun token (if required)
7. ✓ Meets minimum trade size
8. ✓ Doesn't exceed max position size

### 4. `mod.rs` - Configuration & Core Types

**CopyTraderConfig Structure**:

```rust
pub struct CopyTraderConfig {
    // Target & Connection
    pub target_wallet: String,          // Wallet to copy
    pub rpc_url: String,                // RPC endpoint
    pub ws_url: String,                 // WebSocket endpoint
    
    // Position Sizing
    pub copy_percentage: f64,           // 0.0-2.0 (1.0 = 100%)
    pub min_trade_size: f64,            // Min SOL to copy
    pub max_position_size: f64,         // Max SOL per position
    
    // Execution
    pub delay_ms: u64,                  // Delay before copying (0=instant)
    pub copy_buys: bool,                // Copy buy trades
    pub copy_sells: bool,               // Copy sell trades
    
    // Filtering
    pub pump_fun_only: bool,            // Only pump.fun tokens
    pub min_confidence: f64,            // Min confidence (0.0-1.0)
    
    // Reliability
    pub enable_duplicate_detection: bool,
    pub max_rpc_retries: u64,
    pub rate_limit_rps: u32,            // Requests per second
    
    // Advanced
    pub helius_api_key: Option<String>, // Enhanced API
    pub execution_timeout_secs: u64,
    pub use_jito: bool,
    pub priority_fee_multiplier: f64,
}
```

## Complete Copy Trading Flow

```
┌─────────────────────────────────────────────────────────────┐
│ 1. TARGET WALLET MAKES TRADE                                │
│    - Buys 1.5 SOL of pump.fun token via DFlow              │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. DETECTION (follower.rs)                                  │
│    - WebSocket: Account notification received              │
│    - Polling: New signature detected                        │
│    - Fetch full transaction details                         │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. DECODING (decoder.rs)                                    │
│    - Identify DFlow program in instructions                 │
│    - Parse instruction data                                 │
│    - Extract: token mint, 1.5 SOL, BUY action              │
│    - Verify: Is pump.fun token                              │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. SIGNAL CREATION (signal.rs)                             │
│    - Create TradeSignal                                     │
│    - Calculate confidence: 0.8 (1.5 SOL → 80%)            │
│    - Add metadata: timestamp, DEX, wallet                   │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. FILTERING (signal.rs)                                    │
│    - Check: Not duplicate ✓                                 │
│    - Check: Is pump.fun ✓                                   │
│    - Check: Confidence ≥ 0.6 ✓ (0.8 ≥ 0.6)                │
│    - Check: SOL ≥ min_trade_size ✓ (1.5 ≥ 0.1)            │
│    - Check: Adjusted amount ≤ max_position ✓               │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 6. POSITION SIZING                                          │
│    - Original: 1.5 SOL                                      │
│    - Copy percentage: 50% (0.5)                             │
│    - Adjusted: 1.5 × 0.5 = 0.75 SOL                        │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 7. EXECUTION (your executor)                                │
│    - Apply delay if configured (0ms = instant)              │
│    - Build transaction: BUY 0.75 SOL → token               │
│    - Set priority fee (multiplier × base)                   │
│    - Submit to Solana (or Jito if enabled)                  │
│    - Wait for confirmation                                  │
└────────────┬────────────────────────────────────────────────┘
             │
             ▼
┌─────────────────────────────────────────────────────────────┐
│ 8. COMPLETE ✓                                               │
│    - Your wallet now holds same token                       │
│    - Position: 0.75 SOL (50% of original 1.5 SOL)          │
└─────────────────────────────────────────────────────────────┘
```

## Configuration Examples

### Example 1: Conservative Copy Trading
```rust
let config = CopyTraderConfig {
    target_wallet: "54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy".to_string(),
    rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
    ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
    
    copy_percentage: 0.3,    // 30% of their trade size
    min_trade_size: 0.5,     // Only copy trades ≥ 0.5 SOL
    max_position_size: 3.0,  // Max 3 SOL per position
    
    delay_ms: 1000,          // 1 second delay (reduce frontrun risk)
    copy_buys: true,
    copy_sells: false,       // Only copy buys, manage sells yourself
    
    pump_fun_only: true,
    min_confidence: 0.7,     // Only high-confidence trades
    
    enable_duplicate_detection: true,
    max_rpc_retries: 5,
    rate_limit_rps: 5,
    
    helius_api_key: None,
    execution_timeout_secs: 30,
    use_jito: false,
    priority_fee_multiplier: 1.0,
};
```

### Example 2: Aggressive Mirror Trading
```rust
let config = CopyTraderConfig {
    target_wallet: "54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy".to_string(),
    rpc_url: env::var("HELIUS_RPC_URL")?,  // Fast RPC
    ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
    
    copy_percentage: 1.0,    // 100% exact mirror
    min_trade_size: 0.05,    // Copy even small trades
    max_position_size: 20.0, // Large max position
    
    delay_ms: 0,             // Execute immediately
    copy_buys: true,
    copy_sells: true,        // Mirror both buys and sells
    
    pump_fun_only: true,
    min_confidence: 0.5,     // Accept all trades
    
    enable_duplicate_detection: true,
    max_rpc_retries: 3,
    rate_limit_rps: 20,      // Higher rate limit
    
    helius_api_key: Some(env::var("HELIUS_API_KEY")?),
    execution_timeout_secs: 15,
    use_jito: true,          // Use Jito for speed
    priority_fee_multiplier: 2.0,  // High priority
};
```

### Example 3: Selective Large Trades Only
```rust
let config = CopyTraderConfig {
    target_wallet: "54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy".to_string(),
    rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
    ws_url: "wss://api.mainnet-beta.solana.com".to_string(),
    
    copy_percentage: 0.5,    
    min_trade_size: 2.0,     // Only copy significant trades ≥ 2 SOL
    max_position_size: 10.0,
    
    delay_ms: 500,
    copy_buys: true,
    copy_sells: false,
    
    pump_fun_only: true,
    min_confidence: 0.8,     // High confidence only
    
    enable_duplicate_detection: true,
    max_rpc_retries: 3,
    rate_limit_rps: 10,
    
    helius_api_key: None,
    execution_timeout_secs: 30,
    use_jito: false,
    priority_fee_multiplier: 1.5,
};
```

## Edge Cases Handled

### 1. Duplicate Transactions
**Problem**: WebSocket and polling might detect same transaction

**Solution**: 
- `SignalFilter` maintains `HashSet<String>` of seen signatures
- Automatic deduplication on signature hash
- Configurable cache size (10,000 signatures)
- Auto-cleanup when cache fills

### 2. Failed Transactions
**Problem**: Target wallet's transaction might fail

**Solution**:
- Decoder checks `tx.meta.err` field
- Only processes successful transactions
- Logs failures for monitoring

### 3. RPC Rate Limiting
**Problem**: Too many requests → banned by RPC

**Solution**:
- Built-in rate limiter with configurable RPS
- Exponential backoff on errors
- Retry logic with max attempts
- Spreads requests evenly over time

### 4. WebSocket Disconnections
**Problem**: Connection drops, miss trades

**Solution**:
- Automatic reconnection with backoff
- Polling backup continues during reconnect
- Resubscribes to all subscriptions on reconnect
- No trades missed

### 5. Slow RPC Responses
**Problem**: By time you fetch full transaction, price moved

**Solution**:
- Configurable `delay_ms` (can be 0 for instant)
- Use Helius or premium RPC for speed
- Jito integration for faster inclusion
- Higher priority fees option

### 6. Invalid Transaction Format
**Problem**: Transaction structure doesn't match expected format

**Solution**:
- Robust error handling at each decode step
- Returns `None` instead of erroring
- Logs parsing issues for debugging
- Continues monitoring

## Performance Optimization

### Fast RPC Endpoints
```rust
// Use Helius or QuickNode for best performance
rpc_url: "https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
```

### Adjust Polling Interval
```rust
// In follower.rs, line 213
let mut poll_interval = interval(Duration::from_secs(5));
// Reduce to 2-3 seconds for faster detection
```

### Parallel Processing
```rust
// Process multiple signals concurrently
let handles: Vec<_> = signals.into_iter()
    .map(|signal| tokio::spawn(execute_copy_trade(signal)))
    .collect();
```

### Helius Enhanced Transactions
```rust
config.helius_api_key = Some("your-api-key".to_string());
// Provides parsed transaction data, faster than RPC
```

## Monitoring & Alerts

### Key Metrics to Track

1. **Detection Latency**: Time from trade → signal
2. **Execution Latency**: Time from signal → confirmation
3. **Success Rate**: Percentage of successful copies
4. **Duplicate Rate**: How many duplicates detected
5. **Filter Rate**: How many signals filtered out

### Logging

The system uses `tracing` for structured logging:

```rust
// follower.rs logs:
info!("Found {} new transactions", count);
warn!("Polling error: {}", e);
error!("WebSocket error: {}", e);
debug!("Skipping duplicate signature");

// decoder.rs logs:
debug!("Decoding DFlow transaction");
warn!("Failed to parse instruction data");

// signal.rs logs:
debug!("Signal filtered: {:?}", result);
```

### Integration with Your Monitoring

```rust
use your_monitoring::{record_metric, send_alert};

// After signal received
record_metric("copy_trade.signal_received", 1);

// After execution
if execution_success {
    record_metric("copy_trade.execution_success", 1);
} else {
    record_metric("copy_trade.execution_failure", 1);
    send_alert("Copy trade execution failed");
}
```

## Integration with Executor

### Connect to Your Trading Executor

```rust
use your_executor::TradingExecutor;

async fn execute_copy_trade(
    signal: &TradeSignal,
    copy_amount: f64,
    config: &CopyTraderConfig,
) -> Result<String> {
    // Initialize your executor
    let executor = TradingExecutor::new(/* your config */)?;
    
    match signal.action {
        TradeAction::Buy => {
            executor.buy_token(
                &signal.token_mint,
                copy_amount,
                config.use_jito,
                config.priority_fee_multiplier,
            ).await
        }
        TradeAction::Sell => {
            executor.sell_token(
                &signal.token_mint,
                signal.token_amount.unwrap_or_default(),
                config.use_jito,
                config.priority_fee_multiplier,
            ).await
        }
    }
}
```

## Testing

### Unit Tests

Run tests for each module:
```bash
cargo test --lib copy_trader
```

### Integration Test

Test with target wallet (testnet):
```bash
# Set environment variables
export SOLANA_RPC_URL="https://api.testnet.solana.com"
export SOLANA_WS_URL="wss://api.testnet.solana.com"

# Run demo
cargo run --example copy_trader_demo
```

### Dry Run Mode

```rust
// Add to config
pub struct CopyTraderConfig {
    // ... existing fields ...
    pub dry_run: bool,  // Don't actually execute, just log
}

// In execution
if config.dry_run {
    info!("DRY RUN: Would execute trade: {:?}", signal);
    return Ok("dry-run".to_string());
}
```

## Security Considerations

### 1. Wallet Security
- Never hardcode private keys
- Use environment variables or secure vaults
- Keep execution wallet separate from storage wallet

### 2. RPC Security
- Don't expose RPC URLs with API keys
- Use environment variables
- Rotate API keys periodically

### 3. Rate Limiting
- Respect RPC provider limits
- Use `rate_limit_rps` appropriately
- Monitor for rate limit errors

### 4. Position Limits
- Always set `max_position_size`
- Implement daily loss limits
- Use stop losses

### 5. Validation
- Validate all signals before execution
- Check token blacklists
- Verify pump.fun token authenticity

## Troubleshooting

### Issue: Not detecting transactions

**Check**:
1. WebSocket connection established?
2. RPC endpoint accessible?
3. Target wallet address correct?
4. Wallet actually making trades?

**Debug**:
```rust
// Add debug logging
debug!("WebSocket status: {:?}", ws_stream.state());
debug!("Last signature: {:?}", last_signature);
```

### Issue: Detecting but not executing

**Check**:
1. Signals being filtered? Check `min_confidence`, `min_trade_size`
2. Is `copy_buys` or `copy_sells` enabled?
3. Is `pump_fun_only` true when tokens aren't pump.fun?

**Debug**:
```rust
// Log filter results
info!("Filter result: {:?}", filter_result);
info!("Signal: {:?}", signal);
```

### Issue: High latency

**Optimize**:
1. Use faster RPC (Helius, QuickNode)
2. Reduce polling interval
3. Enable Jito bundles
4. Increase priority fee multiplier
5. Run bot closer to RPC region

### Issue: RPC rate limits

**Solutions**:
1. Increase `rate_limit_rps` carefully
2. Use premium RPC with higher limits
3. Reduce polling frequency
4. Disable duplicate detection (uses extra RPC calls)

## Advanced Features

### Multi-Wallet Copy Trading

```rust
let wallets = vec![
    "wallet1...".to_string(),
    "wallet2...".to_string(),
    "wallet3...".to_string(),
];

let handles: Vec<_> = wallets.into_iter().map(|wallet| {
    let config = CopyTraderConfig::new(wallet, rpc_url, ws_url);
    let follower = WalletFollower::new(config)?;
    let (tx, rx) = mpsc::unbounded_channel();
    
    tokio::spawn(async move {
        follower.start(tx).await
    })
}).collect();
```

### Smart Position Sizing

```rust
fn calculate_dynamic_copy_amount(
    signal: &TradeSignal,
    config: &CopyTraderConfig,
    current_portfolio: &Portfolio,
) -> f64 {
    let base_amount = signal.adjusted_amount(config.copy_percentage);
    
    // Adjust based on confidence
    let confidence_multiplier = signal.confidence;
    
    // Adjust based on current exposure
    let exposure_multiplier = if current_portfolio.total_exposure > 50.0 {
        0.5  // Reduce size if highly exposed
    } else {
        1.0
    };
    
    (base_amount * confidence_multiplier * exposure_multiplier)
        .clamp(config.min_trade_size, config.max_position_size)
}
```

### Conditional Execution

```rust
async fn should_execute_copy_trade(
    signal: &TradeSignal,
    market_conditions: &MarketConditions,
) -> bool {
    // Don't copy during high volatility
    if market_conditions.volatility > 0.8 {
        return false;
    }
    
    // Check token liquidity
    let token_info = fetch_token_info(&signal.token_mint).await?;
    if token_info.liquidity < 10.0 {  // Min 10 SOL liquidity
        return false;
    }
    
    // Check if we already hold this token
    let current_position = get_position(&signal.token_mint).await?;
    if current_position.is_some() {
        return false;  // Don't double-position
    }
    
    true
}
```

## Maintenance

### Update Program IDs

If DEX programs update their addresses:

```rust
// In copy_trader/mod.rs, program_ids module
pub fn new_dex() -> Pubkey {
    Pubkey::from_str("NewDexProgramId...")
        .expect("Valid program ID")
}
```

### Add New DEX Support

1. Add program ID to `program_ids` module
2. Implement decoder in `decoder.rs`:
```rust
fn decode_new_dex_swap(...) -> Result<Option<TradeSignal>> {
    // Parse new DEX format
}
```
3. Call from `decode_transaction`:
```rust
if let Some(signal) = self.decode_new_dex_swap(...) {
    return Ok(Some(signal));
}
```

## License & Disclaimer

**Use at your own risk**. Copy trading involves:
- Market risk
- Execution risk
- Smart contract risk
- Impermanent loss

Always test thoroughly on testnet first. Never risk more than you can afford to lose.

---

## Quick Start Checklist

- [ ] Set environment variables (RPC_URL, WS_URL)
- [ ] Configure `CopyTraderConfig` with target wallet
- [ ] Set appropriate `copy_percentage` and limits
- [ ] Enable desired filters (`pump_fun_only`, `min_confidence`)
- [ ] Test on testnet first
- [ ] Integrate with your executor
- [ ] Set up monitoring and alerts
- [ ] Start with small position sizes
- [ ] Monitor for 24h before scaling up

**Ready to launch!** 🚀
