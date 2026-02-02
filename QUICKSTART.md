# Quick Start Guide - Solana Copy Trading Bot

## What You Have

**✅ A fully functional Rust trading bot with TWO independent trading modes:**

### Mode 1: Copy Trading (Primary Feature)
- Monitors wallet `54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy` in real-time
- Detects their DFlow and OkxDex swaps instantly  
- Copies the same trades automatically
- Uses WebSocket + RPC polling for maximum reliability

### Mode 2: Volume Spike Scalping (Bonus Feature)
- Scans all pump.fun tokens for volume spikes
- Enters trades when 300%+ volume increases detected
- Independent scalping strategy
- Can run alongside or separately from copy trading

## How to Run

### Step 1: Configure Your Setup

1. **Copy the environment template:**
```bash
copy .env.example .env
```

2. **Edit `.env` and add your wallet private key:**
```
WALLET_PRIVATE_KEY=your_base58_encoded_private_key_here
```

3. **Optional: Add premium RPC (recommended for production):**
```
RPC_URL=https://mainnet.helius-rpc.com/?api-key=YOUR_KEY
WS_URL=wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY
```

### Step 2: Choose Which Bot(s) to Run

Edit `config/bot_config.yaml`:

**For COPY TRADING ONLY (recommended to start):**
```yaml
copy_trading:
  enabled: true    # ← Keep this
  
volume_spike_scalping:
  enabled: false   # ← Disable this
```

**For VOLUME SCALPING ONLY:**
```yaml
copy_trading:
  enabled: false   # ← Disable this
  
volume_spike_scalping:
  enabled: true    # ← Keep this
```

**For BOTH (more trades, more risk):**
```yaml
copy_trading:
  enabled: true    # ← Both enabled
  
volume_spike_scalping:
  enabled: true    # ← Both enabled
```

### Step 3: Test in Dry-Run Mode First

**IMPORTANT: Always test first!**

```bash
# Set dry-run mode in .env
DRY_RUN=true

# Build and run
cargo run --release
```

This will:
- Connect to Solana
- Monitor the target wallet
- Log all detected trades
- **NOT execute real trades** (simulation only)

### Step 4: Run for Real

Once you're confident it works:

1. Edit `.env`:
```
DRY_RUN=false
```

2. Start with small amounts in `config/bot_config.yaml`:
```yaml
copy_trading:
  target_wallets:
    - max_copy_amount_sol: 0.1  # Start with max 0.1 SOL per trade
```

3. Run:
```bash
cargo run --release
```

## Safety Features Built-In

The bot includes multiple safety mechanisms:

### Automatic Stop-Loss
```yaml
risk_management:
  default_stop_loss_percentage: 20.0  # Exits at -20% loss
```

### Daily Limits
```yaml
risk_management:
  max_daily_loss_sol: 1.0        # Stops trading after 1 SOL loss per day
  max_daily_trades: 50           # Max 50 trades per day
```

### Position Limits
```yaml
risk_management:
  max_position_percentage: 20.0   # Max 20% of balance in one position
  min_sol_balance: 0.05           # Stops if balance drops below 0.05 SOL
```

### Circuit Breaker
```yaml
risk_management:
  max_consecutive_losses: 5       # Pauses after 5 losses in a row
```

## Configuration Details

### Copy Trading Settings

Located in `config/bot_config.yaml`:

```yaml
copy_trading:
  target_wallets:
    - address: "54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy"
      name: "Primary Target"
      copy_percentage: 100        # 100% = same size, 50% = half size
      max_copy_amount_sol: 0.5    # Never copy more than 0.5 SOL
      min_copy_amount_sol: 0.01   # Ignore trades smaller than 0.01 SOL
      copy_sells: true            # Copy their sell orders too
      copy_buys: true             # Copy their buy orders
      delay_ms: 0                 # 0 = instant, 100 = 100ms delay
```

### Volume Spike Settings

```yaml
volume_spike_scalping:
  spike_threshold_percentage: 300  # 300% volume increase triggers entry
  entry_amount_sol: 0.1            # Trade 0.1 SOL per spike
  stop_loss_percentage: 15.0       # Exit at -15% loss
  quick_profit_target_percentage: 20.0  # Take profit at +20%
```

## Monitoring

### Console Output
The bot logs all activity in real-time:
- Trade signals detected
- Trades executed
- P&L updates
- Errors and warnings

### Telegram Notifications (Optional)

1. Create a bot with @BotFather on Telegram
2. Add to `.env`:
```
TELEGRAM_BOT_TOKEN=your_bot_token
TELEGRAM_CHAT_ID=your_chat_id
```

3. Enable in `config/bot_config.yaml`:
```yaml
telegram:
  enabled: true
```

## Troubleshooting

### "Insufficient balance" error
- Add more SOL to your wallet
- Minimum: 0.1 SOL for trading + 0.05 SOL for fees

### "Failed to connect to RPC"
- Check your RPC_URL in `.env`
- Free RPCs have rate limits - consider Helius/QuickNode

### "Transaction failed"
- Network congestion - the bot will retry automatically
- Increase priority_fee_lamports in config

### Copy trading not detecting trades
- Verify WebSocket URL is correct
- Check target wallet is actively trading
- Monitor logs for connection errors

## Performance Tips

1. **Use a premium RPC** (Helius, QuickNode) for:
   - Faster transaction confirmations
   - Higher rate limits
   - Better WebSocket reliability

2. **Enable Jito** for MEV protection:
```yaml
copy_trading:
  execution:
    use_jito: true
```

3. **Adjust priority fees** during high congestion:
```yaml
trading:
  priority_fee_lamports: 200000  # Increase for faster execution
```

## What Each Module Does

```
main.rs              - Starts both bots based on config
copy_trader/         - Wallet following & copying
  ├── follower.rs    - WebSocket + RPC monitoring
  ├── decoder.rs     - DFlow/OkxDex transaction parsing
  ├── signal.rs      - Trade signal filtering
  └── executor.rs    - Copy trade execution
  
trading/             - Trade execution engine
  ├── executor.rs    - Pump.fun buy/sell
  └── strategies/    - Trading strategies
  
pumpfun/             - Pump.fun integration
  ├── bonding_curve.rs - Price calculations
  ├── instructions.rs  - Transaction building
  └── constants.rs     - Program IDs
  
risk/                - Risk management
  └── manager.rs     - Stop-loss, limits, circuit breaker
  
monitoring/          - Alerts and logging
  ├── logger.rs      - File logging
  └── alerts.rs      - Telegram notifications
```

## Examples

See `examples/` folder for standalone demos:
- `copy_trader_demo.rs` - Simple copy trading example
- `pumpfun_demo.rs` - Direct pump.fun trading

## Next Steps

1. ✅ Test in dry-run mode
2. ✅ Start with small amounts (0.01-0.1 SOL)
3. ✅ Monitor for a few hours
4. ✅ Gradually increase position sizes
5. ✅ Enable Telegram notifications
6. ✅ Review and adjust risk parameters

## Support

Check the logs in `./logs/` directory for detailed information.

The bot will create:
- `trading_bot.log` - Main log file
- Rotates automatically at 100MB

---

**⚠️ IMPORTANT DISCLAIMERS:**

- Trading crypto is risky - only use funds you can afford to lose
- Start with DRY_RUN=true to test without risk
- Use small amounts initially
- Monitor the bot actively
- This is experimental software - use at your own risk
- Never commit your `.env` file with private keys
