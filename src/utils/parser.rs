use anyhow::{Result, anyhow};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub fn parse_pubkey(address: &str) -> Result<Pubkey> {
    Pubkey::from_str(address).map_err(|e| anyhow!("Invalid pubkey: {}", e))
}

pub fn parse_amount(amount_str: &str) -> Result<f64> {
    amount_str.parse::<f64>().map_err(|e| anyhow!("Invalid amount: {}", e))
}

pub fn format_pubkey(pubkey: &Pubkey) -> String {
    pubkey.to_string()
}
