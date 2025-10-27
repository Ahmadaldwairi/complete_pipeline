//! 📊 Position Tracker - Monitor active trades and trigger exits
//!
//! Tracks open positions, monitors price movements, and generates SELL decisions
//! when profit targets hit, stop losses trigger, or time decay occurs.

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use log::{info, warn, debug};
use crate::udp_bus::TradeDecision;
use crate::feature_cache::MintFeatures;

/// Active trading position
#[derive(Debug, Clone)]
pub struct ActivePosition {
    /// Token mint address (hex string)
    pub mint: String,
    
    /// Entry time
    pub entry_time: Instant,
    
    /// Entry timestamp (unix seconds)
    pub entry_timestamp: u64,
    
    /// Position size in SOL
    pub size_sol: f64,
    
    /// Position size in USD at entry
    pub size_usd: f64,
    
    /// Entry price (SOL per token, estimated)
    pub entry_price_sol: f64,
    
    /// Tokens received
    pub tokens: f64,
    
    /// Confidence score at entry (0-100)
    pub entry_confidence: u8,
    
    /// Profit targets (tier1, tier2, tier3) in % gain
    pub profit_targets: (f64, f64, f64),
    
    /// Stop loss threshold in % loss
    pub stop_loss_pct: f64,
    
    /// Maximum hold time in seconds
    pub max_hold_secs: u64,
    
    /// Source that triggered entry
    pub trigger_source: String,
}

impl ActivePosition {
    /// Check if position should exit based on current price and features
    pub fn should_exit(&self, current_features: &MintFeatures, sol_price_usd: f64) -> Option<ExitReason> {
        let elapsed = self.entry_time.elapsed().as_secs();
        
        // Calculate current price from features
        let current_price_sol = current_features.current_price;
        
        // Calculate PnL percentage
        let price_change_pct = ((current_price_sol - self.entry_price_sol) / self.entry_price_sol.max(0.0001)) * 100.0;
        
        debug!("Position check: {} | elapsed: {}s | price_change: {:.2}%", 
               &self.mint[..8], elapsed, price_change_pct);
        
        // Check profit targets (tiered exits)
        if price_change_pct >= self.profit_targets.2 {
            return Some(ExitReason::ProfitTarget {
                tier: 3,
                pnl_pct: price_change_pct,
                exit_percent: 100, // Exit all
            });
        }
        
        if price_change_pct >= self.profit_targets.1 {
            return Some(ExitReason::ProfitTarget {
                tier: 2,
                pnl_pct: price_change_pct,
                exit_percent: 60, // Exit 60%
            });
        }
        
        if price_change_pct >= self.profit_targets.0 {
            return Some(ExitReason::ProfitTarget {
                tier: 1,
                pnl_pct: price_change_pct,
                exit_percent: 30, // Exit 30%
            });
        }
        
        // Check stop loss
        if price_change_pct <= -self.stop_loss_pct {
            return Some(ExitReason::StopLoss {
                pnl_pct: price_change_pct,
                exit_percent: 100, // Exit all
            });
        }
        
        // Check time decay
        if elapsed >= self.max_hold_secs {
            return Some(ExitReason::TimeDecay {
                elapsed_secs: elapsed,
                pnl_pct: price_change_pct,
                exit_percent: 100, // Exit all
            });
        }
        
        // Check volume drop (potential dump signal)
        if current_features.vol_5s_sol < 0.5 && price_change_pct < 10.0 {
            // Volume dried up and price hasn't pumped much = likely dead
            if elapsed > 30 {
                return Some(ExitReason::VolumeDrop {
                    volume_5s: current_features.vol_5s_sol,
                    pnl_pct: price_change_pct,
                    exit_percent: 100,
                });
            }
        }
        
        None // Hold position
    }
    
    /// Get position value in USD at current price
    pub fn current_value_usd(&self, current_price_sol: f64, sol_price_usd: f64) -> f64 {
        self.tokens * current_price_sol * sol_price_usd
    }
    
    /// Get unrealized PnL in USD
    pub fn unrealized_pnl_usd(&self, current_price_sol: f64, sol_price_usd: f64) -> f64 {
        self.current_value_usd(current_price_sol, sol_price_usd) - self.size_usd
    }
}

/// Reason for exiting a position
#[derive(Debug, Clone)]
pub enum ExitReason {
    /// Profit target hit
    ProfitTarget {
        tier: u8,            // 1, 2, or 3
        pnl_pct: f64,        // Percentage gain
        exit_percent: u8,    // What % of position to exit
    },
    
