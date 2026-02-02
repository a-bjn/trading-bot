use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// Token information from pump.fun API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpFunToken {
    pub mint: String,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub image_uri: String,
    pub creator: String,
    pub bonding_curve_info: BondingCurveInfo,
    pub created_timestamp: i64,
}

/// Bonding curve information for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondingCurveInfo {
    pub progress_percentage: f64,
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
}

/// On-chain bonding curve account structure
/// This is the actual account data stored on Solana
#[derive(Debug, Clone, Copy, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct BondingCurve {
    /// Virtual token reserves (includes initial virtual liquidity)
    pub virtual_token_reserves: u64,
    /// Virtual SOL reserves (includes initial virtual liquidity)
    pub virtual_sol_reserves: u64,
    /// Real token reserves (actual tokens available to buy)
    pub real_token_reserves: u64,
    /// Real SOL reserves (actual SOL collected from buys)
    pub real_sol_reserves: u64,
    /// Total supply of the token
    pub token_total_supply: u64,
    /// Whether the bonding curve is complete (migrated to Raydium)
    pub complete: bool,
}

/// Bonding curve state with metadata
#[derive(Debug, Clone)]
pub struct BondingCurveState {
    pub bonding_curve: BondingCurve,
    pub mint: Pubkey,
    pub address: Pubkey,
    pub current_price: f64,
    pub market_cap_sol: f64,
}

/// Global state account for pump.fun program
#[derive(Debug, Clone, Copy, BorshSerialize, BorshDeserialize)]
pub struct GlobalState {
    pub initialized: bool,
    pub authority: Pubkey,
    pub fee_recipient: Pubkey,
    pub initial_virtual_token_reserves: u64,
    pub initial_virtual_sol_reserves: u64,
    pub initial_real_token_reserves: u64,
    pub token_total_supply: u64,
    pub fee_basis_points: u64,
}

/// Quote for a trade
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Quote {
    /// Amount in (SOL for buys, tokens for sells)
    pub amount_in: u64,
    /// Amount out before fees
    pub amount_out: u64,
    /// Amount out after fees
    pub amount_out_after_fees: u64,
    /// Fee amount
    pub fee_amount: u64,
    /// Price impact in basis points
    pub price_impact_bps: u64,
    /// Effective price (SOL per token)
    pub effective_price: f64,
}

/// Trade direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeDirection {
    Buy,
    Sell,
}

/// Trade parameters
#[derive(Debug, Clone, Copy)]
pub struct TradeParams {
    pub direction: TradeDirection,
    pub amount_in: u64,
    pub slippage_bps: u64,
}

/// Token metadata structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}
