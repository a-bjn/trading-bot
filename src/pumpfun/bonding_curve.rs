use crate::pumpfun::{BondingCurve, Quote, TradeDirection, TradeParams, FEE_BASIS_POINTS};
use anyhow::{anyhow, Result};

/// Bonding curve calculator implementing constant product formula (x * y = k)
/// 
/// Pump.fun uses a Uniswap V2-style constant product bonding curve with virtual reserves.
/// 
/// Key Formulas:
/// - Constant Product: virtual_sol * virtual_tokens = k (constant)
/// - Buy: tokens_out = virtual_tokens - (k / (virtual_sol + sol_in))
/// - Sell: sol_out = virtual_sol - (k / (virtual_tokens + tokens_in))
/// - Fee: 1% (100 basis points) deducted from output
/// 
/// Virtual reserves ensure liquidity before migration to Raydium:
/// - Initial virtual reserves: 30 SOL and 1.073M tokens
/// - Real reserves start at: 0 SOL and 793.1M tokens
/// - Migration occurs when real_token_reserves reaches 0
pub struct BondingCurveCalculator;

impl BondingCurveCalculator {
    /// Calculate tokens out for a given SOL input (buy operation)
    /// 
    /// Uses constant product formula: (virtual_sol + sol_in) * (virtual_tokens - tokens_out) = k
    /// Where k = virtual_sol * virtual_tokens
    /// 
    /// # Arguments
    /// * `bonding_curve` - Current bonding curve state
    /// * `sol_amount_in` - Amount of SOL to spend (in lamports)
    /// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
    /// 
    /// # Returns
    /// * Quote with amount out, fees, and price impact
    pub fn calculate_buy_quote(
        bonding_curve: &BondingCurve,
        sol_amount_in: u64,
        slippage_bps: u64,
    ) -> Result<Quote> {
        if bonding_curve.complete {
            return Err(anyhow!("Bonding curve has migrated to Raydium"));
        }

        if sol_amount_in == 0 {
            return Err(anyhow!("SOL amount must be greater than 0"));
        }

        // Calculate constant product k = x * y
        let k = bonding_curve
            .virtual_sol_reserves
            .checked_mul(bonding_curve.virtual_token_reserves)
            .ok_or_else(|| anyhow!("Overflow calculating k"))?;

        // Calculate new SOL reserves after input
        let new_virtual_sol_reserves = bonding_curve
            .virtual_sol_reserves
            .checked_add(sol_amount_in)
            .ok_or_else(|| anyhow!("Overflow calculating new SOL reserves"))?;

        // Calculate new token reserves: new_tokens = k / new_sol
        let new_virtual_token_reserves = k
            .checked_div(new_virtual_sol_reserves)
            .ok_or_else(|| anyhow!("Division error calculating new token reserves"))?;

        // Tokens out = old_reserves - new_reserves
        let tokens_out = bonding_curve
            .virtual_token_reserves
            .checked_sub(new_virtual_token_reserves)
            .ok_or_else(|| anyhow!("Insufficient token reserves"))?;

        // Check if there are enough real tokens available
        if tokens_out > bonding_curve.real_token_reserves {
            return Err(anyhow!(
                "Insufficient real token reserves. Available: {}, Required: {}",
                bonding_curve.real_token_reserves,
                tokens_out
            ));
        }

        // Calculate fee (1% of tokens out)
        let fee_amount = tokens_out
            .checked_mul(FEE_BASIS_POINTS)
            .ok_or_else(|| anyhow!("Overflow calculating fee"))?
            .checked_div(10_000)
            .unwrap_or(0);

        let tokens_out_after_fees = tokens_out
            .checked_sub(fee_amount)
            .ok_or_else(|| anyhow!("Fee exceeds output amount"))?;

        // Apply slippage tolerance
        let min_amount_out = Self::apply_slippage(tokens_out_after_fees, slippage_bps)?;

        // Calculate price impact
        let price_impact_bps = Self::calculate_price_impact(
            bonding_curve.virtual_token_reserves,
            tokens_out,
        )?;

        // Calculate effective price (SOL per token)
        let effective_price = if tokens_out_after_fees > 0 {
            (sol_amount_in as f64) / (tokens_out_after_fees as f64)
        } else {
            0.0
        };

        Ok(Quote {
            amount_in: sol_amount_in,
            amount_out: tokens_out,
            amount_out_after_fees: tokens_out_after_fees,
            fee_amount,
            price_impact_bps,
            effective_price,
        })
    }

