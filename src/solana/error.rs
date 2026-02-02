use thiserror::Error;

#[derive(Error, Debug)]
pub enum SolanaError {
    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Timeout error: operation timed out after {0}ms")]
    TimeoutError(u64),

    #[error("Transaction error: {0}")]
    TransactionError(String),

    #[error("Simulation error: {0}")]
    SimulationError(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Wallet error: {0}")]
    WalletError(String),

    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),

    #[error("Insufficient balance: required {required} SOL, available {available} SOL")]
    InsufficientBalance { required: f64, available: f64 },

    #[error("Jito error: {0}")]
    JitoError(String),

    #[error("Bundle error: {0}")]
    BundleError(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl From<solana_client::client_error::ClientError> for SolanaError {
    fn from(err: solana_client::client_error::ClientError) -> Self {
        SolanaError::RpcError(err.to_string())
    }
}

impl From<solana_sdk::signer::SignerError> for SolanaError {
    fn from(err: solana_sdk::signer::SignerError) -> Self {
        SolanaError::WalletError(err.to_string())
    }
}

impl From<reqwest::Error> for SolanaError {
    fn from(err: reqwest::Error) -> Self {
        SolanaError::NetworkError(err.to_string())
    }
}

impl From<serde_json::Error> for SolanaError {
    fn from(err: serde_json::Error) -> Self {
        SolanaError::SerializationError(err.to_string())
    }
}

impl From<bs58::decode::Error> for SolanaError {
    fn from(err: bs58::decode::Error) -> Self {
        SolanaError::ParseError(err.to_string())
    }
}


impl From<solana_sdk::program_error::ProgramError> for SolanaError {
    fn from(err: solana_sdk::program_error::ProgramError) -> Self {
        SolanaError::TransactionError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SolanaError>;
