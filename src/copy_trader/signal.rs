use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::fmt;

/// Trade action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeAction {
    Buy,
    Sell,
}

impl fmt::Display for TradeAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TradeAction::Buy => write!(f, "Buy"),
            TradeAction::Sell => write!(f, "Sell"),
        }
    }
}

/// Trade signal generated from a decoded transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSignal {
    /// Unique transaction signature
    pub signature: String,
    
    /// Token mint address
    pub token_mint: Pubkey,
    
    /// Amount of SOL involved in the trade
    pub sol_amount: f64,
    
    /// Trade action (buy/sell)
    pub action: TradeAction,
    
    /// Timestamp of the transaction
    pub timestamp: DateTime<Utc>,
    
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    
    /// Source DEX program
    pub dex_program: String,
    
    /// Token amount (if available)
    pub token_amount: Option<u64>,
    
    /// Is this a pump.fun transaction
    pub is_pump_fun: bool,
    
    /// Wallet that made the trade
    pub wallet: Pubkey,
}

impl TradeSignal {
    /// Create a new trade signal
    pub fn new(
        signature: String,
        token_mint: Pubkey,
        sol_amount: f64,
        action: TradeAction,
        timestamp: DateTime<Utc>,
        dex_program: String,
        wallet: Pubkey,
    ) -> Self {
        let confidence = Self::calculate_confidence(sol_amount);
        
        Self {
            signature,
            token_mint,
            sol_amount,
            action,
            timestamp,
            confidence,
            dex_program,
            token_amount: None,
            is_pump_fun: false,
            wallet,
        }
    }
    
    /// Calculate confidence score based on trade size
    /// Larger trades get higher confidence scores
    fn calculate_confidence(sol_amount: f64) -> f64 {
        if sol_amount >= 10.0 {
            1.0
        } else if sol_amount >= 5.0 {
            0.9
        } else if sol_amount >= 1.0 {
            0.8
        } else if sol_amount >= 0.5 {
            0.7
        } else if sol_amount >= 0.1 {
            0.6
        } else {
            0.5
        }
    }
    
    /// Set token amount
    pub fn with_token_amount(mut self, amount: u64) -> Self {
        self.token_amount = Some(amount);
        self
    }
    
    /// Set pump.fun flag
    pub fn with_pump_fun(mut self, is_pump_fun: bool) -> Self {
        self.is_pump_fun = is_pump_fun;
        self
    }
    
    /// Set custom confidence
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
    
    /// Validate the signal
    pub fn is_valid(&self) -> bool {
        self.sol_amount > 0.0 && self.confidence > 0.0
    }
    
    /// Check if the signal meets minimum trade size
    pub fn meets_minimum(&self, min_size: f64) -> bool {
        self.sol_amount >= min_size
    }
    
    /// Calculate adjusted amount based on copy percentage
    pub fn adjusted_amount(&self, copy_percentage: f64) -> f64 {
        (self.sol_amount * copy_percentage).max(0.0)
    }
    
    /// Check if adjusted amount exceeds maximum position size
    pub fn exceeds_max_position(&self, copy_percentage: f64, max_position: f64) -> bool {
        self.adjusted_amount(copy_percentage) > max_position
    }
}

/// Signal filter for processing trade signals
pub struct SignalFilter {
    /// Only accept pump.fun transactions
    pump_fun_only: bool,
    
    /// Minimum confidence threshold
    min_confidence: f64,
    
    /// Recently seen signatures (for duplicate detection)
    seen_signatures: HashSet<String>,
    
    /// Maximum number of signatures to track
    max_signatures_tracked: usize,
}

impl SignalFilter {
    /// Create a new signal filter
    pub fn new(pump_fun_only: bool, min_confidence: f64) -> Self {
        Self {
            pump_fun_only,
            min_confidence,
            seen_signatures: HashSet::new(),
            max_signatures_tracked: 10000,
        }
    }
    
