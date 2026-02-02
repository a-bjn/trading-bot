use crate::pumpfun::{
    types::{BondingCurve, BondingCurveState, PumpFunToken},
    BondingCurveCalculator, PumpFunResult,
};
use anyhow::Result;
use reqwest::Client;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};
use borsh::BorshDeserialize;

/// HTTP client for interacting with pump.fun API
pub struct PumpFunClient {
    http_client: Client,
    api_url: String,
}

impl PumpFunClient {
    /// Create a new pump.fun API client
    pub fn new(api_url: String) -> Self {
        Self {
            http_client: Client::new(),
            api_url,
        }
    }

    /// Get token information from pump.fun API
    /// 
    /// # Arguments
    /// * `mint` - The token mint address
    pub async fn get_token_info(&self, mint: &str) -> Result<PumpFunToken> {
        let url = format!("{}/coins/{}", self.api_url, mint);
        let response = self.http_client.get(&url).send().await?;
        let token = response.json::<PumpFunToken>().await?;
        Ok(token)
    }

    /// Monitor new tokens from pump.fun
    pub async fn monitor_new_tokens(&self) -> Result<()> {
        // TODO: Implement WebSocket or polling for new tokens
        Ok(())
    }

    /// Get trending tokens
    pub async fn get_trending_tokens(&self) -> Result<Vec<PumpFunToken>> {
        let url = format!("{}/coins/trending", self.api_url);
        let response = self.http_client.get(&url).send().await?;
        let tokens = response.json::<Vec<PumpFunToken>>().await?;
        Ok(tokens)
    }

    /// Get recently created tokens
    pub async fn get_recent_tokens(&self, limit: Option<usize>) -> Result<Vec<PumpFunToken>> {
        let limit = limit.unwrap_or(50);
        let url = format!("{}/coins/recent?limit={}", self.api_url, limit);
        let response = self.http_client.get(&url).send().await?;
        let tokens = response.json::<Vec<PumpFunToken>>().await?;
        Ok(tokens)
    }

    /// Fetch bonding curve state from RPC
    /// 
    /// # Arguments
    /// * `rpc_client` - The Solana RPC client
    /// * `mint` - The token mint address
    /// 
    /// # Returns
    /// * BondingCurve account data
    pub fn fetch_bonding_curve(
        rpc_client: &RpcClient,
        mint: &Pubkey,
    ) -> PumpFunResult<BondingCurve> {
        crate::pumpfun::fetch_bonding_curve(rpc_client, mint)
    }

    /// Fetch bonding curve state with metadata
    /// 
    /// # Arguments
    /// * `rpc_client` - The Solana RPC client
    /// * `mint` - The token mint address
    /// 
    /// # Returns
    /// * BondingCurveState with price and market cap
    pub fn fetch_bonding_curve_state(
        rpc_client: &RpcClient,
        mint: &Pubkey,
    ) -> PumpFunResult<BondingCurveState> {
        crate::pumpfun::fetch_bonding_curve_state(rpc_client, mint)
    }

    /// Get current bonding curve price
    /// 
    /// # Arguments
    /// * `rpc_client` - The Solana RPC client
    /// * `mint` - The token mint address
    /// 
    /// # Returns
    /// * Current price in SOL per token
    pub fn get_current_price(
        rpc_client: &RpcClient,
        mint: &Pubkey,
    ) -> PumpFunResult<f64> {
        let bonding_curve = Self::fetch_bonding_curve(rpc_client, mint)?;
        Ok(BondingCurveCalculator::get_current_price(&bonding_curve))
    }

    /// Get bonding curve progress percentage
    /// 
    /// # Arguments
    /// * `rpc_client` - The Solana RPC client
    /// * `mint` - The token mint address
    /// * `initial_real_token_reserves` - Initial real token reserves
    /// 
    /// # Returns
    /// * Progress percentage (0.0 to 100.0)
    pub fn get_progress_percentage(
        rpc_client: &RpcClient,
        mint: &Pubkey,
        initial_real_token_reserves: u64,
    ) -> PumpFunResult<f64> {
        let bonding_curve = Self::fetch_bonding_curve(rpc_client, mint)?;
        Ok(BondingCurveCalculator::get_progress_percentage(
            &bonding_curve,
            initial_real_token_reserves,
        ))
    }

    /// Check if bonding curve has migrated to Raydium
    /// 
    /// # Arguments
    /// * `rpc_client` - The Solana RPC client
    /// * `mint` - The token mint address
    /// 
    /// # Returns
    /// * True if migrated, false otherwise
    pub fn is_migrated(
        rpc_client: &RpcClient,
        mint: &Pubkey,
    ) -> PumpFunResult<bool> {
        let bonding_curve = Self::fetch_bonding_curve(rpc_client, mint)?;
        Ok(bonding_curve.complete)
    }
}
