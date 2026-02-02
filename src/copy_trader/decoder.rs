use anyhow::{anyhow, Result};
use solana_sdk::{
    instruction::CompiledInstruction,
    message::Message,
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransactionWithStatusMeta, UiMessage,
    UiInstruction, UiTransaction,
};
use std::str::FromStr;
use tracing::debug;

use super::signal::{TradeAction, TradeSignal};
use crate::copy_trader::program_ids;
use chrono::{DateTime, Utc};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
/// Transaction decoder for extracting swap information
pub struct TransactionDecoder {
    /// Target wallet to monitor
    target_wallet: Pubkey,
}

impl TransactionDecoder {
    /// Create a new transaction decoder
    pub fn new(target_wallet: Pubkey) -> Self {
        Self { target_wallet }
    }
    
    /// Decode a transaction and extract trade signal
    pub fn decode_transaction(
        &self,
        tx: &EncodedConfirmedTransactionWithStatusMeta,
        signature: &str,
    ) -> Result<Option<TradeSignal>> {
        let ui_transaction = match &tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => ui_tx,
            _ => return Ok(None),
        };
        
        let transaction = ui_transaction;
        
        // Extract timestamp
        let timestamp = if let Some(block_time) = tx.block_time {
            DateTime::from_timestamp(block_time, 0)
                .unwrap_or_else(Utc::now)
        } else {
            Utc::now()
        };
        
        // Check if transaction involves our target wallet
        if !self.involves_target_wallet(&transaction.message)? {
            return Ok(None);
        }
        
        // Try to decode as DFlow transaction
        if let Some(signal) = self.decode_dflow_swap(&transaction, signature, timestamp)? {
            return Ok(Some(signal));
        }
        
        // Try to decode as OkxDex transaction
        if let Some(signal) = self.decode_okx_dex_swap(&transaction, signature, timestamp)? {
            return Ok(Some(signal));
        }
        