    /// Filter a trade signal
    pub fn filter(&mut self, signal: &TradeSignal) -> FilterResult {
        // Check if already seen (duplicate)
        if self.seen_signatures.contains(&signal.signature) {
            return FilterResult::Duplicate;
        }
        
        // Basic validation
        if !signal.is_valid() {
            return FilterResult::Invalid("Signal validation failed".to_string());
        }
        
        // Check pump.fun requirement
        if self.pump_fun_only && !signal.is_pump_fun {
            return FilterResult::Filtered("Not a pump.fun transaction".to_string());
        }
        
        // Check confidence threshold
        if signal.confidence < self.min_confidence {
            return FilterResult::Filtered(format!(
                "Confidence {} below threshold {}",
                signal.confidence, self.min_confidence
            ));
        }
        
        // Add to seen signatures
        self.seen_signatures.insert(signal.signature.clone());
        
        // Limit the size of seen_signatures
        if self.seen_signatures.len() > self.max_signatures_tracked {
            // Remove oldest entries (simple approach: clear half)
            let to_remove: Vec<String> = self
                .seen_signatures
                .iter()
                .take(self.max_signatures_tracked / 2)
                .cloned()
                .collect();
            
            for sig in to_remove {
                self.seen_signatures.remove(&sig);
            }
        }
        
        FilterResult::Accepted
    }
    
    /// Check if a signature has been seen
    pub fn has_seen(&self, signature: &str) -> bool {
        self.seen_signatures.contains(signature)
    }
    
    /// Clear seen signatures
    pub fn clear_history(&mut self) {
        self.seen_signatures.clear();
    }
}

/// Result of filtering a signal
#[derive(Debug, Clone, PartialEq)]
pub enum FilterResult {
    /// Signal accepted
    Accepted,
    
    /// Signal is a duplicate
    Duplicate,
    
    /// Signal filtered out with reason
    Filtered(String),
    
    /// Signal is invalid with reason
    Invalid(String),
}

impl FilterResult {
    pub fn is_accepted(&self) -> bool {
        matches!(self, FilterResult::Accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    
    fn create_test_signal() -> TradeSignal {
        TradeSignal::new(
            "test_signature_123".to_string(),
            Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap(),
            1.0,
            TradeAction::Buy,
            Utc::now(),
            "DFlow".to_string(),
            Pubkey::from_str("54Pz1e35z9uoFdnxtzjp7xZQoFiofqhdayQWBMN7dsuy").unwrap(),
        )
    }
    
    #[test]
    fn test_confidence_calculation() {
        assert_eq!(TradeSignal::calculate_confidence(15.0), 1.0);
        assert_eq!(TradeSignal::calculate_confidence(7.0), 0.9);
        assert_eq!(TradeSignal::calculate_confidence(2.0), 0.8);
        assert_eq!(TradeSignal::calculate_confidence(0.05), 0.5);
    }
    
    #[test]
    fn test_signal_validation() {
        let signal = create_test_signal();
        assert!(signal.is_valid());
        
        let invalid_signal = TradeSignal::new(
            "test".to_string(),
            Pubkey::default(),
            -1.0,
            TradeAction::Buy,
            Utc::now(),
            "DFlow".to_string(),
            Pubkey::default(),
        );
        assert!(!invalid_signal.is_valid());
    }
    
    #[test]
    fn test_adjusted_amount() {
        let signal = create_test_signal();
        assert_eq!(signal.adjusted_amount(1.0), 1.0);
        assert_eq!(signal.adjusted_amount(0.5), 0.5);
        assert_eq!(signal.adjusted_amount(2.0), 2.0);
    }
    
    #[test]
    fn test_signal_filter() {
        let mut filter = SignalFilter::new(false, 0.6);
        let signal = create_test_signal();
        
        // First time should accept
        assert_eq!(filter.filter(&signal), FilterResult::Accepted);
        
        // Second time should be duplicate
        assert_eq!(filter.filter(&signal), FilterResult::Duplicate);
    }
    
    #[test]
    fn test_pump_fun_filter() {
        let mut filter = SignalFilter::new(true, 0.5);
        let mut signal = create_test_signal();
        
        // Not pump.fun, should be filtered
        let result = filter.filter(&signal);
        assert!(matches!(result, FilterResult::Filtered(_)));
        
        // Mark as pump.fun
        signal.is_pump_fun = true;
        filter.clear_history();
        
        // Should now be accepted
        assert_eq!(filter.filter(&signal), FilterResult::Accepted);
    }
}
