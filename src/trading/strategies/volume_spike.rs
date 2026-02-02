use anyhow::Result;
use crate::config::VolumeSpikeScalpingConfig;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Volume data point for a token
#[derive(Debug, Clone)]
struct VolumeDataPoint {
    timestamp: SystemTime,
    volume_sol: f64,
}

/// Volume history tracker
#[derive(Debug)]
struct VolumeTracker {
    // Token mint -> list of volume data points
    volume_history: HashMap<String, Vec<VolumeDataPoint>>,
    // Detection window duration
    window_duration: Duration,
}

impl VolumeTracker {
    fn new(window_seconds: u64) -> Self {
        Self {
            volume_history: HashMap::new(),
            window_duration: Duration::from_secs(window_seconds),
        }
    }
    
    /// Add a volume data point
    fn add_volume(&mut self, token_mint: &str, volume_sol: f64) {
        let now = SystemTime::now();
        let data_point = VolumeDataPoint {
            timestamp: now,
            volume_sol,
        };
        
        self.volume_history
            .entry(token_mint.to_string())
            .or_insert_with(Vec::new)
            .push(data_point);
        
        // Clean old data points
        self.clean_old_data(token_mint);
    }
    
    /// Remove data points outside the detection window
    fn clean_old_data(&mut self, token_mint: &str) {
        if let Some(history) = self.volume_history.get_mut(token_mint) {
            let now = SystemTime::now();
            let cutoff = now - self.window_duration;
            
            history.retain(|point| {
                point.timestamp >= cutoff
            });
        }
    }
    
    /// Calculate current volume within the window
    fn get_current_volume(&mut self, token_mint: &str) -> f64 {
        self.clean_old_data(token_mint);
        
        self.volume_history
            .get(token_mint)
            .map(|history| {
                history.iter().map(|p| p.volume_sol).sum()
            })
            .unwrap_or(0.0)
    }
    
    /// Calculate base volume (excluding recent spike period)
    fn get_base_volume(&mut self, token_mint: &str, spike_window_seconds: u64) -> f64 {
        self.clean_old_data(token_mint);
        
        let now = SystemTime::now();
        let spike_cutoff = now - Duration::from_secs(spike_window_seconds);
        
        self.volume_history
            .get(token_mint)
            .map(|history| {
                history.iter()
                    .filter(|p| p.timestamp < spike_cutoff)
                    .map(|p| p.volume_sol)
                    .sum()
            })
            .unwrap_or(0.0)
    }
}

pub struct VolumeSpikeStrategy {
    config: VolumeSpikeScalpingConfig,
    volume_tracker: Arc<RwLock<VolumeTracker>>,
}

impl VolumeSpikeStrategy {
    pub fn new(config: VolumeSpikeScalpingConfig) -> Self {
        let tracker = VolumeTracker::new(config.detection_window_seconds);
        
        Self {
            config,
            volume_tracker: Arc::new(RwLock::new(tracker)),
        }
    }
    
    /// Record a trade volume for spike detection
    pub async fn record_volume(&self, token_mint: &str, volume_sol: f64) {
        let mut tracker = self.volume_tracker.write().await;
        tracker.add_volume(token_mint, volume_sol);
    }
    
    /// Detect if there's a volume spike
    pub async fn detect_spike(&self, token_mint: &str) -> Result<bool> {
        let mut tracker = self.volume_tracker.write().await;
        
        // Get current volume in detection window
        let current_volume = tracker.get_current_volume(token_mint);
        
        // Check minimum base volume
        if current_volume < self.config.min_base_volume_sol {
            debug!("Volume too low: {} SOL < {} SOL", current_volume, self.config.min_base_volume_sol);
            return Ok(false);
        }
        
        // Calculate base volume (excluding recent spike period)
        let spike_window = 10; // Look at last 10 seconds for spike
        let base_volume = tracker.get_base_volume(token_mint, spike_window);
        
        if base_volume < self.config.min_base_volume_sol {
            // Not enough historical data
            debug!("Insufficient base volume data: {} SOL", base_volume);
            return Ok(false);
        }
        
        // Calculate volume increase percentage
        let volume_increase_pct = ((current_volume - base_volume) / base_volume) * 100.0;
        
        // Check if spike exceeds threshold
        let is_spike = volume_increase_pct >= self.config.spike_threshold_percentage;
        
        if is_spike {
            info!(
                "Volume spike detected for {}: {:.2}% increase (current: {:.4} SOL, base: {:.4} SOL)",
                token_mint, volume_increase_pct, current_volume, base_volume
            );
        }
        
        Ok(is_spike)
    }

    /// Check if we should enter a trade based on entry criteria
    pub async fn should_enter(&self, token_mint: &str) -> Result<bool> {
        // Parse token mint
        let _mint_pubkey = token_mint.parse::<Pubkey>()
            .map_err(|e| anyhow::anyhow!("Invalid token mint: {}", e))?;
        
        // TODO: Implement comprehensive entry checks:
        // 1. Check liquidity is within range
        // 2. Verify minimum holder count
        // 3. Validate market cap range
        // 4. Check token age
        // 5. Verify no rug pull indicators
        
        // For now, use simplified logic
        // In production, you'd fetch these from RPC/indexer
        
        info!("Entry criteria check for {}: Basic checks passed", token_mint);
        
        // Return true for now - implement real checks based on your needs
        Ok(true)
    }
    
    /// Check liquidity is within acceptable range
    pub async fn check_liquidity(&self, _token_mint: &str, liquidity_sol: f64) -> bool {
        liquidity_sol >= self.config.min_liquidity_sol 
            && liquidity_sol <= self.config.max_liquidity_sol
    }
    
    /// Get entry amount for this trade
    pub fn get_entry_amount(&self) -> f64 {
        self.config.entry_amount_sol
    }
    
    /// Get stop loss percentage
    pub fn get_stop_loss_percentage(&self) -> f64 {
        self.config.stop_loss_percentage
    }
    
    /// Get take profit percentage
    pub fn get_take_profit_percentage(&self) -> f64 {
        self.config.quick_profit_target_percentage
    }
    
    /// Get maximum hold time in seconds
    pub fn get_max_hold_time(&self) -> u64 {
        self.config.quick_profit_time_limit_seconds
    }
}
