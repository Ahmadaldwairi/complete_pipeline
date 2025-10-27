//! 🛡️ Anti-Churn Guardrails
//!
//! Prevents excessive trading that leads to losses from fees and slippage.
//! Enforces: loss backoff, position limits, rate limiting, wallet cooling.

use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::{VecDeque, HashMap};
use std::sync::{Arc, Mutex};
use log::{info, warn, debug};

/// Trade outcome for backoff tracking
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TradeOutcome {
    Win,
    Loss,
}

/// Entry for loss backoff tracking
#[derive(Debug, Clone)]
struct LossEntry {
    timestamp: u64,
    mint: [u8; 32],
}

/// Entry for wallet copy tracking
#[derive(Debug, Clone)]
struct WalletCopyEntry {
    wallet: [u8; 32],
    timestamp: u64,
    was_profitable: bool,
}

/// Entry for rate limiting
#[derive(Debug, Clone)]
struct RateLimitEntry {
    trigger_type: u8, // 0=rank, 1=momentum, 2=copy, 3=late
    timestamp: u64,
}

/// Configuration for guardrails
#[derive(Debug, Clone)]
pub struct GuardrailConfig {
    // Loss backoff
    pub loss_backoff_window_secs: u64,  // Default: 180 (3 min)
    pub loss_backoff_threshold: usize,  // Default: 3 losses
    pub loss_backoff_duration_secs: u64, // Default: 120 (2 min pause)
    
    // Position limits
    pub max_concurrent_positions: usize, // Default: 3
    pub max_advisor_positions: usize,    // Default: 2 (copy+late only)
    
    // Rate limiting
    pub advisor_rate_limit_secs: u64,    // Default: 30 (≤1 advisor entry per 30s)
    pub min_decision_interval_ms: u64,   // Default: 100 (general rate limit)
    
    // Wallet cooling
    pub wallet_cooling_period_secs: u64, // Default: 90 (no copy same wallet twice in 90s)
    pub tier_a_bypass_cooling: bool,     // Default: true (Tier A can bypass if last was profitable)
}

impl Default for GuardrailConfig {
    fn default() -> Self {
        Self {
            loss_backoff_window_secs: 180,
            loss_backoff_threshold: 3,
            loss_backoff_duration_secs: 120,
            max_concurrent_positions: 3,
            max_advisor_positions: 2,
            advisor_rate_limit_secs: 30,
            min_decision_interval_ms: 100,
            wallet_cooling_period_secs: 90,
            tier_a_bypass_cooling: true,
        }
    }
}

/// Anti-churn guardrails system
pub struct Guardrails {
    config: GuardrailConfig,
    
    // Loss backoff tracking
    recent_losses: Arc<Mutex<VecDeque<LossEntry>>>,
    backoff_until: Arc<Mutex<Option<u64>>>,
    
    // Position tracking
    open_positions: Arc<Mutex<HashMap<[u8; 32], bool>>>, // mint -> is_advisor
    
    // Rate limiting
    last_advisor_entry: Arc<Mutex<Option<u64>>>,
    last_decision: Arc<Mutex<Option<u64>>>,
    recent_entries: Arc<Mutex<VecDeque<RateLimitEntry>>>,
    
    // Wallet cooling
    wallet_copy_history: Arc<Mutex<VecDeque<WalletCopyEntry>>>,
}

impl Guardrails {
    /// Create new guardrails system with default configuration
    pub fn new() -> Self {
        Self::with_config(GuardrailConfig::default())
    }
    