    /// Stop loss triggered
    StopLoss {
        pnl_pct: f64,
        exit_percent: u8,
    },
    
    /// Max hold time exceeded
    TimeDecay {
        elapsed_secs: u64,
        pnl_pct: f64,
        exit_percent: u8,
    },
    
    /// Volume dried up
    VolumeDrop {
        volume_5s: f64,
        pnl_pct: f64,
        exit_percent: u8,
    },
    
    /// Emergency exit signal
    Emergency {
        reason: String,
        exit_percent: u8,
    },
}

impl ExitReason {
    pub fn to_string(&self) -> String {
        match self {
            ExitReason::ProfitTarget { tier, pnl_pct, exit_percent } => {
                format!("TP{} ({:+.1}%, exit {}%)", tier, pnl_pct, exit_percent)
            }
            ExitReason::StopLoss { pnl_pct, .. } => {
                format!("STOP_LOSS ({:+.1}%)", pnl_pct)
            }
            ExitReason::TimeDecay { elapsed_secs, pnl_pct, .. } => {
                format!("TIME_DECAY ({}s, {:+.1}%)", elapsed_secs, pnl_pct)
            }
            ExitReason::VolumeDrop { volume_5s, pnl_pct, .. } => {
                format!("VOL_DROP ({:.2}SOL/5s, {:+.1}%)", volume_5s, pnl_pct)
            }
            ExitReason::Emergency { reason, .. } => {
                format!("EMERGENCY ({})", reason)
            }
        }
    }
}

/// Position tracker manages all active positions
pub struct PositionTracker {
    positions: HashMap<String, ActivePosition>,
    max_positions: usize,
}

impl PositionTracker {
    pub fn new(max_positions: usize) -> Self {
        Self {
            positions: HashMap::new(),
            max_positions,
        }
    }
    
    /// Add a new position
    pub fn add_position(&mut self, position: ActivePosition) -> anyhow::Result<()> {
        if self.positions.len() >= self.max_positions {
            anyhow::bail!("Max positions reached: {}", self.max_positions);
        }
        
        info!("📊 Opening position: {} ({} SOL, conf={})",
              &position.mint[..8], position.size_sol, position.entry_confidence);
        
        self.positions.insert(position.mint.clone(), position);
        Ok(())
    }
    
    /// Remove a position (after exit)
    pub fn remove_position(&mut self, mint: &str) -> Option<ActivePosition> {
        self.positions.remove(mint)
    }
    
    /// Get position count
    pub fn count(&self) -> usize {
        self.positions.len()
    }
    
    /// Check if at max capacity
    pub fn is_full(&self) -> bool {
        self.positions.len() >= self.max_positions
    }
    
    /// Get all positions for monitoring
    pub fn get_all(&self) -> Vec<&ActivePosition> {
        self.positions.values().collect()
    }
    
    /// Check a specific position for exit signals
    pub fn check_position(&self, mint: &str, features: &MintFeatures, sol_price_usd: f64) -> Option<(ExitReason, &ActivePosition)> {
        if let Some(pos) = self.positions.get(mint) {
            if let Some(reason) = pos.should_exit(features, sol_price_usd) {
                return Some((reason, pos));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_position_tracker_limits() {
        let mut tracker = PositionTracker::new(3);
        assert_eq!(tracker.count(), 0);
        assert!(!tracker.is_full());
        
        // Add positions up to limit
        for i in 0..3 {
            let pos = ActivePosition {
                mint: format!("mint_{}", i),
                entry_time: Instant::now(),
                entry_timestamp: 0,
                size_sol: 0.1,
                size_usd: 20.0,
                entry_price_sol: 0.001,
                tokens: 100.0,
                entry_confidence: 80,
                profit_targets: (0.30, 0.60, 1.0),
                stop_loss_pct: 15.0,
                max_hold_secs: 120,
                trigger_source: "test".to_string(),
            };
            assert!(tracker.add_position(pos).is_ok());
        }
        
        assert_eq!(tracker.count(), 3);
        assert!(tracker.is_full());
        
        // Try to add beyond limit
        let pos = ActivePosition {
            mint: "mint_overflow".to_string(),
            entry_time: Instant::now(),
            entry_timestamp: 0,
            size_sol: 0.1,
            size_usd: 20.0,
            entry_price_sol: 0.001,
            tokens: 100.0,
            entry_confidence: 80,
            profit_targets: (0.30, 0.60, 1.0),
            stop_loss_pct: 15.0,
            max_hold_secs: 120,
            trigger_source: "test".to_string(),
        };
        assert!(tracker.add_position(pos).is_err());
    }
}