    /// Calculate SOL out for a given token input (sell operation)
    /// 
    /// Uses constant product formula: (virtual_tokens + tokens_in) * (virtual_sol - sol_out) = k
    /// 
    /// # Arguments
    /// * `bonding_curve` - Current bonding curve state
    /// * `token_amount_in` - Amount of tokens to sell
    /// * `slippage_bps` - Slippage tolerance in basis points (100 = 1%)
    pub fn calculate_sell_quote(
        bonding_curve: &BondingCurve,
        token_amount_in: u64,
        slippage_bps: u64,
    ) -> Result<Quote> {
        if bonding_curve.complete {
            return Err(anyhow!("Bonding curve has migrated to Raydium"));
        }

        if token_amount_in == 0 {
            return Err(anyhow!("Token amount must be greater than 0"));
        }

        // Calculate constant product k = x * y
        let k = bonding_curve
            .virtual_sol_reserves
            .checked_mul(bonding_curve.virtual_token_reserves)
            .ok_or_else(|| anyhow!("Overflow calculating k"))?;

        // Calculate new token reserves after input
        let new_virtual_token_reserves = bonding_curve
            .virtual_token_reserves
            .checked_add(token_amount_in)
            .ok_or_else(|| anyhow!("Overflow calculating new token reserves"))?;

        // Calculate new SOL reserves: new_sol = k / new_tokens
        let new_virtual_sol_reserves = k
            .checked_div(new_virtual_token_reserves)
            .ok_or_else(|| anyhow!("Division error calculating new SOL reserves"))?;

        // SOL out = old_reserves - new_reserves
        let sol_out = bonding_curve
            .virtual_sol_reserves
            .checked_sub(new_virtual_sol_reserves)
            .ok_or_else(|| anyhow!("Insufficient SOL reserves"))?;

        // Check if there is enough real SOL available
        if sol_out > bonding_curve.real_sol_reserves {
            return Err(anyhow!(
                "Insufficient real SOL reserves. Available: {}, Required: {}",
                bonding_curve.real_sol_reserves,
                sol_out
            ));
        }

        // Calculate fee (1% of SOL out)
        let fee_amount = sol_out
            .checked_mul(FEE_BASIS_POINTS)
            .ok_or_else(|| anyhow!("Overflow calculating fee"))?
            .checked_div(10_000)
            .unwrap_or(0);

        let sol_out_after_fees = sol_out
            .checked_sub(fee_amount)
            .ok_or_else(|| anyhow!("Fee exceeds output amount"))?;

        // Apply slippage tolerance
        let min_amount_out = Self::apply_slippage(sol_out_after_fees, slippage_bps)?;

        // Calculate price impact
        let price_impact_bps = Self::calculate_price_impact(
            bonding_curve.virtual_sol_reserves,
            sol_out,
        )?;

        // Calculate effective price (SOL per token)
        let effective_price = if token_amount_in > 0 {
            (sol_out_after_fees as f64) / (token_amount_in as f64)
        } else {
            0.0
        };

        Ok(Quote {
            amount_in: token_amount_in,
            amount_out: sol_out,
            amount_out_after_fees: sol_out_after_fees,
            fee_amount,
            price_impact_bps,
            effective_price,
        })
    }

    /// Calculate price impact in basis points
    /// 
    /// Price impact = (amount_out / reserves) * 10000
    fn calculate_price_impact(reserves: u64, amount_out: u64) -> Result<u64> {
        if reserves == 0 {
            return Ok(0);
        }

        let impact = (amount_out as u128)
            .checked_mul(10_000)
            .ok_or_else(|| anyhow!("Overflow calculating price impact"))?
            .checked_div(reserves as u128)
            .unwrap_or(0);

        Ok(impact.min(u64::MAX as u128) as u64)
    }

    /// Apply slippage tolerance to minimum output amount
    /// 
    /// min_out = amount_out * (10000 - slippage_bps) / 10000
    fn apply_slippage(amount: u64, slippage_bps: u64) -> Result<u64> {
        if slippage_bps >= 10_000 {
            return Err(anyhow!("Slippage tolerance cannot be 100% or greater"));
        }

        let multiplier = 10_000u64
            .checked_sub(slippage_bps)
            .ok_or_else(|| anyhow!("Invalid slippage value"))?;

        let min_amount = (amount as u128)
            .checked_mul(multiplier as u128)
            .ok_or_else(|| anyhow!("Overflow applying slippage"))?
            .checked_div(10_000)
            .unwrap_or(0);

        Ok(min_amount as u64)
    }

