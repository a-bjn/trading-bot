use crate::pumpfun::{
    constants::*, BondingCurve, BondingCurveCalculator, BondingCurveState, PumpFunError,
    PumpFunResult, Quote,
};
use borsh::BorshDeserialize;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};
use std::str::FromStr;

/// Get the bonding curve PDA for a given mint
/// 
/// # Arguments
/// * `mint` - The token mint address
/// 
/// # Returns
/// * Tuple of (bonding_curve_pda, bump_seed)
pub fn get_bonding_curve_pda(mint: &Pubkey) -> (Pubkey, u8) {
    let program_id = get_program_id();
    
    Pubkey::find_program_address(
        &[b"bonding-curve", mint.as_ref()],
        &program_id,
    )
}

/// Get the associated bonding curve token account
/// 
/// This is the token account owned by the bonding curve that holds the tokens
/// 
/// # Arguments
/// * `bonding_curve` - The bonding curve PDA
/// * `mint` - The token mint address
pub fn get_associated_bonding_curve_address(bonding_curve: &Pubkey, mint: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(bonding_curve, mint)
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
    let (bonding_curve_pda, _bump) = get_bonding_curve_pda(mint);

    let account = rpc_client
        .get_account_with_commitment(&bonding_curve_pda, CommitmentConfig::confirmed())
        .map_err(|e| PumpFunError::AccountFetchError(e.to_string()))?
        .value
        .ok_or_else(|| {
            PumpFunError::BondingCurveNotFound(mint.to_string())
        })?;

    // Deserialize using Borsh (skip 8-byte discriminator)
    let data = if account.data.len() > 8 {
        &account.data[8..]
    } else {
        &account.data
    };
    
    BondingCurve::try_from_slice(data)
        .map_err(|_| PumpFunError::DeserializationError)
}

/// Fetch bonding curve state with metadata
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
pub fn fetch_bonding_curve_state(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> PumpFunResult<BondingCurveState> {
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    let (bonding_curve_pda, _bump) = get_bonding_curve_pda(mint);

    let current_price = BondingCurveCalculator::get_current_price(&bonding_curve);
    let market_cap_sol = BondingCurveCalculator::get_market_cap_sol(&bonding_curve);

    Ok(BondingCurveState {
        bonding_curve,
        mint: *mint,
        address: bonding_curve_pda,
        current_price,
        market_cap_sol,
    })
}

/// Check if a token has migrated to Raydium
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
/// 
/// # Returns
/// * True if the bonding curve is complete (migrated)
pub fn is_migrated(rpc_client: &RpcClient, mint: &Pubkey) -> PumpFunResult<bool> {
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    Ok(bonding_curve.complete)
}

/// Get user's token balance
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `token_account` - The token account address
pub fn get_token_balance(
    rpc_client: &RpcClient,
    token_account: &Pubkey,
) -> PumpFunResult<u64> {
    let account = rpc_client
        .get_account_with_commitment(token_account, CommitmentConfig::confirmed())
        .map_err(|e| PumpFunError::AccountFetchError(e.to_string()))?
        .value
        .ok_or(PumpFunError::TokenAccountNotFound)?;

    // Parse SPL token account (amount is at offset 64, 8 bytes)
    if account.data.len() < 72 {
        return Err(PumpFunError::DeserializationError);
    }

    let amount_bytes: [u8; 8] = account.data[64..72]
        .try_into()
        .map_err(|_| PumpFunError::DeserializationError)?;

    Ok(u64::from_le_bytes(amount_bytes))
}

/// Get user's SOL balance
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `wallet` - The wallet address
pub fn get_sol_balance(rpc_client: &RpcClient, wallet: &Pubkey) -> PumpFunResult<u64> {
    rpc_client
        .get_balance_with_commitment(wallet, CommitmentConfig::confirmed())
        .map(|response| response.value)
        .map_err(|e| PumpFunError::AccountFetchError(e.to_string()))
}

/// Check if a token account exists
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `token_account` - The token account address
pub fn token_account_exists(
    rpc_client: &RpcClient,
    token_account: &Pubkey,
) -> PumpFunResult<bool> {
    match rpc_client.get_account_with_commitment(token_account, CommitmentConfig::confirmed()) {
        Ok(response) => Ok(response.value.is_some()),
        Err(_) => Ok(false),
    }
}

/// Get the associated token address for a wallet and mint
/// 
/// # Arguments
/// * `wallet` - The wallet address
/// * `mint` - The token mint address
pub fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(wallet, mint)
}

/// Parse a Pubkey from a string
/// 
/// # Arguments
/// * `s` - The string to parse
pub fn parse_pubkey(s: &str) -> PumpFunResult<Pubkey> {
    Pubkey::from_str(s)
        .map_err(|_| PumpFunError::InvalidConfiguration(format!("Invalid pubkey: {}", s)))
}

/// Convert lamports to SOL
pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

/// Convert SOL to lamports
pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000.0) as u64
}

/// Convert tokens with decimals to base units
pub fn tokens_to_base_units(tokens: f64, decimals: u8) -> u64 {
    (tokens * 10f64.powi(decimals as i32)) as u64
}

/// Convert base units to tokens with decimals
pub fn base_units_to_tokens(base_units: u64, decimals: u8) -> f64 {
    base_units as f64 / 10f64.powi(decimals as i32)
}

