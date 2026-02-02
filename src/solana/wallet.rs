use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
    program_pack::Pack,
};
use spl_token::id as token_program_id;
use spl_associated_token_account::{
    get_associated_token_address,
    instruction::create_associated_token_account,
};
use std::sync::Arc;
use std::path::Path;
use tracing::{info, debug, warn};

use crate::solana::error::{Result, SolanaError};
use crate::solana::client::SolanaClient;

/// Wallet for managing keypair and signing transactions
pub struct Wallet {
    keypair: Arc<Keypair>,
    client: Arc<SolanaClient>,
}

impl Wallet {
    /// Create a new wallet from a base58 encoded private key
    pub fn from_base58(private_key: &str, client: Arc<SolanaClient>) -> Result<Self> {
        let decoded = bs58::decode(private_key)
            .into_vec()
            .map_err(|e| SolanaError::InvalidKeypair(format!("Failed to decode base58: {}", e)))?;

        let keypair = Keypair::from_bytes(&decoded)
            .map_err(|e| SolanaError::InvalidKeypair(format!("Failed to create keypair: {}", e)))?;

        info!("Wallet loaded: {}", keypair.pubkey());

        Ok(Self {
            keypair: Arc::new(keypair),
            client,
        })
    }

    /// Create a new wallet from environment variable
    pub fn from_env(client: Arc<SolanaClient>) -> Result<Self> {
        let private_key = std::env::var("WALLET_PRIVATE_KEY")
            .map_err(|_| SolanaError::WalletError(
                "WALLET_PRIVATE_KEY environment variable not set".to_string()
            ))?;

        Self::from_base58(&private_key, client)
    }

    /// Load wallet from file or environment variable
    /// Priority: 1. WALLET_PRIVATE_KEY env var, 2. WALLET_KEYFILE env var, 3. default path
    pub fn load(client: Arc<SolanaClient>) -> Result<Self> {
        // Try environment variable first
        if let Ok(private_key) = std::env::var("WALLET_PRIVATE_KEY") {
            info!("Loading wallet from WALLET_PRIVATE_KEY environment variable");
            return Self::from_base58(&private_key, client);
        }

        // Try keyfile path from environment
        let keyfile_path = std::env::var("WALLET_KEYFILE")
            .unwrap_or_else(|_| "wallet.json".to_string());

        // Load from file
        info!("Loading wallet from file: {}", keyfile_path);
        Self::from_file(&keyfile_path, client)
    }

    /// Create a new wallet from a keyfile (JSON format)
    pub fn from_file<P: AsRef<Path>>(path: P, client: Arc<SolanaClient>) -> Result<Self> {
        let path = path.as_ref();
        
        if !path.exists() {
            return Err(SolanaError::WalletError(
                format!("Wallet file not found: {}", path.display())
            ));
        }

        // Read file content
        let content = std::fs::read_to_string(path)
            .map_err(|e| SolanaError::IoError(e))?;

        // Try to parse as JSON array first (Solana CLI format)
        let private_key = if let Ok(json_array) = serde_json::from_str::<Vec<u8>>(&content) {
            // JSON array format [1,2,3,...]
            if json_array.len() != 64 {
                return Err(SolanaError::InvalidKeypair(
                    format!("Invalid keypair length: expected 64 bytes, got {}", json_array.len())
                ));
            }
            Keypair::from_bytes(&json_array)
                .map_err(|e| SolanaError::InvalidKeypair(format!("Failed to create keypair from JSON: {}", e)))?
        } else {
            // Try as base58 string
            let trimmed = content.trim();
            // Remove quotes if present
            let trimmed = trimmed.trim_matches('"').trim_matches('\'');
            
            let decoded = bs58::decode(trimmed)
                .into_vec()
                .map_err(|e| SolanaError::InvalidKeypair(format!("Failed to decode base58 from file: {}", e)))?;

            Keypair::from_bytes(&decoded)
                .map_err(|e| SolanaError::InvalidKeypair(format!("Failed to create keypair from file: {}", e)))?
        };

        info!("Wallet loaded from file: {}", private_key.pubkey());

        Ok(Self {
            keypair: Arc::new(private_key),
            client,
        })
    }

    /// Create a new random wallet (for testing)
    pub fn new_random(client: Arc<SolanaClient>) -> Self {
        let keypair = Keypair::new();
        info!("Generated new random wallet: {}", keypair.pubkey());

        Self {
            keypair: Arc::new(keypair),
            client,
        }
    }

    /// Get the public key
    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    /// Get the keypair
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Sign a transaction
    pub fn sign_transaction(&self, transaction: &mut Transaction) -> Result<()> {
        // Get recent blockhash if not set
        if transaction.message.recent_blockhash == solana_sdk::hash::Hash::default() {
            return Err(SolanaError::TransactionError(
                "Transaction blockhash not set. Call get_latest_blockhash() first.".to_string()
            ));
        }
        
        transaction.sign(&[&*self.keypair], transaction.message.recent_blockhash);
        debug!("Transaction signed by {}", self.pubkey());
        Ok(())
    }

    /// Sign multiple transactions
    pub fn sign_transactions(&self, transactions: &mut [Transaction]) -> Result<()> {
        for tx in transactions.iter_mut() {
            self.sign_transaction(tx)?;
        }
        Ok(())
    }