    /// Get current price in SOL per token
    pub fn get_current_price(bonding_curve: &BondingCurve) -> f64 {
        if bonding_curve.virtual_token_reserves == 0 {
            return 0.0;
        }

        (bonding_curve.virtual_sol_reserves as f64) / (bonding_curve.virtual_token_reserves as f64)
    }

    /// Get market cap in SOL
    pub fn get_market_cap_sol(bonding_curve: &BondingCurve) -> f64 {
        let price = Self::get_current_price(bonding_curve);
        price * (bonding_curve.token_total_supply as f64)
    }

    /// Calculate SOL input required for a given token output (inverse buy)
    /// 
    /// Uses constant product formula in reverse: solve for sol_in given tokens_out
    /// 
    /// Formula: sol_in = (k / (virtual_tokens - tokens_out)) - virtual_sol
    /// 
    /// # Arguments
    /// * `bonding_curve` - Current bonding curve state
    /// * `token_amount_out` - Desired token output (before fees)
    /// * `slippage_bps` - Slippage tolerance in basis points
    /// 
    /// # Returns
    /// * Quote with SOL input required, fees, and price impact
    pub fn calculate_sol_input_for_tokens(
        bonding_curve: &BondingCurve,
        token_amount_out: u64,
        slippage_bps: u64,
    ) -> Result<Quote> {
        if bonding_curve.complete {
            return Err(anyhow!("Bonding curve has migrated to Raydium"));
        }

        if token_amount_out == 0 {
            return Err(anyhow!("Token amount must be greater than 0"));
        }

        // Check if there are enough real tokens available
        if token_amount_out > bonding_curve.real_token_reserves {
            return Err(anyhow!(
                "Insufficient real token reserves. Available: {}, Required: {}",
                bonding_curve.real_token_reserves,
                token_amount_out
            ));
        }

        // Calculate constant product k = x * y
        let k = bonding_curve
            .virtual_sol_reserves
            .checked_mul(bonding_curve.virtual_token_reserves)
            .ok_or_else(|| anyhow!("Overflow calculating k"))?;

        // Calculate new token reserves after output
        let new_virtual_token_reserves = bonding_curve
            .virtual_token_reserves
            .checked_sub(token_amount_out)
            .ok_or_else(|| anyhow!("Insufficient token reserves"))?;

        // Calculate new SOL reserves: new_sol = k / new_tokens
        let new_virtual_sol_reserves = k
            .checked_div(new_virtual_token_reserves)
            .ok_or_else(|| anyhow!("Division error calculating new SOL reserves"))?;

        // SOL input = new_reserves - old_reserves
        let sol_input = new_virtual_sol_reserves
            .checked_sub(bonding_curve.virtual_sol_reserves)
            .ok_or_else(|| anyhow!("Insufficient SOL reserves"))?;

        // Calculate fee (1% of tokens out)
        let fee_amount = token_amount_out
            .checked_mul(FEE_BASIS_POINTS)
            .ok_or_else(|| anyhow!("Overflow calculating fee"))?
            .checked_div(10_000)
            .unwrap_or(0);

        let tokens_out_after_fees = token_amount_out
            .checked_sub(fee_amount)
            .ok_or_else(|| anyhow!("Fee exceeds output amount"))?;

        // Apply slippage tolerance to SOL input (user pays more to account for slippage)
        let max_sol_input = Self::apply_slippage_inverse(sol_input, slippage_bps)?;

        // Calculate price impact
        let price_impact_bps = Self::calculate_price_impact(
            bonding_curve.virtual_token_reserves,
            token_amount_out,
        )?;

        // Calculate effective price (SOL per token)
        let effective_price = if tokens_out_after_fees > 0 {
            (max_sol_input as f64) / (tokens_out_after_fees as f64)
        } else {
            0.0
        };

        Ok(Quote {
            amount_in: max_sol_input,
            amount_out: token_amount_out,
            amount_out_after_fees: tokens_out_after_fees,
            fee_amount,
            price_impact_bps,
            effective_price,
        })
    }