    /// Create new guardrails system with custom configuration
    pub fn with_config(config: GuardrailConfig) -> Self {
        info!("🛡️ Initializing anti-churn guardrails:");
        info!("   Loss backoff: {} losses in {}s → pause {}s", 
              config.loss_backoff_threshold, 
              config.loss_backoff_window_secs,
              config.loss_backoff_duration_secs);
        info!("   Position limits: {} total, {} advisor", 
              config.max_concurrent_positions,
              config.max_advisor_positions);
        info!("   Rate limits: advisor {}s, general {}ms",
              config.advisor_rate_limit_secs,
              config.min_decision_interval_ms);
        info!("   Wallet cooling: {}s (Tier A bypass: {})",
              config.wallet_cooling_period_secs,
              config.tier_a_bypass_cooling);
        
        Self {
            config,
            recent_losses: Arc::new(Mutex::new(VecDeque::new())),
            backoff_until: Arc::new(Mutex::new(None)),
            open_positions: Arc::new(Mutex::new(HashMap::new())),
            last_advisor_entry: Arc::new(Mutex::new(None)),
            last_decision: Arc::new(Mutex::new(None)),
            recent_entries: Arc::new(Mutex::new(VecDeque::new())),
            wallet_copy_history: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
    
    /// Check if a new decision is allowed
    /// 
    /// Returns Ok(()) if allowed, Err(reason) if blocked.
    pub fn check_decision_allowed(
        &self,
        trigger_type: u8, // 0=rank, 1=momentum, 2=copy, 3=late
        _mint: &[u8; 32],
        wallet: Option<&[u8; 32]>, // For copy trades
        wallet_tier: Option<u8>,   // For copy trades (0=C, 1=B, 2=A)
    ) -> Result<(), String> {
        let now = Self::now_secs();
        
        // 1. Check loss backoff
        if let Some(until) = *self.backoff_until.lock().unwrap() {
            if now < until {
                let remaining = until - now;
                return Err(format!("Loss backoff active: {}s remaining", remaining));
            }
        }
        
        // 2. Check position limits
        let positions = self.open_positions.lock().unwrap();
        let total_positions = positions.len();
        let advisor_positions = positions.values().filter(|&&is_adv| is_adv).count();
        
        let is_advisor = trigger_type == 2 || trigger_type == 3; // copy or late
        
        if total_positions >= self.config.max_concurrent_positions {
            return Err(format!("Max positions reached: {}/{}", 
                              total_positions, 
                              self.config.max_concurrent_positions));
        }
        
        if is_advisor && advisor_positions >= self.config.max_advisor_positions {
            return Err(format!("Max advisor positions reached: {}/{}", 
                              advisor_positions,
                              self.config.max_advisor_positions));
        }
        
        drop(positions); // Release lock
        
        // 3. Check rate limits
        if is_advisor {
            if let Some(last) = *self.last_advisor_entry.lock().unwrap() {
                let elapsed = now - last;
                if elapsed < self.config.advisor_rate_limit_secs {
                    let remaining = self.config.advisor_rate_limit_secs - elapsed;
                    return Err(format!("Advisor rate limit: {}s remaining", remaining));
                }
            }
        }
        
        // General rate limit (100ms minimum between any decisions)
        if let Some(last) = *self.last_decision.lock().unwrap() {
            let now_ms = Self::now_millis();
            let last_ms = last * 1000; // Convert stored secs to ms for comparison
            let elapsed_ms = now_ms.saturating_sub(last_ms);
            
            if elapsed_ms < self.config.min_decision_interval_ms {
                return Err(format!("General rate limit: {}ms since last decision", elapsed_ms));
            }
        }
        
        // 4. Check wallet cooling (for copy trades only)
        if trigger_type == 2 && wallet.is_some() {
            let wallet_pubkey = wallet.unwrap();
            let is_tier_a = wallet_tier == Some(2);
            
            let mut history = self.wallet_copy_history.lock().unwrap();
            
            // Clean old entries
            history.retain(|entry| now - entry.timestamp < self.config.wallet_cooling_period_secs);
            
            // Check if this wallet was recently copied
            if let Some(last_copy) = history.iter()
                .filter(|e| &e.wallet == wallet_pubkey)
                .last() 
            {
                let elapsed = now - last_copy.timestamp;
                
                // Tier A bypass: allow if last copy was profitable
                if is_tier_a && self.config.tier_a_bypass_cooling && last_copy.was_profitable {
                    debug!("✅ Tier A wallet cooling bypassed (last copy was profitable)");
                } else if elapsed < self.config.wallet_cooling_period_secs {
                    return Err(format!("Wallet cooling: {}s since last copy ({}s required)", 
                                      elapsed, 
                                      self.config.wallet_cooling_period_secs));
                }
            }
        }
        
        Ok(())
    }
    
    /// Record a new decision (call after check_decision_allowed succeeds)
    pub fn record_decision(
        &self,
        trigger_type: u8,
        mint: &[u8; 32],
        wallet: Option<&[u8; 32]>,
    ) {
        let now = Self::now_secs();
        
        let is_advisor = trigger_type == 2 || trigger_type == 3;
        
        // Update position tracking
        self.open_positions.lock().unwrap().insert(*mint, is_advisor);
        
        // Update rate limit tracking
        *self.last_decision.lock().unwrap() = Some(now);
        
        if is_advisor {
            *self.last_advisor_entry.lock().unwrap() = Some(now);
        }
        
        // Record entry for stats
        let mut entries = self.recent_entries.lock().unwrap();
        entries.push_back(RateLimitEntry {
            trigger_type,
            timestamp: now,
        });
        
        // Keep last 100 entries
        while entries.len() > 100 {
            entries.pop_front();
        }
        
        // Record wallet copy
        if trigger_type == 2 && wallet.is_some() {
            let mut history = self.wallet_copy_history.lock().unwrap();
            history.push_back(WalletCopyEntry {
                wallet: *wallet.unwrap(),
                timestamp: now,
                was_profitable: false, // Will be updated on close
            });
            
            // Keep last 200 copies
            while history.len() > 200 {
                history.pop_front();
            }
        }
        
        debug!("📝 Recorded decision: trigger={}, mint={}...", 
               trigger_type, 
               hex::encode(&mint[..4]));
    }
    
    /// Record trade outcome (win/loss) for backoff tracking
    pub fn record_outcome(
        &self,
        mint: &[u8; 32],
        outcome: TradeOutcome,
        wallet: Option<&[u8; 32]>, // For updating wallet copy history
    ) {
        let now = Self::now_secs();
        
        // Remove from open positions
        self.open_positions.lock().unwrap().remove(mint);
        
        // Update wallet copy history if applicable
        if outcome == TradeOutcome::Win && wallet.is_some() {
            let wallet_pubkey = wallet.unwrap();
            let mut history = self.wallet_copy_history.lock().unwrap();
            
            // Find and mark the most recent copy as profitable
            if let Some(entry) = history.iter_mut()
                .rev()
                .find(|e| &e.wallet == wallet_pubkey)
            {
                entry.was_profitable = true;
            }
        }
        
        // Track losses for backoff
        if outcome == TradeOutcome::Loss {
            let mut losses = self.recent_losses.lock().unwrap();
            
            // Add new loss
            losses.push_back(LossEntry {
                timestamp: now,
                mint: *mint,
            });
            
            // Remove losses outside window
            let window_start = now.saturating_sub(self.config.loss_backoff_window_secs);
            losses.retain(|entry| entry.timestamp >= window_start);
            
            // Check if we hit the threshold
            if losses.len() >= self.config.loss_backoff_threshold {
                let backoff_until = now + self.config.loss_backoff_duration_secs;
                *self.backoff_until.lock().unwrap() = Some(backoff_until);
                
                warn!("⚠️ LOSS BACKOFF TRIGGERED: {} losses in {}s → pausing until {}s from now",
                      losses.len(),
                      self.config.loss_backoff_window_secs,
                      self.config.loss_backoff_duration_secs);
                
                // Clear loss history after triggering backoff
                losses.clear();
            }
            
            info!("❌ Loss recorded: mint={}... (recent losses: {}/{})",
                  hex::encode(&mint[..4]),
                  losses.len(),
                  self.config.loss_backoff_threshold);
        } else {
            info!("✅ Win recorded: mint={}...", hex::encode(&mint[..4]));
        }
    }
    
    /// Get current statistics
    pub fn stats(&self) -> GuardrailStats {
        let now = Self::now_secs();
        
        let positions = self.open_positions.lock().unwrap();
        let advisor_positions = positions.values().filter(|&&is_adv| is_adv).count();
        
        let backoff_remaining = if let Some(until) = *self.backoff_until.lock().unwrap() {
            until.saturating_sub(now)
        } else {
            0
        };
        
        let recent_losses_count = self.recent_losses.lock().unwrap().len();
        let wallet_copies_tracked = self.wallet_copy_history.lock().unwrap().len();
        
        GuardrailStats {
            open_positions: positions.len(),
            advisor_positions,
            backoff_remaining_secs: backoff_remaining,
            recent_losses_count,
            wallet_copies_tracked,
        }
    }
    
    /// Print statistics summary
    pub fn print_stats(&self) {
        let stats = self.stats();
        info!("🛡️ Guardrail Statistics:");
        info!("   Open positions: {} (advisor: {})", stats.open_positions, stats.advisor_positions);
        info!("   Backoff remaining: {}s", stats.backoff_remaining_secs);
        info!("   Recent losses: {}/{}", stats.recent_losses_count, self.config.loss_backoff_threshold);
        info!("   Wallet copies tracked: {}", stats.wallet_copies_tracked);
    }
    
    // Helper functions
    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
    
    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

/// Statistics snapshot
#[derive(Debug, Clone)]
pub struct GuardrailStats {
    pub open_positions: usize,
    pub advisor_positions: usize,
    pub backoff_remaining_secs: u64,
    pub recent_losses_count: usize,
    pub wallet_copies_tracked: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_guardrails_initialization() {
        let guardrails = Guardrails::new();
        let stats = guardrails.stats();
        
        assert_eq!(stats.open_positions, 0);
        assert_eq!(stats.advisor_positions, 0);
        assert_eq!(stats.backoff_remaining_secs, 0);
    }
    
    #[test]
    fn test_position_limits() {
        let config = GuardrailConfig {
            max_concurrent_positions: 2,
            max_advisor_positions: 1,
            ..Default::default()
        };
        
        let guardrails = Guardrails::with_config(config);
        let mint1 = [1u8; 32];
        let mint2 = [2u8; 32];
        let mint3 = [3u8; 32];
        
        // Allow first position (rank-based)
        assert!(guardrails.check_decision_allowed(0, &mint1, None, None).is_ok());
        guardrails.record_decision(0, &mint1, None);
        
        // Allow second position (advisor)
        assert!(guardrails.check_decision_allowed(2, &mint2, None, None).is_ok());
        guardrails.record_decision(2, &mint2, None);
        
        // Block third position (max total reached)
        assert!(guardrails.check_decision_allowed(0, &mint3, None, None).is_err());
    }
    
    #[test]
    fn test_outcome_recording() {
        let guardrails = Guardrails::new();
        let mint = [1u8; 32];
        
        guardrails.record_decision(0, &mint, None);
        assert_eq!(guardrails.stats().open_positions, 1);
        
        guardrails.record_outcome(&mint, TradeOutcome::Win, None);
        assert_eq!(guardrails.stats().open_positions, 0);
    }
    
    #[test]
    fn test_config_defaults() {
        let config = GuardrailConfig::default();
        assert_eq!(config.loss_backoff_threshold, 3);
        assert_eq!(config.max_concurrent_positions, 3);
        assert_eq!(config.advisor_rate_limit_secs, 30);
    }
}
