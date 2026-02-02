# Solana Copy Trading Bot

A high-performance copy trading system for Solana that monitors and replicates trades from a target wallet in real-time.

## Features

- **Real-time Monitoring**: WebSocket subscription for instant transaction updates
- **Dual Monitoring**: WebSocket + RPC polling for redundancy
- **Smart Decoding**: Supports DFlow and OkxDex program formats
- **Pump.fun Focus**: Filters for pump.fun token transactions
- **Confidence Scoring**: Trade signals ranked by size and reliability
- **Duplicate Detection**: Prevents processing the same transaction twice
- **Rate Limiting**: Built-in RPC rate limiting to avoid throttling
- **Configurable Copying**: Adjust trade size, delays, and limits
- **Retry Logic**: Automatic retry for failed RPC calls
- **Edge Case Handling**: Robust error handling and recovery

## Architecture

### Module Structure

```
src/
├── copy_trader/
│   ├── mod.rs         # Main module with configuration
│   ├── follower.rs    # Real-time transaction monitoring
│   ├── decoder.rs     # Transaction parsing and decoding
│   └── signal.rs      # Trade signal generation and filtering
├── lib.rs             # Library exports
└── main.rs            # Entry point with example usage
```

### Copy Trading Flow

1. **Monitor** (`follower.rs`)
   - WebSocket subscribes to target wallet notifications
   - RPC polling fetches recent transactions as backup
   - Rate limiter prevents API throttling

2. **Decode** (`decoder.rs`)
   - Parse transaction structure
   - Extract swap instruction data
   - Identify DEX program (DFlow/OkxDex)
   - Extract token mint, amounts, and direction

3. **Signal** (`signal.rs`)
   - Convert transaction to TradeSignal struct
   - Calculate confidence score based on trade size
   - Filter non-pump.fun transactions
   - Check for duplicates

4. **Execute** (main.rs / your implementation)
   - Apply copy percentage to trade size
   - Respect min/max position limits
   - Optional execution delay
   - Submit copy trade to network

## Configuration

### Environment Variables

Create a `.env` file with the following variables:

```env
# Target wallet to monitor (default: 54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy)
TARGET_WALLET=54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy

# Copy percentage (1.0 = 100%, 0.5 = 50%)
COPY_PERCENTAGE=1.0

# Minimum trade size in SOL to copy
MIN_TRADE_SIZE=0.01

# Maximum position size in SOL
MAX_POSITION_SIZE=10.0

# Delay in milliseconds before executing (0 = instant)
EXECUTION_DELAY_MS=0

# Solana RPC endpoint
RPC_URL=https://api.mainnet-beta.solana.com

# WebSocket URL for real-time updates
WS_URL=wss://api.mainnet-beta.solana.com

# Helius API key (optional, for enhanced transactions)
HELIUS_API_KEY=your_api_key_here

# Enable duplicate transaction detection
ENABLE_DUPLICATE_DETECTION=true

# Maximum retries for failed RPC calls
MAX_RPC_RETRIES=3

# Rate limit: requests per second
RATE_LIMIT_RPS=10
```

### Configuration Struct

```rust
pub struct CopyTraderConfig {
    pub target_wallet: String,
    pub copy_percentage: f64,
    pub min_trade_size: f64,
    pub max_position_size: f64,
    pub execution_delay_ms: u64,
    pub rpc_url: String,
    pub helius_api_key: Option<String>,
    pub ws_url: String,
    pub enable_duplicate_detection: bool,
    pub max_rpc_retries: u32,
    pub rate_limit_rps: u32,
}
```

## Usage

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

### Run with custom log level

```bash
RUST_LOG=debug cargo run --release
```

## Key Components

### 1. WalletFollower

Monitors the target wallet using dual strategy:

- **WebSocket**: Subscribes to `accountSubscribe` for instant notifications
- **Polling**: Fetches signatures every 5 seconds as backup

```rust
let follower = WalletFollower::new(config)?;
let (signal_tx, signal_rx) = mpsc::unbounded_channel();
follower.start(signal_tx).await?;
```

### 2. TransactionDecoder

Decodes Solana transactions to extract swap information:

```rust
let decoder = TransactionDecoder::new(target_wallet);
if let Some(signal) = decoder.decode_transaction(&tx, signature)? {
    // Process signal
}
```

Supports:
- DFlow program swaps
- OkxDex program swaps
- Pump.fun token identification