        Ok(None)
    }
    
    /// Check if the transaction involves the target wallet
    fn involves_target_wallet(&self, message: &UiMessage) -> Result<bool> {
        match message {
            UiMessage::Parsed(parsed) => {
                for account_key in &parsed.account_keys {
                    let pubkey_str = &account_key.pubkey;
                    {
                        if let Ok(pubkey) = Pubkey::from_str(pubkey_str) {
                            if pubkey == self.target_wallet {
                                return Ok(true);
                            }
                        }
                    }
                }
                Ok(false)
            }
            UiMessage::Raw(raw) => {
                for account_key in &raw.account_keys {
                    if let Ok(pubkey) = Pubkey::from_str(account_key) {
                        if pubkey == self.target_wallet {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
        }
    }
    
    /// Decode a DFlow swap transaction
    fn decode_dflow_swap(
        &self,
        transaction: &UiTransaction,
        signature: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<TradeSignal>> {
        let dflow_program = program_ids::dflow();
        
        // Look for DFlow program in instructions
        let instructions = self.get_instructions(&transaction.message)?;
        
        for instruction in instructions {
            if let Some(program_id) = instruction.program_id {
                if program_id == dflow_program {
                    // Parse DFlow instruction data
                    if let Some(signal) = self.parse_dflow_instruction(
                        &instruction,
                        signature,
                        timestamp,
                    )? {
                        return Ok(Some(signal));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Decode an OkxDex swap transaction
    fn decode_okx_dex_swap(
        &self,
        transaction: &UiTransaction,
        signature: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<TradeSignal>> {
        let okx_program = program_ids::okx_dex();
        
        let instructions = self.get_instructions(&transaction.message)?;
        
        for instruction in instructions {
            if let Some(program_id) = instruction.program_id {
                if program_id == okx_program {
                    if let Some(signal) = self.parse_okx_dex_instruction(
                        &instruction,
                        signature,
                        timestamp,
                    )? {
                        return Ok(Some(signal));
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Parse DFlow instruction data
    fn parse_dflow_instruction(
        &self,
        instruction: &InstructionData,
        signature: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<TradeSignal>> {
        let data = match &instruction.data {
            Some(d) => d,
            None => return Ok(None),
        };
        
        // Decode base64 or base58 instruction data
        let decoded_data = self.decode_instruction_data(data)?;
        
        // DFlow swap instruction format:
        // [0]: discriminator (8 bytes)
        // [8-16]: amount_in (u64)
        // [16-24]: minimum_amount_out (u64)
        
        if decoded_data.len() < 24 {
            return Ok(None);
        }
        
        let amount_in = u64::from_le_bytes(
            decoded_data[8..16].try_into().unwrap_or([0u8; 8])
        );
        
        // Extract token mint and SOL amount from accounts
        let (token_mint, sol_amount, action) = self.extract_swap_details(
            &instruction.accounts,
            amount_in,
        )?;
        
        // Check if it's a pump.fun token
        let is_pump_fun = self.is_pump_fun_token(&token_mint)?;
        
        let signal = TradeSignal::new(
            signature.to_string(),
            token_mint,
            sol_amount,
            action,
            timestamp,
            "DFlow".to_string(),
            self.target_wallet,
        )
        .with_token_amount(amount_in)
        .with_pump_fun(is_pump_fun);
        
        Ok(Some(signal))
    }
    
    /// Parse OkxDex instruction data
    fn parse_okx_dex_instruction(
        &self,
        instruction: &InstructionData,
        signature: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<TradeSignal>> {
        let data = match &instruction.data {
            Some(d) => d,
            None => return Ok(None),
        };
        
        let decoded_data = self.decode_instruction_data(data)?;
        
        // OkxDex swap instruction format (similar structure)
        if decoded_data.len() < 24 {
            return Ok(None);
        }
        
        let amount_in = u64::from_le_bytes(
            decoded_data[8..16].try_into().unwrap_or([0u8; 8])
        );
        
        let (token_mint, sol_amount, action) = self.extract_swap_details(
            &instruction.accounts,
            amount_in,
        )?;
        
        let is_pump_fun = self.is_pump_fun_token(&token_mint)?;
        
        let signal = TradeSignal::new(
            signature.to_string(),
            token_mint,
            sol_amount,
            action,
            timestamp,
            "OkxDex".to_string(),
            self.target_wallet,
        )
        .with_token_amount(amount_in)
        .with_pump_fun(is_pump_fun);
        
        Ok(Some(signal))
    }
    
    /// Extract instructions from transaction message
    fn get_instructions(&self, message: &UiMessage) -> Result<Vec<InstructionData>> {
        let mut instructions = Vec::new();
        
        match message {
            UiMessage::Parsed(parsed) => {
                for ix in &parsed.instructions {
                    match ix {
                        UiInstruction::Parsed(_parsed_ix) => {
                            // Skip fully parsed instructions (system program, SPL token, etc.)
                            debug!("Skipping fully parsed instruction");
                        }
                        UiInstruction::Compiled(compiled_ix) => {
                            let program_id = parsed.account_keys
                                .get(compiled_ix.program_id_index as usize)
                                .and_then(|key| Pubkey::from_str(&key.pubkey).ok());
                            
                            instructions.push(InstructionData {
                                program_id,
                                accounts: vec![],
                                data: Some(compiled_ix.data.clone()),
                            });
                        }
                    }
                }
            }
            UiMessage::Raw(raw) => {
                for ix in &raw.instructions {
                    let program_id = raw.account_keys
                        .get(ix.program_id_index as usize)
                        .and_then(|key| Pubkey::from_str(key).ok());
                    
                    let accounts: Vec<Pubkey> = ix.accounts
                        .iter()
                        .filter_map(|&idx| {
                            raw.account_keys
                                .get(idx as usize)
                                .and_then(|key| Pubkey::from_str(key).ok())
                        })
                        .collect();
                    
                    instructions.push(InstructionData {
                        program_id,
                        accounts,
                        data: Some(ix.data.clone()),
                    });
                }
            }
        }
        
        Ok(instructions)
    }
    
    /// Decode instruction data from base58 or base64
    fn decode_instruction_data(&self, data: &str) -> Result<Vec<u8>> {
        // Try base58 first
        if let Ok(decoded) = bs58::decode(data).into_vec() {
            return Ok(decoded);
        }
        
        // Try base64
        if let Ok(decoded) = BASE64_STANDARD.decode(data) {
            return Ok(decoded);
        }
        
        Err(anyhow!("Failed to decode instruction data"))
    }
    
    /// Extract swap details from instruction accounts
    fn extract_swap_details(
        &self,
        accounts: &[Pubkey],
        amount_in: u64,
    ) -> Result<(Pubkey, f64, TradeAction)> {
        // Account indices for typical swap instruction:
        // [0]: user account
        // [1]: token mint
        // [2]: SOL account
        // [3-4]: pool accounts
        
        if accounts.len() < 2 {
            return Err(anyhow!("Insufficient accounts for swap"));
        }
        
        let token_mint = accounts[1];
        
        // Convert lamports to SOL (1 SOL = 1_000_000_000 lamports)
        let sol_amount = amount_in as f64 / 1_000_000_000.0;
        
        // Determine action (buy = SOL -> Token, sell = Token -> SOL)
        // This is a simplified heuristic; in practice, you'd check token balances
        let action = if sol_amount > 0.0 {
            TradeAction::Buy
        } else {
            TradeAction::Sell
        };
        
        Ok((token_mint, sol_amount.abs(), action))
    }
    
    /// Check if a token is from pump.fun
    fn is_pump_fun_token(&self, token_mint: &Pubkey) -> Result<bool> {
        // In a real implementation, this would query token metadata
        // or check against a pump.fun token registry
        // For now, we'll return true as a placeholder
        
        // You can implement this by:
        // 1. Checking token metadata program
        // 2. Querying pump.fun API
        // 3. Maintaining a local cache of pump.fun tokens
        
        Ok(true) // Placeholder
    }
}

/// Instruction data extracted from transaction
#[derive(Debug, Clone)]
struct InstructionData {
    program_id: Option<Pubkey>,
    accounts: Vec<Pubkey>,
    data: Option<String>,
}

/// Helper function to decode a transaction signature string
pub fn decode_signature(signature_str: &str) -> Result<Signature> {
    Signature::from_str(signature_str)
        .map_err(|e| anyhow!("Failed to decode signature: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decoder_creation() {
        let wallet = Pubkey::from_str("54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy")
            .unwrap();
        let decoder = TransactionDecoder::new(wallet);
        assert_eq!(decoder.target_wallet, wallet);
    }
    
    #[test]
    fn test_decode_instruction_data() {
        let wallet = Pubkey::default();
        let decoder = TransactionDecoder::new(wallet);
        
        // Test base58 encoding
        let data = bs58::encode(vec![1, 2, 3, 4]).into_string();
        let decoded = decoder.decode_instruction_data(&data).unwrap();
        assert_eq!(decoded, vec![1, 2, 3, 4]);
    }
    
    #[test]
    fn test_extract_swap_details() {
        let wallet = Pubkey::default();
        let decoder = TransactionDecoder::new(wallet);
        
        let accounts = vec![
            Pubkey::default(),
            Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap(),
        ];
        
        let amount = 1_000_000_000u64; // 1 SOL
        let (token_mint, sol_amount, action) = decoder
            .extract_swap_details(&accounts, amount)
            .unwrap();
        
        assert_eq!(token_mint, accounts[1]);
        assert_eq!(sol_amount, 1.0);
        assert_eq!(action, TradeAction::Buy);
    }
}