    /// Calculate token input required for a given SOL output (inverse sell)
    /// 
    /// Uses constant product formula in reverse: solve for tokens_in given sol_out
    /// 
    /// Formula: tokens_in = (k / (virtual_sol - sol_out)) - virtual_tokens
    /// 
    /// # Arguments
    /// * `bonding_curve` - Current bonding curve state
    /// * `sol_amount_out` - Desired SOL output (before fees)
    /// * `slippage_bps` - Slippage tolerance in basis points
    /// 
    /// # Returns
    /// * Quote with token input required, fees, and price impact
    pub fn calculate_token_input_for_sol(
        bonding_curve: &BondingCurve,
        sol_amount_out: u64,
        slippage_bps: u64,
    ) -> Result<Quote> {
        if bonding_curve.complete {
            return Err(anyhow!("Bonding curve has migrated to Raydium"));
        }

        if sol_amount_out == 0 {
            return Err(anyhow!("SOL amount must be greater than 0"));
        }

        // Check if there is enough real SOL available
        if sol_amount_out > bonding_curve.real_sol_reserves {
            return Err(anyhow!(
                "Insufficient real SOL reserves. Available: {}, Required: {}",
                bonding_curve.real_sol_reserves,
                sol_amount_out
            ));
        }

        // Calculate constant product k = x * y
        let k = bonding_curve
            .virtual_sol_reserves
            .checked_mul(bonding_curve.virtual_token_reserves)
            .ok_or_else(|| anyhow!("Overflow calculating k"))?;

        // Calculate new SOL reserves after output
        let new_virtual_sol_reserves = bonding_curve
            .virtual_sol_reserves
            .checked_sub(sol_amount_out)
            .ok_or_else(|| anyhow!("Insufficient SOL reserves"))?;

        // Calculate new token reserves: new_tokens = k / new_sol
        let new_virtual_token_reserves = k
            .checked_div(new_virtual_sol_reserves)
            .ok_or_else(|| anyhow!("Division error calculating new token reserves"))?;

        // Token input = new_reserves - old_reserves
        let token_input = new_virtual_token_reserves
            .checked_sub(bonding_curve.virtual_token_reserves)
            .ok_or_else(|| anyhow!("Insufficient token reserves"))?;

        // Calculate fee (1% of SOL out)
        let fee_amount = sol_amount_out
            .checked_mul(FEE_BASIS_POINTS)
            .ok_or_else(|| anyhow!("Overflow calculating fee"))?
            .checked_div(10_000)
            .unwrap_or(0);

        let sol_out_after_fees = sol_amount_out
            .checked_sub(fee_amount)
            .ok_or_else(|| anyhow!("Fee exceeds output amount"))?;

        // Apply slippage tolerance to token input (user pays more to account for slippage)
        let max_token_input = Self::apply_slippage_inverse(token_input, slippage_bps)?;

        // Calculate price impact
        let price_impact_bps = Self::calculate_price_impact(
            bonding_curve.virtual_sol_reserves,
            sol_amount_out,
        )?;

        // Calculate effective price (SOL per token)
        let effective_price = if max_token_input > 0 {
            (sol_out_after_fees as f64) / (max_token_input as f64)
        } else {
            0.0
        };

        Ok(Quote {
            amount_in: max_token_input,
            amount_out: sol_amount_out,
            amount_out_after_fees: sol_out_after_fees,
            fee_amount,
            price_impact_bps,
            effective_price,
        })
    }

    /// Get bonding curve progress percentage
    /// 
    /// Progress = (initial_real_token_reserves - current_real_token_reserves) / initial_real_token_reserves * 100
    /// 
    /// # Arguments
    /// * `bonding_curve` - Current bonding curve state
    /// * `initial_real_token_reserves` - Initial real token reserves (from constants)
    /// 
    /// # Returns
    /// * Progress percentage (0.0 to 100.0)
    pub fn get_progress_percentage(
        bonding_curve: &BondingCurve,
        initial_real_token_reserves: u64,
    ) -> f64 {
        if initial_real_token_reserves == 0 {
            return 0.0;
        }

        if bonding_curve.complete {
            return 100.0;
        }

        let tokens_sold = initial_real_token_reserves
            .saturating_sub(bonding_curve.real_token_reserves);

        let progress = (tokens_sold as f64) / (initial_real_token_reserves as f64) * 100.0;
        progress.min(100.0).max(0.0)
    }

