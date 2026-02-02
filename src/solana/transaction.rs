use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    instruction::Instruction,
    message::Message,
    pubkey::Pubkey,
    signature::{Signature, Signer},
    transaction::Transaction,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, debug, error};

use crate::solana::error::{Result, SolanaError};
use crate::solana::client::SolanaClient;
use crate::solana::wallet::Wallet;

/// Transaction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub signature: String,
    pub success: bool,
    pub error: Option<String>,
    pub slot: Option<u64>,
}

impl TransactionResult {
    pub fn success(signature: Signature) -> Self {
        Self {
            signature: signature.to_string(),
            success: true,
            error: None,
            slot: None,
        }
    }

    pub fn failed(signature: Signature, error: String) -> Self {
        Self {
            signature: signature.to_string(),
            success: false,
            error: Some(error),
            slot: None,
        }
    }
}

/// Transaction builder with compute budget and priority fees
pub struct TransactionBuilder {
    client: Arc<SolanaClient>,
    wallet: Arc<Wallet>,
    instructions: Vec<Instruction>,
    compute_unit_limit: Option<u32>,
    compute_unit_price: Option<u64>,
    recent_blockhash: Option<Hash>,
}

impl TransactionBuilder {
    /// Create a new transaction builder
    pub fn new(client: Arc<SolanaClient>, wallet: Arc<Wallet>) -> Self {
        Self {
            client,
            wallet,
            instructions: Vec::new(),
            compute_unit_limit: None,
            compute_unit_price: None,
            recent_blockhash: None,
        }
    }

    /// Add an instruction
    pub fn add_instruction(&mut self, instruction: Instruction) -> &mut Self {
        self.instructions.push(instruction);
        self
    }

    /// Add multiple instructions
    pub fn add_instructions(&mut self, instructions: Vec<Instruction>) -> &mut Self {
        self.instructions.extend(instructions);
        self
    }

    /// Set compute unit limit
    pub fn with_compute_limit(&mut self, limit: u32) -> &mut Self {
        self.compute_unit_limit = Some(limit);
        self
    }

    /// Set compute unit price (priority fee in micro-lamports)
    pub fn with_compute_price(&mut self, price: u64) -> &mut Self {
        self.compute_unit_price = Some(price);
        self
    }

    /// Set recent blockhash (optional, will fetch if not provided)
    pub fn with_blockhash(&mut self, blockhash: Hash) -> &mut Self {
        self.recent_blockhash = Some(blockhash);
        self
    }

    /// Calculate recommended priority fee based on network conditions
    pub async fn with_dynamic_priority_fee(&mut self) -> Result<&mut Self> {
        // Get accounts involved in the transaction
        let accounts: Vec<Pubkey> = self
            .instructions
            .iter()
            .flat_map(|ix| ix.accounts.iter().map(|acc| acc.pubkey))
            .collect();

        // Get recommended fee from RPC
        let fee = self.client.get_recommended_fee(&accounts).await?;
        
        // Convert lamports to micro-lamports per compute unit
        let micro_lamports = fee / 1000; // Rough conversion
        
        debug!("Dynamic priority fee: {} micro-lamports", micro_lamports);
        self.compute_unit_price = Some(micro_lamports);
        
        Ok(self)
    }

    /// Build the transaction
    pub async fn build(&mut self) -> Result<Transaction> {
        if self.instructions.is_empty() {
            return Err(SolanaError::TransactionError(
                "No instructions added to transaction".to_string(),
            ));
        }

        // Prepare compute budget instructions
        let mut all_instructions = Vec::new();

        // Add compute unit limit if specified
        if let Some(limit) = self.compute_unit_limit {
            all_instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(limit));
            debug!("Added compute unit limit: {}", limit);
        }

        // Add compute unit price (priority fee) if specified
        if let Some(price) = self.compute_unit_price {
            all_instructions.push(ComputeBudgetInstruction::set_compute_unit_price(price));
            debug!("Added compute unit price: {} micro-lamports", price);
        }

        // Add main instructions
        all_instructions.extend(self.instructions.drain(..));

        // Get recent blockhash if not provided
        let blockhash = if let Some(hash) = self.recent_blockhash {
            hash
        } else {
            self.client.get_latest_blockhash().await?
        };

        // Create transaction
        let message = Message::new(&all_instructions, Some(&self.wallet.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);
        transaction.message.recent_blockhash = blockhash;

        // Sign transaction
        transaction.sign(&[self.wallet.keypair()], blockhash);

        debug!(
            "Transaction built with {} instructions, signature: {}",
            all_instructions.len(),
            transaction.signatures[0]
        );

        Ok(transaction)
    }