### 3. TradeSignal

Represents a decoded trade with metadata:

```rust
pub struct TradeSignal {
    pub signature: String,
    pub token_mint: Pubkey,
    pub sol_amount: f64,
    pub action: TradeAction,      // Buy or Sell
    pub timestamp: DateTime<Utc>,
    pub confidence: f64,          // 0.0 to 1.0
    pub dex_program: String,
    pub token_amount: Option<u64>,
    pub is_pump_fun: bool,
    pub wallet: Pubkey,
}
```

### 4. SignalFilter

Filters and validates trade signals:

```rust
let mut filter = SignalFilter::new(
    true,  // pump_fun_only
    0.5    // min_confidence
);

match filter.filter(&signal) {
    FilterResult::Accepted => { /* Execute trade */ }
    FilterResult::Duplicate => { /* Already seen */ }
    FilterResult::Filtered(reason) => { /* Filtered out */ }
    FilterResult::Invalid(reason) => { /* Invalid signal */ }
}
```

## Edge Cases Handled

### 1. Duplicate Detection
- Maintains a HashSet of seen signatures
- Automatically cleans old entries when limit reached
- Configurable via `enable_duplicate_detection`

### 2. Failed Transactions
- Retry logic with exponential backoff
- Configurable max retries via `max_rpc_retries`
- Graceful degradation if transaction unavailable

### 3. Rate Limiting
- Token bucket style rate limiter
- Configurable requests per second
- Prevents API throttling and bans

### 4. WebSocket Reconnection
- Automatic reconnection on connection loss
- 5-second delay between reconnection attempts
- Seamless continuation of monitoring

### 5. Invalid Data
- Validates all transaction structures
- Handles missing or malformed instruction data
- Gracefully skips unparseable transactions

## Confidence Scoring

Trade signals are scored based on SOL amount:

| SOL Amount | Confidence |
|------------|------------|
| ≥ 10.0     | 1.0        |
| ≥ 5.0      | 0.9        |
| ≥ 1.0      | 0.8        |
| ≥ 0.5      | 0.7        |
| ≥ 0.1      | 0.6        |
| < 0.1      | 0.5        |

Larger trades indicate stronger conviction and get higher confidence.

## Performance Considerations

### WebSocket vs Polling

- **WebSocket**: Near-instant (< 1 second latency)
- **Polling**: 5-second intervals for backup
- Both run concurrently for redundancy

### Rate Limiting

Default: 10 requests/second
- Prevents API throttling
- Adjustable based on your RPC provider limits
- Premium RPCs (Helius, QuickNode) support higher rates

### Memory Usage

- Signature cache: ~10,000 entries max
- Automatic cleanup prevents unbounded growth
- Minimal memory footprint per signal

## Integration with Trading Executor

To complete the copy trading system, integrate with a trading executor:

```rust
async fn process_trade_signal(signal: TradeSignal, config: &CopyTraderConfig) -> Result<()> {
    let adjusted_amount = signal.adjusted_amount(config.copy_percentage);
    
    // 1. Build swap transaction
    let tx = build_swap_transaction(
        signal.token_mint,
        adjusted_amount,
        signal.action,
    ).await?;
    
    // 2. Sign with your wallet
    let signed_tx = wallet.sign_transaction(tx)?;
    
    // 3. Submit to network
    let signature = rpc_client.send_transaction(&signed_tx).await?;
    
    // 4. Monitor confirmation
    rpc_client.confirm_transaction(&signature).await?;
    
    Ok(())
}
```

## Testing

Run tests:

```bash
cargo test
```

Run tests with output:

```bash
cargo test -- --nocapture
```

## Dependencies

- `solana-client`: RPC client for blockchain interaction
- `solana-sdk`: Core Solana types and utilities
- `tokio`: Async runtime
- `tokio-tungstenite`: WebSocket support
- `serde`: Serialization/deserialization
- `reqwest`: HTTP client for Helius API
- `tracing`: Structured logging
- `anyhow`: Error handling

## Roadmap

- [ ] Helius enhanced transactions integration
- [ ] Multi-wallet monitoring
- [ ] Advanced filtering (liquidity, market cap)
- [ ] Slippage protection
- [ ] MEV protection via Jito bundles
- [ ] Performance metrics and dashboards
- [ ] Telegram notifications
- [ ] Database logging

## Production Monitoring & Safety Features

