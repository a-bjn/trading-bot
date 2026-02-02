use crate::pumpfun::constants::*;
use anyhow::Result;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

/// Instruction discriminators for pump.fun program
pub mod discriminators {
    pub const BUY: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234]; // buy instruction
    pub const SELL: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173]; // sell instruction
}

/// Build a buy instruction for pump.fun
/// 
/// # Arguments
/// * `buyer` - The wallet buying tokens
/// * `mint` - The token mint address
/// * `bonding_curve` - The bonding curve PDA
/// * `associated_bonding_curve` - The associated bonding curve token account
/// * `buyer_token_account` - The buyer's token account
/// * `sol_amount` - Amount of SOL to spend (in lamports)
/// * `min_tokens_out` - Minimum tokens to receive (slippage protection)
/// 
/// # Returns
/// * Instruction for buying tokens
pub fn build_buy_instruction(
    buyer: Pubkey,
    mint: Pubkey,
    bonding_curve: Pubkey,
    associated_bonding_curve: Pubkey,
    buyer_token_account: Pubkey,
    sol_amount: u64,
    min_tokens_out: u64,
) -> Result<Instruction> {
    let program_id = get_program_id();
    let global_state = get_global_state();
    let fee_recipient = get_fee_recipient();
    let system_program = get_system_program();
    let token_program = get_token_program();
    let rent_program = get_rent_program();
    let event_authority = get_event_authority();

    // Build instruction data
    let mut data = Vec::new();
    data.extend_from_slice(&discriminators::BUY);
    data.extend_from_slice(&sol_amount.to_le_bytes());
    data.extend_from_slice(&min_tokens_out.to_le_bytes());

    let accounts = vec![
        AccountMeta::new_readonly(global_state, false),
        AccountMeta::new(fee_recipient, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(bonding_curve, false),
        AccountMeta::new(associated_bonding_curve, false),
        AccountMeta::new(buyer_token_account, false),
        AccountMeta::new(buyer, true), // Signer
        AccountMeta::new_readonly(system_program, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(rent_program, false),
        AccountMeta::new_readonly(event_authority, false),
        AccountMeta::new_readonly(program_id, false),
    ];

    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

/// Build a sell instruction for pump.fun
/// 
/// # Arguments
/// * `seller` - The wallet selling tokens
/// * `mint` - The token mint address
/// * `bonding_curve` - The bonding curve PDA
/// * `associated_bonding_curve` - The associated bonding curve token account
/// * `seller_token_account` - The seller's token account
/// * `token_amount` - Amount of tokens to sell
/// * `min_sol_out` - Minimum SOL to receive (slippage protection)
pub fn build_sell_instruction(
    seller: Pubkey,
    mint: Pubkey,
    bonding_curve: Pubkey,
    associated_bonding_curve: Pubkey,
    seller_token_account: Pubkey,
    token_amount: u64,
    min_sol_out: u64,
) -> Result<Instruction> {
    let program_id = get_program_id();
    let global_state = get_global_state();
    let fee_recipient = get_fee_recipient();
    let system_program = get_system_program();
    let token_program = get_token_program();
    let event_authority = get_event_authority();

    // Build instruction data
    let mut data = Vec::new();
    data.extend_from_slice(&discriminators::SELL);
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&min_sol_out.to_le_bytes());

    let accounts = vec![
        AccountMeta::new_readonly(global_state, false),
        AccountMeta::new(fee_recipient, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new(bonding_curve, false),
        AccountMeta::new(associated_bonding_curve, false),
        AccountMeta::new(seller_token_account, false),
        AccountMeta::new(seller, true), // Signer
        AccountMeta::new_readonly(system_program, false),
        AccountMeta::new_readonly(token_program, false),
        AccountMeta::new_readonly(event_authority, false),
        AccountMeta::new_readonly(program_id, false),
    ];

    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

/// Build compute budget instructions
/// 
/// # Arguments
/// * `compute_unit_limit` - Maximum compute units (default: 1.4M)
/// * `compute_unit_price` - Priority fee in micro-lamports per CU
/// 
/// # Returns
/// * Vector of compute budget instructions
pub fn build_compute_budget_instructions(
    compute_unit_limit: Option<u32>,
    compute_unit_price: Option<u64>,
) -> Vec<Instruction> {
    let limit = compute_unit_limit.unwrap_or(COMPUTE_UNIT_LIMIT);
    let price = compute_unit_price.unwrap_or(DEFAULT_COMPUTE_UNIT_PRICE);

    vec![
        ComputeBudgetInstruction::set_compute_unit_limit(limit),
        ComputeBudgetInstruction::set_compute_unit_price(price),
    ]
}

/// Build a complete buy transaction with compute budget
/// 
/// # Arguments
/// * `buyer` - The wallet buying tokens
/// * `mint` - The token mint address
/// * `bonding_curve` - The bonding curve PDA
/// * `associated_bonding_curve` - The associated bonding curve token account
/// * `buyer_token_account` - The buyer's token account
/// * `sol_amount` - Amount of SOL to spend (in lamports)
/// * `min_tokens_out` - Minimum tokens to receive (slippage protection)
/// * `priority_fee` - Optional priority fee in micro-lamports per CU
pub fn build_buy_transaction_instructions(
    buyer: Pubkey,
    mint: Pubkey,
    bonding_curve: Pubkey,
    associated_bonding_curve: Pubkey,
    buyer_token_account: Pubkey,
    sol_amount: u64,
    min_tokens_out: u64,
    priority_fee: Option<u64>,
) -> Result<Vec<Instruction>> {
    let mut instructions = build_compute_budget_instructions(None, priority_fee);

    let buy_ix = build_buy_instruction(
        buyer,
        mint,
        bonding_curve,
        associated_bonding_curve,
        buyer_token_account,
        sol_amount,
        min_tokens_out,
    )?;

    instructions.push(buy_ix);

    Ok(instructions)
}

/// Build a complete sell transaction with compute budget
pub fn build_sell_transaction_instructions(
    seller: Pubkey,
    mint: Pubkey,
    bonding_curve: Pubkey,
    associated_bonding_curve: Pubkey,
    seller_token_account: Pubkey,
    token_amount: u64,
    min_sol_out: u64,
    priority_fee: Option<u64>,
) -> Result<Vec<Instruction>> {
    let mut instructions = build_compute_budget_instructions(None, priority_fee);

    let sell_ix = build_sell_instruction(
        seller,
        mint,
        bonding_curve,
        associated_bonding_curve,
        seller_token_account,
        token_amount,
        min_sol_out,
    )?;

    instructions.push(sell_ix);

    Ok(instructions)
}

/// Build create associated token account instruction if needed
pub fn build_create_ata_instruction(
    payer: Pubkey,
    wallet: Pubkey,
    mint: Pubkey,
) -> Instruction {
    let associated_token_address =
        spl_associated_token_account::get_associated_token_address(&wallet, &mint);

    spl_associated_token_account::instruction::create_associated_token_account(
        &payer,
        &wallet,
        &mint,
        &get_token_program(),
    )
}

/// Calculate priority fee based on network conditions
/// 
/// # Arguments
/// * `base_fee` - Base priority fee
/// * `is_high_priority` - Whether this is a high priority transaction
/// 
/// # Returns
/// * Calculated priority fee in micro-lamports per CU
pub fn calculate_priority_fee(base_fee: u64, is_high_priority: bool) -> u64 {
    if is_high_priority {
        base_fee.saturating_mul(5) // 5x for high priority
    } else {
        base_fee
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_compute_budget_instructions() {
        let instructions = build_compute_budget_instructions(None, None);
        assert_eq!(instructions.len(), 2);
    }

    #[test]
    fn test_calculate_priority_fee() {
        let base = 1000;
        let normal = calculate_priority_fee(base, false);
        let high = calculate_priority_fee(base, true);

        assert_eq!(normal, base);
        assert_eq!(high, base * 5);
    }

    #[test]
    fn test_build_buy_instruction() {
        let buyer = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        let bonding_curve = Pubkey::new_unique();
        let associated_bonding_curve = Pubkey::new_unique();
        let buyer_token_account = Pubkey::new_unique();

        let result = build_buy_instruction(
            buyer,
            mint,
            bonding_curve,
            associated_bonding_curve,
            buyer_token_account,
            1_000_000_000,
            1_000_000,
        );

        assert!(result.is_ok());
        let instruction = result.unwrap();
        assert_eq!(instruction.program_id, get_program_id());
        assert!(instruction.accounts.len() > 0);
    }
}