/// Calculate token output for SOL input (buy operation)
/// 
/// Convenience wrapper around BondingCurveCalculator::calculate_buy_quote
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
/// * `sol_amount_in` - Amount of SOL to spend (in lamports)
/// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
/// 
/// # Returns
/// * Quote with token output, fees, and price impact
pub fn calculate_token_output_for_sol(
    rpc_client: &RpcClient,
    mint: &Pubkey,
    sol_amount_in: u64,
    slippage_bps: u64,
) -> PumpFunResult<Quote> {
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    BondingCurveCalculator::calculate_buy_quote(&bonding_curve, sol_amount_in, slippage_bps)
        .map_err(|e| PumpFunError::InvalidConfiguration(e.to_string()))
}

/// Calculate SOL input required for token output (inverse buy)
/// 
/// Convenience wrapper around BondingCurveCalculator::calculate_sol_input_for_tokens
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
/// * `token_amount_out` - Desired token output (before fees)
/// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
/// 
/// # Returns
/// * Quote with SOL input required, fees, and price impact
pub fn calculate_sol_input_for_token_output(
    rpc_client: &RpcClient,
    mint: &Pubkey,
    token_amount_out: u64,
    slippage_bps: u64,
) -> PumpFunResult<Quote> {
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    BondingCurveCalculator::calculate_sol_input_for_tokens(&bonding_curve, token_amount_out, slippage_bps)
        .map_err(|e| PumpFunError::InvalidConfiguration(e.to_string()))
}

/// Calculate SOL output for token input (sell operation)
/// 
/// Convenience wrapper around BondingCurveCalculator::calculate_sell_quote
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
/// * `token_amount_in` - Amount of tokens to sell
/// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
/// 
/// # Returns
/// * Quote with SOL output, fees, and price impact
pub fn calculate_sol_output_for_tokens(
    rpc_client: &RpcClient,
    mint: &Pubkey,
    token_amount_in: u64,
    slippage_bps: u64,
) -> PumpFunResult<Quote> {
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    BondingCurveCalculator::calculate_sell_quote(&bonding_curve, token_amount_in, slippage_bps)
        .map_err(|e| PumpFunError::InvalidConfiguration(e.to_string()))
}

/// Calculate token input required for SOL output (inverse sell)
/// 
/// Convenience wrapper around BondingCurveCalculator::calculate_token_input_for_sol
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
/// * `sol_amount_out` - Desired SOL output (before fees)
/// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
/// 
/// # Returns
/// * Quote with token input required, fees, and price impact
pub fn calculate_token_input_for_sol_output(
    rpc_client: &RpcClient,
    mint: &Pubkey,
    sol_amount_out: u64,
    slippage_bps: u64,
) -> PumpFunResult<Quote> {
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    BondingCurveCalculator::calculate_token_input_for_sol(&bonding_curve, sol_amount_out, slippage_bps)
        .map_err(|e| PumpFunError::InvalidConfiguration(e.to_string()))
}

/// Get bonding curve progress percentage
/// 
/// Convenience wrapper around BondingCurveCalculator::get_progress_percentage
/// 
/// # Arguments
/// * `rpc_client` - The Solana RPC client
/// * `mint` - The token mint address
/// 
/// # Returns
/// * Progress percentage (0.0 to 100.0)
pub fn get_bonding_curve_progress(
    rpc_client: &RpcClient,
    mint: &Pubkey,
) -> PumpFunResult<f64> {
    use crate::pumpfun::constants::INITIAL_REAL_TOKEN_RESERVES;
    let bonding_curve = fetch_bonding_curve(rpc_client, mint)?;
    Ok(BondingCurveCalculator::get_progress_percentage(
        &bonding_curve,
        INITIAL_REAL_TOKEN_RESERVES,
    ))
}

/// Derive associated bonding curve address
/// 
/// This is a convenience wrapper that combines PDA derivation with ATA derivation
/// 
/// # Arguments
/// * `mint` - The token mint address
/// 
/// # Returns
/// * Tuple of (bonding_curve_pda, associated_bonding_curve_token_account)
pub fn derive_bonding_curve_addresses(mint: &Pubkey) -> (Pubkey, Pubkey) {
    let (bonding_curve_pda, _bump) = get_bonding_curve_pda(mint);
    let associated_bonding_curve = get_associated_bonding_curve_address(&bonding_curve_pda, mint);
    (bonding_curve_pda, associated_bonding_curve)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bonding_curve_pda() {
        let mint = Pubkey::new_unique();
        let (pda, bump) = get_bonding_curve_pda(&mint);
        
        assert_ne!(pda, Pubkey::default());
        assert!(bump <= 255);
    }

    #[test]
    fn test_lamports_to_sol() {
        assert_eq!(lamports_to_sol(1_000_000_000), 1.0);
        assert_eq!(lamports_to_sol(500_000_000), 0.5);
    }

    #[test]
    fn test_sol_to_lamports() {
        assert_eq!(sol_to_lamports(1.0), 1_000_000_000);
        assert_eq!(sol_to_lamports(0.5), 500_000_000);
    }

    #[test]
    fn test_tokens_conversion() {
        let tokens = 1000.0;
        let decimals = 6;
        let base = tokens_to_base_units(tokens, decimals);
        let back = base_units_to_tokens(base, decimals);
        
        assert_eq!(base, 1_000_000_000);
        assert_eq!(back, tokens);
    }

    #[test]
    fn test_parse_pubkey() {
        let valid = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
        let result = parse_pubkey(valid);
        assert!(result.is_ok());

        let invalid = "not-a-pubkey";
        let result = parse_pubkey(invalid);
        assert!(result.is_err());
    }
}