### Logging System

The bot includes production-grade logging with:

- **File Logging**: Rotating log files (daily rotation, keeps 30 days)
- **JSON Structured Logs**: Parseable JSON format for trade events
- **Separate Error Log**: Dedicated error log file for easier debugging
- **Log Levels**: Configurable via `RUST_LOG` environment variable

**Log Files:**
- `logs/trading-bot.log` - General application logs (JSON format)
- `logs/trading-bot-errors.log` - Error-only logs (human-readable)

**Example Log Entry:**
```json
{
  "timestamp": "2026-02-02T12:34:56Z",
  "level": "INFO",
  "message": "Trade executed",
  "trade_event": {
    "event_type": "trade",
    "trade": {
      "signature": "5KJp...",
      "token_mint": "ABC123...",
      "action": "buy",
      "amount_sol": 0.5,
      "entry_price": 0.001,
      "status": "success"
    }
  }
}
```

### Telegram Alerts

Optional Telegram bot integration for real-time notifications:

**Setup:**
1. Create a Telegram bot via [@BotFather](https://t.me/botfather)
2. Get your bot token
3. Get your chat ID (use [@userinfobot](https://t.me/userinfobot))
4. Set environment variables:
   ```bash
   export TELEGRAM_BOT_TOKEN="your_bot_token"
   export TELEGRAM_CHAT_ID="your_chat_id"
   ```

**Alert Types:**
- ✅ Trade entry/exit notifications
- 🚨 Error alerts
- 💰 Profit/loss milestones
- 📊 Daily summary reports
- 🚀 Volume spike alerts
- 📋 Copy trade notifications

**Rate Limiting**: Automatically rate-limited to prevent Telegram API throttling (configurable via `max_messages_per_minute`)

### Risk Management

Comprehensive risk management system:

**Position Tracking:**
- Real-time position monitoring
- Automatic stop-loss and take-profit execution
- Trailing stop support
- Time-based exits (max hold duration)

**Daily Limits:**
- Maximum daily loss limit
- Maximum daily trades limit
- Automatic daily reset at midnight UTC

**Circuit Breaker:**
- Stops trading after X consecutive losses
- Prevents catastrophic drawdowns
- Manual reset required after activation

**Emergency Exit:**
- Panic sell functionality
- Emergency stop-loss percentage
- Force close all positions

**Example Configuration:**
```yaml
risk_management:
  default_stop_loss_percentage: 10.0      # 10% stop loss
  default_take_profit_percentage: 20.0      # 20% take profit
  use_trailing_stop: true
  trailing_stop_activation_percentage: 5.0  # Activate after 5% profit
  trailing_stop_distance_percentage: 2.0    # 2% trailing distance
  max_daily_loss_sol: 1.0                   # Stop after 1 SOL loss
  max_daily_trades: 10                      # Max 10 trades per day
  max_consecutive_losses: 3                 # Circuit breaker at 3 losses
  min_sol_balance: 0.1                     # Minimum balance required
  emergency_stop_loss_percentage: 50.0     # Emergency exit at 50% loss
  panic_sell_enabled: true
```

### Safety Features

**Dry-Run Mode:**
Test the bot without executing real trades:
```bash
export DRY_RUN=true
cargo run --release
```

In dry-run mode:
- All trades are simulated
- No real transactions are sent
- Logs show `[DRY-RUN]` prefix
- Perfect for testing and validation

**Balance Checks:**
- Pre-trade balance verification
- Minimum balance requirements
- Automatic balance monitoring
- Low balance warnings

**Transaction Simulation:**
- Simulates transactions before sending (if enabled)
- Validates transaction will succeed
- Prevents failed transactions and wasted fees
- Configurable via `advanced.simulate_transactions`

**Circuit Breaker:**
- Automatically stops trading after consecutive failures
- Prevents further losses during bad market conditions
- Requires manual reset to resume trading
- Sends Telegram alert when activated

## Quick Start Guide

### 1. Prerequisites

- Rust 1.70+ installed
- Solana CLI tools (optional, for wallet management)
- Sufficient SOL balance for trading
- RPC endpoint (free or premium)

### 2. Installation

```bash
# Clone the repository
git clone <repository-url>
cd trading-bot

# Build the project
cargo build --release
```

### 3. Configuration

**Step 1: Create `.env` file**
```bash
cp .env.example .env
```

**Step 2: Set required variables**
```bash
# Wallet (required)
export WALLET_PRIVATE_KEY="your_base58_private_key"

# RPC (required)
export RPC_URL="https://api.mainnet-beta.solana.com"
# OR use Helius
export HELIUS_API_KEY="your_helius_key"

# Telegram (optional)
export TELEGRAM_BOT_TOKEN="your_bot_token"
export TELEGRAM_CHAT_ID="your_chat_id"

# Safety (recommended for first run)
export DRY_RUN=true  # Set to false for live trading
```

**Step 3: Configure `config/bot_config.yaml`**
- Set your target wallets
- Configure risk management limits
- Adjust trading parameters
- Enable/disable features

### 4. First Run (Dry-Run)

**Always test in dry-run mode first!**

```bash
export DRY_RUN=true
export RUST_LOG=info
cargo run --release
```

Monitor the logs to ensure:
- ✅ Bot connects to Solana RPC
- ✅ Wallet loads successfully
- ✅ Risk manager initializes
- ✅ Monitoring systems start
- ✅ No errors in logs

### 5. Live Trading

**Only proceed after thorough testing!**

```bash
# Disable dry-run
export DRY_RUN=false

# Start with conservative settings
# - Small position sizes
# - Tight stop-losses
# - Low daily limits

cargo run --release
```

## Configuration Examples

### Conservative Configuration (Recommended for Beginners)

```yaml
risk_management:
  default_stop_loss_percentage: 5.0        # Tight 5% stop loss
  default_take_profit_percentage: 10.0     # Quick 10% profit target
  max_daily_loss_sol: 0.5                   # Stop after 0.5 SOL loss
  max_daily_trades: 5                       # Limit to 5 trades/day
  max_consecutive_losses: 2                 # Circuit breaker at 2 losses
  min_sol_balance: 0.5                     # Keep 0.5 SOL reserve

trading:
  default_trade_size_sol: 0.1               # Small 0.1 SOL trades
  max_position_size_sol: 0.2                # Max 0.2 SOL per position
```

### Aggressive Configuration (Advanced Users Only)

```yaml
risk_management:
  default_stop_loss_percentage: 15.0        # Wider 15% stop loss
  default_take_profit_percentage: 30.0      # Higher 30% profit target
  max_daily_loss_sol: 5.0                    # Allow 5 SOL daily loss
  max_daily_trades: 20                      # Up to 20 trades/day
  max_consecutive_losses: 5                  # Circuit breaker at 5 losses

trading:
  default_trade_size_sol: 1.0               # Larger 1 SOL trades
  max_position_size_sol: 2.0                # Max 2 SOL per position
```

## Safety Recommendations

### ⚠️ Critical Safety Rules

1. **Start Small**: Begin with minimum position sizes (0.01-0.1 SOL)
2. **Use Dry-Run First**: Always test in dry-run mode before live trading
3. **Set Stop-Losses**: Never trade without stop-loss protection
4. **Daily Limits**: Set conservative daily loss limits
5. **Monitor Actively**: Watch logs and Telegram alerts
6. **Separate Wallet**: Use a dedicated trading wallet, not your main wallet
7. **Test on Devnet**: Test thoroughly on Solana devnet first
8. **Keep Reserves**: Maintain minimum SOL balance for fees

### 🔒 Security Best Practices

1. **Private Key Storage**:
   - Never commit private keys to git
   - Use environment variables or secure key management
   - Consider hardware wallet integration
   - Encrypt wallet files if storing locally

2. **RPC Security**:
   - Use premium RPC providers (Helius, QuickNode)
   - Rotate API keys regularly
   - Monitor RPC usage and limits
   - Have backup RPC endpoints

3. **Monitoring**:
   - Enable Telegram alerts
   - Monitor logs regularly
   - Set up log rotation
   - Review daily summaries

4. **Risk Management**:
   - Never risk more than you can afford to lose
   - Set appropriate position sizes
   - Use circuit breakers
   - Enable emergency stop functionality

## Troubleshooting

### Common Issues

#### 1. "Insufficient Balance" Error

**Problem**: Bot refuses to trade due to low balance

**Solutions**:
- Check wallet balance: `solana balance <your_wallet>`
- Ensure balance > `min_sol_balance` + trade amount + fees
- Reserve at least 0.1 SOL for transaction fees
- Reduce `min_sol_balance` in config if appropriate

#### 2. "Circuit Breaker Active" Error

**Problem**: Trading stopped after consecutive losses

**Solutions**:
- Review recent trades and losses
- Check if market conditions have changed
- Manually reset circuit breaker (requires code change or restart)
- Adjust `max_consecutive_losses` if too conservative
- Review and improve trading strategy

#### 3. "Rate Limit Exceeded" Error

**Problem**: RPC rate limiting

**Solutions**:
- Reduce `rate_limit_per_second` in config
- Use premium RPC provider (Helius, QuickNode)
- Add backup RPC endpoints
- Implement request queuing

#### 4. Telegram Alerts Not Working

**Problem**: No Telegram notifications received

**Solutions**:
- Verify `TELEGRAM_BOT_TOKEN` is set correctly
- Verify `TELEGRAM_CHAT_ID` is set correctly
- Check bot is enabled in config: `telegram.enabled: true`
- Test bot token: `curl "https://api.telegram.org/bot<TOKEN>/getMe"`
- Check rate limiting (may be queuing messages)

#### 5. Logs Not Writing to File

**Problem**: No log files created

**Solutions**:
- Check `monitoring.log_to_file: true` in config
- Verify `logs/` directory exists and is writable
- Check disk space
- Review log rotation settings

#### 6. Transactions Failing

**Problem**: Trades execute but fail on-chain

**Solutions**:
- Enable transaction simulation: `advanced.simulate_transactions: true`
- Increase priority fees: `trading.priority_fee_lamports`
- Check network congestion
- Verify sufficient balance for fees
- Review slippage settings

#### 7. Dry-Run Mode Not Working

**Problem**: Real trades executing in dry-run mode

**Solutions**:
- Verify `DRY_RUN=true` environment variable is set
- Check logs for `[DRY-RUN]` prefix
- Restart bot after setting environment variable
- Verify no code overrides dry-run flag

### Debug Mode

Enable detailed logging:
```bash
export RUST_LOG=debug
cargo run --release
```

Check specific modules:
```bash
export RUST_LOG="solana_trading_bot::bot=debug,solana_trading_bot::risk=debug"
cargo run --release
```

### Health Checks

The bot includes health check endpoints (if enabled):
- Check bot status
- View current positions
- Monitor risk metrics
- Review daily statistics

### Getting Help

1. **Check Logs**: Review `logs/trading-bot-errors.log` for errors
2. **Review Configuration**: Verify all settings are correct
3. **Test Components**: Test RPC, wallet, Telegram separately
4. **GitHub Issues**: Open an issue with:
   - Error messages
   - Relevant log excerpts
   - Configuration (redact sensitive info)
   - Steps to reproduce

## Monitoring & Metrics

### Log Analysis

**Parse JSON logs:**
```bash
# View recent trades
cat logs/trading-bot.log | jq 'select(.trade_event)'

# Count successful trades
cat logs/trading-bot.log | jq 'select(.trade_event.trade.status == "success")' | wc -l

# Calculate total P&L
cat logs/trading-bot.log | jq -r '.trade_event.trade.profit_loss_sol // 0' | awk '{sum+=$1} END {print sum}'
```

### Daily Summary

The bot sends daily summary reports via Telegram:
- Total trades executed
- Win rate percentage
- Total profit/loss
- Daily statistics

### Position Monitoring

Monitor open positions:
- Current unrealized P&L
- Stop-loss and take-profit levels
- Position age
- Risk exposure

## Testing

### Unit Tests

Run all tests:
```bash
cargo test
```

Run specific test module:
```bash
cargo test risk::manager
```

Run with output:
```bash
cargo test -- --nocapture
```

### Integration Tests

Test with devnet:
```bash
export RPC_URL="https://api.devnet.solana.com"
export DRY_RUN=true
cargo run --release
```

## Security Considerations

⚠️ **Important**: This is a trading bot that executes real transactions with real money.

- Store private keys securely (hardware wallet, encrypted storage)
- Start with small position sizes while testing
- Use a dedicated trading wallet, not your main wallet
- Monitor for unexpected behavior
- Set strict `max_position_size` limits
- Test thoroughly on devnet first
- Enable all safety features (dry-run, circuit breaker, stop-losses)
- Review logs regularly
- Keep software updated

## License

MIT License - see LICENSE file for details

## Support

For issues, questions, or contributions, please open an issue on GitHub.