    /// Apply slippage tolerance inversely (for input calculations)
    /// 
    /// max_input = amount_input * (10000 + slippage_bps) / 10000
    fn apply_slippage_inverse(amount: u64, slippage_bps: u64) -> Result<u64> {
        if slippage_bps >= 10_000 {
            return Err(anyhow!("Slippage tolerance cannot be 100% or greater"));
        }

        let multiplier = 10_000u64
            .checked_add(slippage_bps)
            .ok_or_else(|| anyhow!("Overflow calculating slippage multiplier"))?;

        let max_amount = (amount as u128)
            .checked_mul(multiplier as u128)
            .ok_or_else(|| anyhow!("Overflow applying slippage"))?
            .checked_div(10_000)
            .unwrap_or(0);

        Ok(max_amount as u64)
    }

    /// Simulate a trade and return updated bonding curve state
    pub fn simulate_trade(
        bonding_curve: &BondingCurve,
        params: &TradeParams,
    ) -> Result<(BondingCurve, Quote)> {
        let quote = match params.direction {
            TradeDirection::Buy => Self::calculate_buy_quote(
                bonding_curve,
                params.amount_in,
                params.slippage_bps,
            )?,
            TradeDirection::Sell => Self::calculate_sell_quote(
                bonding_curve,
                params.amount_in,
                params.slippage_bps,
            )?,
        };

        // Create updated bonding curve state
        let mut new_curve = *bonding_curve;

        match params.direction {
            TradeDirection::Buy => {
                new_curve.virtual_sol_reserves = new_curve
                    .virtual_sol_reserves
                    .checked_add(params.amount_in)
                    .ok_or_else(|| anyhow!("Overflow updating SOL reserves"))?;

                new_curve.real_sol_reserves = new_curve
                    .real_sol_reserves
                    .checked_add(params.amount_in)
                    .ok_or_else(|| anyhow!("Overflow updating real SOL reserves"))?;

                new_curve.virtual_token_reserves = new_curve
                    .virtual_token_reserves
                    .checked_sub(quote.amount_out)
                    .ok_or_else(|| anyhow!("Underflow updating token reserves"))?;

                new_curve.real_token_reserves = new_curve
                    .real_token_reserves
                    .checked_sub(quote.amount_out)
                    .ok_or_else(|| anyhow!("Underflow updating real token reserves"))?;

                // Check if migration threshold is reached
                if new_curve.real_token_reserves == 0 {
                    new_curve.complete = true;
                }
            }
            TradeDirection::Sell => {
                new_curve.virtual_token_reserves = new_curve
                    .virtual_token_reserves
                    .checked_add(params.amount_in)
                    .ok_or_else(|| anyhow!("Overflow updating token reserves"))?;

                new_curve.virtual_sol_reserves = new_curve
                    .virtual_sol_reserves
                    .checked_sub(quote.amount_out)
                    .ok_or_else(|| anyhow!("Underflow updating SOL reserves"))?;

                new_curve.real_sol_reserves = new_curve
                    .real_sol_reserves
                    .checked_sub(quote.amount_out)
                    .ok_or_else(|| anyhow!("Underflow updating real SOL reserves"))?;
            }
        }

        Ok((new_curve, quote))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pumpfun::constants::*;

    fn create_test_bonding_curve() -> BondingCurve {
        BondingCurve {
            virtual_token_reserves: INITIAL_VIRTUAL_TOKEN_RESERVES,
            virtual_sol_reserves: INITIAL_VIRTUAL_SOL_RESERVES,
            real_token_reserves: INITIAL_REAL_TOKEN_RESERVES,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000_000, // 1B tokens
            complete: false,
        }
    }

    #[test]
    fn test_calculate_buy_quote() {
        let curve = create_test_bonding_curve();
        let sol_amount = 1_000_000_000; // 1 SOL
        let slippage = 100; // 1%

        let quote = BondingCurveCalculator::calculate_buy_quote(&curve, sol_amount, slippage);
        assert!(quote.is_ok());

        let quote = quote.unwrap();
        assert!(quote.amount_out > 0);
        assert!(quote.amount_out_after_fees < quote.amount_out);
        assert_eq!(quote.amount_in, sol_amount);
    }

    #[test]
    fn test_calculate_sell_quote() {
        let curve = create_test_bonding_curve();
        let token_amount = 1_000_000_000; // 1k tokens
        let slippage = 100; // 1%

        let quote = BondingCurveCalculator::calculate_sell_quote(&curve, token_amount, slippage);
        assert!(quote.is_ok());
    }

    #[test]
    fn test_get_current_price() {
        let curve = create_test_bonding_curve();
        let price = BondingCurveCalculator::get_current_price(&curve);
        assert!(price > 0.0);
    }
}
