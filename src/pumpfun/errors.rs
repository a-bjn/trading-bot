use thiserror::Error;

/// Custom error types for pump.fun integration
#[derive(Error, Debug)]
pub enum PumpFunError {
    #[error("Bonding curve has migrated to Raydium")]
    BondingCurveMigrated,

    #[error("Insufficient liquidity: requested {requested}, available {available}")]
    InsufficientLiquidity { requested: u64, available: u64 },

    #[error("Slippage tolerance exceeded: expected {expected}, got {actual}")]
    SlippageExceeded { expected: u64, actual: u64 },

    #[error("Price impact too high: {impact_bps} basis points")]
    PriceImpactTooHigh { impact_bps: u64 },

    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    #[error("Invalid bonding curve state")]
    InvalidBondingCurveState,

    #[error("Bonding curve not found for mint: {0}")]
    BondingCurveNotFound(String),

    #[error("Failed to deserialize account data")]
    DeserializationError,

    #[error("Failed to fetch account: {0}")]
    AccountFetchError(String),

    #[error("Arithmetic overflow in calculation")]
    ArithmeticOverflow,

    #[error("Arithmetic underflow in calculation")]
    ArithmeticUnderflow,

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Invalid PDA derivation")]
    InvalidPDA,

    #[error("Token account not found")]
    TokenAccountNotFound,

    #[error("Insufficient token balance: required {required}, available {available}")]
    InsufficientTokenBalance { required: u64, available: u64 },

    #[error("Insufficient SOL balance: required {required}, available {available}")]
    InsufficientSOLBalance { required: u64, available: u64 },

    #[error("Transaction simulation failed: {0}")]
    SimulationFailed(String),

    #[error("Transaction confirmation timeout")]
    ConfirmationTimeout,

    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}

impl From<solana_client::client_error::ClientError> for PumpFunError {
    fn from(error: solana_client::client_error::ClientError) -> Self {
        PumpFunError::RpcError(error.to_string())
    }
}

impl From<std::io::Error> for PumpFunError {
    fn from(error: std::io::Error) -> Self {
        PumpFunError::NetworkError(error.to_string())
    }
}

/// Result type alias for pump.fun operations
pub type PumpFunResult<T> = Result<T, PumpFunError>;
