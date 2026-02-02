use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Pump.fun program ID
pub const PUMP_FUN_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

/// Global state address for pump.fun
pub const GLOBAL_STATE_ADDRESS: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";

/// Fee recipient address
pub const FEE_RECIPIENT_ADDRESS: &str = "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM";

/// Event authority address
pub const EVENT_AUTHORITY_ADDRESS: &str = "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1";

/// Mint authority address
pub const MINT_AUTHORITY_ADDRESS: &str = "TSLvdd1pWpHVjahSpsvCXUbgwsL3JAcvokwaKt1eokM";

/// System program constants
pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const RENT_PROGRAM_ID: &str = "SysvarRent111111111111111111111111111111111";

/// Bonding curve parameters
pub const INITIAL_VIRTUAL_TOKEN_RESERVES: u64 = 1_073_000_000_000_000; // 1.073M tokens with 6 decimals
pub const INITIAL_VIRTUAL_SOL_RESERVES: u64 = 30_000_000_000; // 30 SOL in lamports
pub const INITIAL_REAL_TOKEN_RESERVES: u64 = 793_100_000_000_000; // 793.1M tokens

/// Fee structure (in basis points, 1 bp = 0.01%)
pub const FEE_BASIS_POINTS: u64 = 100; // 1% fee

/// Compute budget constants
pub const COMPUTE_UNIT_LIMIT: u32 = 1_400_000; // 1.4M CU recommended
pub const DEFAULT_COMPUTE_UNIT_PRICE: u64 = 1000; // Micro-lamports per CU

/// Migration threshold (when bonding curve completes)
pub const MIGRATION_TOKEN_THRESHOLD: u64 = 0; // When real token reserves hit 0

/// Token decimals
pub const TOKEN_DECIMALS: u8 = 6;

/// Helper functions to get Pubkeys
pub fn get_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_FUN_PROGRAM_ID).unwrap()
}

pub fn get_global_state() -> Pubkey {
    Pubkey::from_str(GLOBAL_STATE_ADDRESS).unwrap()
}

pub fn get_fee_recipient() -> Pubkey {
    Pubkey::from_str(FEE_RECIPIENT_ADDRESS).unwrap()
}

pub fn get_event_authority() -> Pubkey {
    Pubkey::from_str(EVENT_AUTHORITY_ADDRESS).unwrap()
}

pub fn get_mint_authority() -> Pubkey {
    Pubkey::from_str(MINT_AUTHORITY_ADDRESS).unwrap()
}

pub fn get_system_program() -> Pubkey {
    Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap()
}

pub fn get_token_program() -> Pubkey {
    Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap()
}

pub fn get_associated_token_program() -> Pubkey {
    Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM_ID).unwrap()
}

pub fn get_rent_program() -> Pubkey {
    Pubkey::from_str(RENT_PROGRAM_ID).unwrap()
}