    /// Build and send the transaction
    pub async fn build_and_send(&mut self) -> Result<Signature> {
        let transaction = self.build().await?;
        
        info!("Sending transaction: {}", transaction.signatures[0]);
        
        let signature = self.client.send_transaction(&transaction).await?;
        
        Ok(signature)
    }

    /// Build, send, and confirm the transaction
    pub async fn build_send_and_confirm(&mut self, timeout_seconds: u64) -> Result<TransactionResult> {
        let transaction = self.build().await?;
        let signature = transaction.signatures[0];
        
        info!("Sending and confirming transaction: {}", signature);
        
        match self
            .client
            .send_and_confirm_transaction(&transaction, std::time::Duration::from_secs(timeout_seconds))
            .await
        {
            Ok(sig) => {
                info!("Transaction confirmed: {}", sig);
                Ok(TransactionResult::success(sig))
            }
            Err(e) => {
                error!("Transaction failed: {}", e);
                Ok(TransactionResult::failed(signature, e.to_string()))
            }
        }
    }

    /// Simulate the transaction before sending
    pub async fn simulate(&mut self) -> Result<()> {
        let transaction = self.build().await?;
        
        debug!("Simulating transaction: {}", transaction.signatures[0]);
        
        self.client.simulate_transaction(&transaction).await?;
        
        Ok(())
    }

    /// Build, simulate, then send if successful
    pub async fn simulate_and_send(&mut self) -> Result<Signature> {
        // Build transaction
        let transaction = self.build().await?;
        
        // Simulate first
        debug!("Simulating transaction before sending");
        self.client.simulate_transaction(&transaction).await?;
        
        // Send if simulation succeeded
        info!("Simulation successful, sending transaction");
        let signature = self.client.send_transaction(&transaction).await?;
        
        Ok(signature)
    }

    /// Reset the builder to reuse
    pub fn reset(&mut self) -> &mut Self {
        self.instructions.clear();
        self.compute_unit_limit = None;
        self.compute_unit_price = None;
        self.recent_blockhash = None;
        self
    }

    /// Get the wallet
    pub fn wallet(&self) -> &Arc<Wallet> {
        &self.wallet
    }

    /// Get the client
    pub fn client(&self) -> &Arc<SolanaClient> {
        &self.client
    }
}

/// Helper function to create a simple transfer transaction
pub async fn create_sol_transfer(
    client: Arc<SolanaClient>,
    wallet: Arc<Wallet>,
    to: &Pubkey,
    lamports: u64,
) -> Result<Transaction> {
    let instruction = solana_sdk::system_instruction::transfer(&wallet.pubkey(), to, lamports);
    
    let mut builder = TransactionBuilder::new(client, wallet);
    builder.add_instruction(instruction).build().await
}

/// Helper function to create a token transfer transaction
pub async fn create_token_transfer(
    client: Arc<SolanaClient>,
    wallet: Arc<Wallet>,
    from_token_account: &Pubkey,
    to_token_account: &Pubkey,
    amount: u64,
) -> Result<Transaction> {
    let instruction = spl_token::instruction::transfer(
        &spl_token::id(),
        from_token_account,
        to_token_account,
        &wallet.pubkey(),
        &[],
        amount,
    )?;
    
    let mut builder = TransactionBuilder::new(client, wallet);
    builder.add_instruction(instruction).build().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana::client::RpcConfig;

    #[test]
    fn test_transaction_result() {
        let sig = Signature::default();
        let result = TransactionResult::success(sig);
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_transaction_builder_empty() {
        let config = RpcConfig::default();
        let client = Arc::new(SolanaClient::new(config));
        let wallet = Arc::new(Wallet::new_random(client.clone()));
        
        let mut builder = TransactionBuilder::new(client, wallet);
        
        // Should fail with no instructions
        assert!(builder.build().await.is_err());
    }

    #[tokio::test]
    async fn test_transaction_builder_with_compute() {
        let config = RpcConfig::default();
        let client = Arc::new(SolanaClient::new(config));
        let wallet = Arc::new(Wallet::new_random(client.clone()));
        
        let mut builder = TransactionBuilder::new(client, wallet.clone());
        
        // Add a dummy instruction
        let ix = solana_sdk::system_instruction::transfer(
            &wallet.pubkey(),
            &Pubkey::new_unique(),
            1000,
        );
        
        builder
            .add_instruction(ix)
            .with_compute_limit(200_000)
            .with_compute_price(100_000);
        
        // Should build successfully (may fail to send without real connection)
        let result = builder.build().await;
        assert!(result.is_ok());
    }
}
