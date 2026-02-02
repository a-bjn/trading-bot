/// Pump.fun Integration Demo
/// 
/// This example demonstrates how to use the pump.fun integration to:
/// 1. Fetch bonding curve state
/// 2. Calculate buy/sell quotes
/// 3. Build and execute trades
/// 4. Monitor bonding curve progression

use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solana_trading_bot::pumpfun::*;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize RPC client
    let rpc_url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let rpc_client = RpcClient::new(rpc_url);

    println!("=== Pump.fun Trading Integration Demo ===\n");

    // Example token mint (replace with actual pump.fun token)
    let token_mint = Pubkey::from_str("YOUR_TOKEN_MINT_HERE")?;

    // 1. Fetch bonding curve state
    println!("1. Fetching bonding curve state...");
    match fetch_bonding_curve_state(&rpc_client, &token_mint) {
        Ok(state) => {
            println!("   ✓ Bonding curve found!");
            println!("   - Address: {}", state.address);
            println!("   - Virtual SOL Reserves: {} lamports", state.bonding_curve.virtual_sol_reserves);
            println!("   - Virtual Token Reserves: {} tokens", state.bonding_curve.virtual_token_reserves);
            println!("   - Real SOL Reserves: {} lamports", state.bonding_curve.real_sol_reserves);
            println!("   - Real Token Reserves: {} tokens", state.bonding_curve.real_token_reserves);
            println!("   - Current Price: {} SOL/token", state.current_price);
            println!("   - Market Cap: {} SOL", state.market_cap_sol);
            println!("   - Migrated: {}\n", state.bonding_curve.complete);

            // 2. Calculate buy quote
            println!("2. Calculating buy quote for 1 SOL...");
            let sol_amount = sol_to_lamports(1.0);
            let slippage_bps = 100; // 1% slippage

            match BondingCurveCalculator::calculate_buy_quote(
                &state.bonding_curve,
                sol_amount,
                slippage_bps,
            ) {
                Ok(quote) => {
                    println!("   ✓ Buy Quote:");
                    println!("   - Amount In: {} SOL", lamports_to_sol(quote.amount_in));
                    println!("   - Tokens Out (before fees): {}", base_units_to_tokens(quote.amount_out, 6));
                    println!("   - Tokens Out (after fees): {}", base_units_to_tokens(quote.amount_out_after_fees, 6));
                    println!("   - Fee: {}", base_units_to_tokens(quote.fee_amount, 6));
                    println!("   - Price Impact: {:.2}%", quote.price_impact_bps as f64 / 100.0);
                    println!("   - Effective Price: {} SOL/token\n", quote.effective_price);

                    // 3. Simulate the trade
                    println!("3. Simulating trade...");
                    let trade_params = TradeParams {
                        amount_in: sol_amount,
                        min_amount_out: quote.amount_out_after_fees,
                        direction: TradeDirection::Buy,
                        slippage_bps,
                    };

                    match BondingCurveCalculator::simulate_trade(&state.bonding_curve, &trade_params) {
                        Ok((new_curve, _quote)) => {
                            println!("   ✓ Trade simulation successful!");
                            println!("   - New Virtual SOL Reserves: {} lamports", new_curve.virtual_sol_reserves);
                            println!("   - New Virtual Token Reserves: {} tokens", new_curve.virtual_token_reserves);
                            println!("   - New Real Token Reserves: {} tokens", new_curve.real_token_reserves);
                            println!("   - New Price: {} SOL/token\n", 
                                BondingCurveCalculator::get_current_price(&new_curve));
                        }
                        Err(e) => println!("   ✗ Simulation failed: {}\n", e),
                    }
                }
                Err(e) => println!("   ✗ Quote calculation failed: {}\n", e),
            }

            // 4. Build buy instruction (not executed in demo)
            println!("4. Building buy instruction...");
            let wallet = Keypair::new(); // Replace with actual wallet
            let (bonding_curve_pda, _) = get_bonding_curve_pda(&token_mint);
            let associated_bonding_curve = get_associated_bonding_curve_address(
                &bonding_curve_pda,
                &token_mint,
            );
            let buyer_token_account = get_associated_token_address(
                &wallet.pubkey(),
                &token_mint,
            );

            match build_buy_transaction_instructions(
                wallet.pubkey(),
                token_mint,
                bonding_curve_pda,
                associated_bonding_curve,
                buyer_token_account,
                sol_amount,
                quote.amount_out_after_fees,
                Some(5000), // 5000 micro-lamports priority fee
            ) {
                Ok(instructions) => {
                    println!("   ✓ Buy instructions built successfully!");
                    println!("   - Total instructions: {}", instructions.len());
                    println!("   - Compute budget instructions: 2");
                    println!("   - Buy instruction: 1\n");
                }
                Err(e) => println!("   ✗ Failed to build instructions: {}\n", e),
            }

            // 5. Calculate sell quote
            println!("5. Calculating sell quote for 1000 tokens...");
            let token_amount = tokens_to_base_units(1000.0, 6);

            match BondingCurveCalculator::calculate_sell_quote(
                &state.bonding_curve,
                token_amount,
                slippage_bps,
            ) {
                Ok(quote) => {
                    println!("   ✓ Sell Quote:");
                    println!("   - Tokens In: {}", base_units_to_tokens(quote.amount_in, 6));
                    println!("   - SOL Out (before fees): {} SOL", lamports_to_sol(quote.amount_out));
                    println!("   - SOL Out (after fees): {} SOL", lamports_to_sol(quote.amount_out_after_fees));
                    println!("   - Fee: {} SOL", lamports_to_sol(quote.fee_amount));
                    println!("   - Price Impact: {:.2}%", quote.price_impact_bps as f64 / 100.0);
                    println!("   - Effective Price: {} SOL/token\n", quote.effective_price);
                }
                Err(e) => println!("   ✗ Quote calculation failed: {}\n", e),
            }
        }
        Err(e) => {
            println!("   ✗ Failed to fetch bonding curve: {}", e);
            println!("   Note: Make sure to use a valid pump.fun token mint\n");
        }
    }

    // 6. Helper functions demonstration
    println!("6. Helper functions:");
    println!("   - SOL to lamports: 1.5 SOL = {} lamports", sol_to_lamports(1.5));
    println!("   - Lamports to SOL: 1500000000 lamports = {} SOL", lamports_to_sol(1_500_000_000));
    println!("   - Tokens to base units: 1000 tokens = {} base units", tokens_to_base_units(1000.0, 6));
    println!("   - Base units to tokens: 1000000000 base units = {} tokens", base_units_to_tokens(1_000_000_000, 6));

    println!("\n=== Demo Complete ===");

    Ok(())
}
