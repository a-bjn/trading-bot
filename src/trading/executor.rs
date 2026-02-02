use anyhow::{Context, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::config::TradingConfig;
use crate::pumpfun::{
    bonding_curve::BondingCurveCalculator,
    helpers::{get_associated_bonding_curve_address, get_bonding_curve_pda},
    instructions::{build_buy_transaction_instructions, build_sell_transaction_instructions},
};
use crate::solana::{
    client::SolanaClient,
    error::{Result as SolanaResult, SolanaError},
    jito::JitoClient,
    wallet::Wallet,
};

/// Trade executor for pump.fun swaps
pub struct TradeExecutor {
    wallet: Arc<Wallet>,
    solana_client: Arc<SolanaClient>,
    jito_client: Option<Arc<JitoClient>>,
    config: TradingConfig,
}

impl TradeExecutor {
    /// Create a new trade executor
    pub fn new(
        wallet: Arc<Wallet>,
        solana_client: Arc<SolanaClient>,
        jito_client: Option<Arc<JitoClient>>,
        config: TradingConfig,
    ) -> Self {
        Self {
            wallet,
            solana_client,
            jito_client,
            config,
        }
    }

    /// Execute a buy order
    /// 
    /// # Arguments
    /// * `token_mint` - Token mint address as string
    /// * `amount_sol` - Amount of SOL to spend
    /// 
    /// # Returns
    /// * Transaction signature
    pub async fn execute_buy(&self, token_mint: &str, amount_sol: f64) -> Result<Signature> {
        let mint = token_mint
            .parse::<Pubkey>()
            .context("Invalid token mint address")?;

        self.execute_buy_pubkey(&mint, amount_sol).await
    }

    /// Execute a buy order with Pubkey
    pub async fn execute_buy_pubkey(&self, token_mint: &Pubkey, amount_sol: f64) -> Result<Signature> {
        info!("Executing buy: {} SOL for token {}", amount_sol, token_mint);

        // Validate amount
        if amount_sol < self.config.min_trade_size_sol {
            return Err(anyhow::anyhow!(
                "Amount {} SOL is below minimum {} SOL",
                amount_sol,
                self.config.min_trade_size_sol
            ));
        }

        if amount_sol > self.config.max_position_size_sol {
            return Err(anyhow::anyhow!(
                "Amount {} SOL exceeds maximum position size {} SOL",
                amount_sol,
                self.config.max_position_size_sol
            ));
        }

        // Convert SOL to lamports
        let sol_amount_lamports = (amount_sol * 1_000_000_000.0) as u64;

        // Check balance
        self.wallet
            .verify_balance(sol_amount_lamports + 50_000) // Add buffer for fees
            .await
            .map_err(|e| anyhow::anyhow!("Insufficient balance: {}", e))?;

        // Get bonding curve PDA
        let (bonding_curve, _bump) = get_bonding_curve_pda(token_mint);
        let associated_bonding_curve = get_associated_bonding_curve_address(&bonding_curve, token_mint);

        // Get buyer's token account
        let buyer_token_account = self.wallet.get_associated_token_address(token_mint);

        // Fetch bonding curve state to calculate quote
        let rpc_client = self.solana_client.rpc();
        let bonding_curve_state = crate::pumpfun::helpers::fetch_bonding_curve_state(rpc_client, &token_mint)
            .map_err(|e| anyhow::anyhow!("Failed to fetch bonding curve: {}", e))?;

        // Check if migrated
        if bonding_curve_state.bonding_curve.complete {
            return Err(anyhow::anyhow!("Token has migrated to Raydium, cannot trade on pump.fun"));
        }

        // Calculate quote with slippage
        let slippage_bps = self.config.slippage_bps as u64;
        let quote = BondingCurveCalculator::calculate_buy_quote(
            &bonding_curve_state.bonding_curve,
            sol_amount_lamports,
            slippage_bps,
        )
        .map_err(|e| anyhow::anyhow!("Failed to calculate quote: {}", e))?;

        let min_tokens_out = quote.amount_out_after_fees;

        info!(
            "Buy quote: {} SOL -> {} tokens (min: {}), price impact: {} bps",
            amount_sol,
            quote.amount_out_after_fees as f64 / 1e6,
            min_tokens_out as f64 / 1e6,
            quote.price_impact_bps
        );

        // Check if token account exists, create if needed
        let create_ata_ix = self.wallet
            .create_token_account_if_needed(token_mint)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check token account: {}", e))?;

        // Calculate priority fee
        let priority_fee = self.calculate_priority_fee().await?;

        // Build instructions
        let mut instructions = build_buy_transaction_instructions(
            self.wallet.pubkey(),
            *token_mint,
            bonding_curve,
            associated_bonding_curve,
            buyer_token_account,
            sol_amount_lamports,
            min_tokens_out,
            Some(priority_fee),
        )
        .map_err(|e| anyhow::anyhow!("Failed to build buy instructions: {}", e))?;

        // Prepend create ATA instruction if needed
        if let Some(create_ix) = create_ata_ix {
            instructions.insert(0, create_ix);
        }

        // Execute with retry
        self.execute_transaction_with_retry(instructions, true).await
    }

    /// Execute a sell order
    /// 
    /// # Arguments
    /// * `token_mint` - Token mint address as string
    /// * `amount_tokens` - Amount of tokens to sell (in token units, not base units)
    /// 
    /// # Returns
    /// * Transaction signature
    pub async fn execute_sell(&self, token_mint: &str, amount_tokens: f64) -> Result<Signature> {
        let mint = token_mint
            .parse::<Pubkey>()
            .context("Invalid token mint address")?;

        self.execute_sell_pubkey(&mint, amount_tokens).await
    }

    /// Execute a sell order with Pubkey
    pub async fn execute_sell_pubkey(&self, token_mint: &Pubkey, amount_tokens: f64) -> Result<Signature> {
        info!("Executing sell: {} tokens for {}", amount_tokens, token_mint);

        // Convert tokens to base units (pump.fun uses 6 decimals)
        let token_amount_base_units = (amount_tokens * 1_000_000.0) as u64;

        if token_amount_base_units == 0 {
            return Err(anyhow::anyhow!("Token amount must be greater than 0"));
        }

        // Get bonding curve PDA
        let (bonding_curve, _bump) = get_bonding_curve_pda(token_mint);
        let associated_bonding_curve = get_associated_bonding_curve_address(&bonding_curve, token_mint);

        // Get seller's token account
        let seller_token_account = self.wallet.get_associated_token_address(token_mint);

        // Check token balance
        let token_balance = self.get_token_balance(&seller_token_account).await?;
        if token_balance < token_amount_base_units {
            return Err(anyhow::anyhow!(
                "Insufficient token balance: have {}, need {}",
                token_balance as f64 / 1e6,
                amount_tokens
            ));
        }

        // Fetch bonding curve state to calculate quote
        let rpc_client = self.solana_client.rpc();
        let bonding_curve_state = crate::pumpfun::helpers::fetch_bonding_curve_state(rpc_client, &token_mint)
            .map_err(|e| anyhow::anyhow!("Failed to fetch bonding curve: {}", e))?;

        // Check if migrated
        if bonding_curve_state.bonding_curve.complete {
            return Err(anyhow::anyhow!("Token has migrated to Raydium, cannot trade on pump.fun"));
        }

        // Calculate quote with slippage
        let slippage_bps = self.config.slippage_bps as u64;
        let quote = BondingCurveCalculator::calculate_sell_quote(
            &bonding_curve_state.bonding_curve,
            token_amount_base_units,
            slippage_bps,
        )
        .map_err(|e| anyhow::anyhow!("Failed to calculate quote: {}", e))?;

        let min_sol_out = quote.amount_out_after_fees;

        info!(
            "Sell quote: {} tokens -> {} SOL (min: {}), price impact: {} bps",
            amount_tokens,
            quote.amount_out_after_fees as f64 / 1e9,
            min_sol_out as f64 / 1e9,
            quote.price_impact_bps
        );

        // Calculate priority fee
        let priority_fee = self.calculate_priority_fee().await?;

        // Build instructions
        let instructions = build_sell_transaction_instructions(
            self.wallet.pubkey(),
            *token_mint,
            bonding_curve,
            associated_bonding_curve,
            seller_token_account,
            token_amount_base_units,
            min_sol_out,
            Some(priority_fee),
        )
        .map_err(|e| anyhow::anyhow!("Failed to build sell instructions: {}", e))?;

        // Execute with retry
        self.execute_transaction_with_retry(instructions, true).await
    }

    /// Execute transaction with retry logic and exponential backoff
    async fn execute_transaction_with_retry(
        &self,
        instructions: Vec<solana_sdk::instruction::Instruction>,
        use_jito: bool,
    ) -> Result<Signature> {
        let max_retries = self.config.max_retry_attempts;
        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.execute_transaction(&instructions, use_jito && attempt == 0).await {
                Ok(sig) => {
                    info!("Transaction executed successfully: {}", sig);
                    return Ok(sig);
                }
                Err(e) => {
                    warn!("Transaction attempt {}/{} failed: {}", attempt + 1, max_retries, e);
                    last_error = Some(e);

                    if attempt < max_retries - 1 {
                        // Exponential backoff: 100ms, 200ms, 400ms, etc.
                        let delay_ms = 100 * (2_u64.pow(attempt));
                        debug!("Retrying after {}ms", delay_ms);
                        sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retry attempts failed")))
    }

    /// Execute a single transaction
    async fn execute_transaction(
        &self,
        instructions: &[solana_sdk::instruction::Instruction],
        use_jito: bool,
    ) -> Result<Signature> {
        // Get recent blockhash
        let recent_blockhash = self.solana_client
            .get_latest_blockhash()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get blockhash: {}", e))?;

        // Build transaction
        let mut transaction = Transaction::new_with_payer(instructions, Some(&self.wallet.pubkey()));
        transaction.message.recent_blockhash = recent_blockhash;

        // Sign transaction
        self.wallet
            .sign_transaction(&mut transaction)
            .map_err(|e| anyhow::anyhow!("Failed to sign transaction: {}", e))?;

        // Simulate if configured
        if self.config.slippage_bps > 0 {
            // Simulation is optional, but we'll skip it for speed in production
            // Uncomment if needed:
            // self.solana_client.simulate_transaction(&transaction).await
            //     .map_err(|e| anyhow::anyhow!("Simulation failed: {}", e))?;
        }

        // Submit transaction
        if use_jito && self.jito_client.is_some() {
            // Try Jito first
            match self.submit_via_jito(transaction.clone()).await {
                Ok(sig) => return Ok(sig),
                Err(e) => {
                    warn!("Jito submission failed, falling back to RPC: {}", e);
                    // Fall through to RPC submission
                }
            }
        }

        // Submit via regular RPC
        let signature = self.solana_client
            .send_transaction(&transaction)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send transaction: {}", e))?;

        // Monitor confirmation
        self.monitor_confirmation(&signature).await?;

        Ok(signature)
    }

    /// Submit transaction via Jito bundle
    async fn submit_via_jito(&self, transaction: Transaction) -> Result<Signature> {
        let jito_client = self.jito_client.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Jito client not configured"))?;

        let signature = transaction.signatures[0];

        // Submit bundle (Jito client handles tip transaction)
        jito_client
            .send_bundle_with_tip(&self.wallet, vec![transaction])
            .await
            .map_err(|e| anyhow::anyhow!("Jito bundle submission failed: {}", e))?;

        info!("Transaction submitted via Jito: {}", signature);

        // Monitor confirmation
        self.monitor_confirmation(&signature).await?;

        Ok(signature)
    }

    /// Monitor transaction confirmation
    async fn monitor_confirmation(&self, signature: &Signature) -> Result<()> {
        let timeout = Duration::from_secs(self.config.transaction_timeout_seconds);
        
        self.solana_client
            .confirm_transaction(signature, timeout)
            .await
            .map_err(|e| anyhow::anyhow!("Transaction confirmation failed: {}", e))?;

        Ok(())
    }

    /// Calculate priority fee based on config and network conditions
    async fn calculate_priority_fee(&self) -> Result<u64> {
        let base_fee = self.config.priority_fee_lamports;
        let max_fee = self.config.max_priority_fee_lamports;

        // Try to get recommended fee from network
        let recommended_fee = self.solana_client
            .get_recommended_fee(&[])
            .await
            .unwrap_or(base_fee);

        // Use the higher of base fee or recommended, but cap at max
        let fee = base_fee.max(recommended_fee).min(max_fee);

        debug!("Calculated priority fee: {} micro-lamports per CU", fee);
        Ok(fee)
    }

    /// Get token balance for a token account
    async fn get_token_balance(&self, token_account: &Pubkey) -> Result<u64> {
        let account = self.solana_client
            .get_account_optional(token_account)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get token account: {}", e))?;

        match account {
            Some(acc) => {
                // Parse SPL token account (amount is at offset 64, 8 bytes)
                if acc.data.len() < 72 {
                    return Err(anyhow::anyhow!("Invalid token account data"));
                }

                let amount_bytes: [u8; 8] = acc.data[64..72]
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Failed to parse token amount"))?;

                Ok(u64::from_le_bytes(amount_bytes))
            }
            None => Ok(0),
        }
    }
}