    /// Check SOL balance
    pub async fn get_sol_balance(&self) -> Result<u64> {
        let balance = self.client.get_balance(&self.pubkey()).await?;
        debug!("SOL balance: {} lamports ({} SOL)", balance, balance as f64 / 1e9);
        Ok(balance)
    }

    /// Check if wallet has sufficient SOL balance
    pub async fn has_sufficient_balance(&self, required_lamports: u64) -> Result<bool> {
        let balance = self.get_sol_balance().await?;
        Ok(balance >= required_lamports)
    }

    /// Verify sufficient balance or return error
    pub async fn verify_balance(&self, required_lamports: u64) -> Result<()> {
        let balance = self.get_sol_balance().await?;
        
        if balance < required_lamports {
            return Err(SolanaError::InsufficientBalance {
                required: required_lamports as f64 / 1e9,
                available: balance as f64 / 1e9,
            });
        }

        Ok(())
    }

    /// Get associated token account address for a mint
    pub fn get_associated_token_address(&self, mint: &Pubkey) -> Pubkey {
        get_associated_token_address(&self.pubkey(), mint)
    }

    /// Get all token accounts owned by this wallet
    pub async fn get_token_accounts(&self) -> Result<Vec<(Pubkey, spl_token::state::Account)>> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder::UiAccountEncoding;
        use solana_client::rpc_filter::{RpcFilterType, Memcmp};
        
        let token_program = spl_token::id();
        let owner_pubkey = self.pubkey();
        let owner_bytes = owner_pubkey.as_ref();
        
        // Use getProgramAccounts with owner filter
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
            ..Default::default()
        };

        let owner_filter = RpcFilterType::Memcmp(
            Memcmp::new_base58_encoded(32, owner_bytes) // Owner is at offset 32 in token account
        );

        let accounts = self.client.rpc()
            .get_program_accounts_with_config(
                &token_program,
                solana_client::rpc_config::RpcProgramAccountsConfig {
                    filters: Some(vec![owner_filter]),
                    account_config: config,
                    ..Default::default()
                },
            )
            .map_err(|e| SolanaError::RpcError(format!("Failed to get token accounts: {}", e)))?;
        
        let mut token_accounts = Vec::new();
        for (pubkey, account) in accounts {
            // Parse token account data
            if let Ok(token_account) = spl_token::state::Account::unpack(&account.data) {
                token_accounts.push((pubkey, token_account));
            }
        }

        Ok(token_accounts)
    }

    /// Get token balance for a specific mint
    pub async fn get_token_balance(&self, mint: &Pubkey) -> Result<Option<u64>> {
        let ata = self.get_associated_token_address(mint);
        
        match self.client.get_account_optional(&ata).await? {
            Some(account) => {
                let token_account = spl_token::state::Account::unpack(&account.data)
                    .map_err(|e| SolanaError::ParseError(format!("Failed to parse token account: {}", e)))?;
                
                Ok(Some(token_account.amount))
            }
            None => Ok(None),
        }
    }

    /// Check if token account exists
    pub async fn token_account_exists(&self, mint: &Pubkey) -> Result<bool> {
        let ata = self.get_associated_token_address(mint);
        Ok(self.client.get_account_optional(&ata).await?.is_some())
    }

    /// Create associated token account instruction
    pub fn create_token_account_instruction(
        &self,
        mint: &Pubkey,
        payer: &Pubkey,
    ) -> solana_sdk::instruction::Instruction {
        create_associated_token_account(
            payer,
            &self.pubkey(),
            mint,
            &token_program_id(),
        )
    }

    /// Create associated token account if it doesn't exist
    pub async fn create_token_account_if_needed(
        &self,
        mint: &Pubkey,
    ) -> Result<Option<solana_sdk::instruction::Instruction>> {
        if !self.token_account_exists(mint).await? {
            info!("Creating associated token account for mint: {}", mint);
            Ok(Some(self.create_token_account_instruction(mint, &self.pubkey())))
        } else {
            debug!("Token account already exists for mint: {}", mint);
            Ok(None)
        }
    }

    /// Export private key as base58 (use with caution!)
    pub fn export_private_key(&self) -> String {
        bs58::encode(self.keypair.to_bytes()).into_string()
    }

    /// Get the client reference
    pub fn client(&self) -> &Arc<SolanaClient> {
        &self.client
    }

    /// Clone the keypair Arc
    pub fn keypair_arc(&self) -> Arc<Keypair> {
        Arc::clone(&self.keypair)
    }
}

// Implement Clone for Wallet
impl Clone for Wallet {
    fn clone(&self) -> Self {
        Self {
            keypair: Arc::clone(&self.keypair),
            client: Arc::clone(&self.client),
        }
    }
}

// Implement Display for Wallet
impl std::fmt::Display for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Wallet({})", self.pubkey())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solana::client::RpcConfig;

    #[test]
    fn test_random_wallet() {
        let config = RpcConfig::default();
        let client = Arc::new(SolanaClient::new(config));
        let wallet = Wallet::new_random(client);
        
        // Verify pubkey is not default
        assert_ne!(wallet.pubkey(), Pubkey::default());
    }

    #[test]
    fn test_export_import() {
        let config = RpcConfig::default();
        let client = Arc::new(SolanaClient::new(config));
        let wallet1 = Wallet::new_random(client.clone());
        
        let private_key = wallet1.export_private_key();
        let wallet2 = Wallet::from_base58(&private_key, client).unwrap();
        
        assert_eq!(wallet1.pubkey(), wallet2.pubkey());
    }
}
