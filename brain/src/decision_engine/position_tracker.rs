//! 📊 Position Tracker - Monitor active trades and trigger exits
//!
//! Tracks open positions, monitors price movements, and generates SELL decisions
//! when profit targets hit, stop losses trigger, or time decay occurs.

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use log::{info, warn, debug};
use crate::feature_cache::MintFeatures;
use crate::decision_engine::triggers::EntryTrigger;

/// Position state in 3-state confirmation system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionState {
    /// Transaction submitted but not yet confirmed
    Submitted,
    /// Transaction confirmed on-chain (soft confirmation from mempool or finalized)
    Confirmed,
    /// Transaction failed or timed out
    Failed,
}

/// Provisional position (SUBMITTED state, not yet confirmed)
#[derive(Debug, Clone)]
pub struct ProvisionalPosition {
    /// Token mint address (hex string)
    pub mint: String,
    
    /// Transaction signature
    pub signature: String,
    
    /// Submission timestamp
    pub submitted_at: Instant,
    
    /// Expected tokens to receive
    pub expected_tokens: u64,
    
    /// Expected SOL amount (lamports)
    pub expected_sol_lamports: u64,
    
    /// Expected slippage (basis points)
    pub expected_slip_bps: u16,
    
    /// Position side (0=BUY, 1=SELL)
    pub side: u8,
    
    /// Fast confirmation heuristic (low mempool competition)
    pub fast_confirm: bool,
    
    /// Current state
    pub state: PositionState,
}

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
    
    /// Entry path that triggered this position
    pub entry_path: EntryTrigger,
    
    /// Early score from 7-signal system (0.0-15.0)
    pub early_score: f64,
    
    /// Profit targets (tier1, tier2, tier3) in % gain
    pub profit_targets: (f64, f64, f64),
    
    /// Stop loss threshold in % loss
    pub stop_loss_pct: f64,
    
    /// Maximum hold time in seconds (path-specific)
    pub max_hold_secs: u64,
    
    /// Source that triggered entry
    pub trigger_source: String,
    
    /// SELL retry counter (incremented on each failed SELL)
    pub sell_retry_count: u8,
    
    /// MC at entry (for velocity tracking)
    pub entry_mc_sol: f64,
    
    /// MC 10s ago (for velocity-based exit)
    pub mc_10s_ago: Option<f64>,
    
    /// MC 20s ago (for velocity-based exit)
    pub mc_20s_ago: Option<f64>,
}

impl ActivePosition {
    /// Get path-specific profit target based on entry path
    pub fn get_profit_target_usd(&self) -> f64 {
        match self.entry_path {
            EntryTrigger::RankBased => {
                // Rank: $1-3 profit target
                if self.early_score >= 8.0 {
                    3.0  // High confidence = higher target
                } else {
                    1.0  // Standard target
                }
            },
            EntryTrigger::Momentum => {
                // Momentum: $5-20 profit target (velocity-based)
                if self.early_score >= 8.0 {
                    20.0  // Ultra high confidence
                } else if self.early_score >= 7.0 {
                    10.0  // High confidence
                } else {
                    5.0   // Standard
                }
            },
            EntryTrigger::CopyTrade => {
                // Copy-trade: $1-2 quick profit
                if self.early_score >= 8.0 {
                    2.0
                } else {
                    1.0
                }
            },
            EntryTrigger::LateOpportunity => {
                // Late: Not used for 1M+ MC hunting
                5.0
            }
        }
    }
    
    /// Update MC velocity tracking (call every 10s)
    pub fn update_mc_velocity(&mut self, current_mc_sol: f64) {
        // Shift history
        self.mc_20s_ago = self.mc_10s_ago;
        self.mc_10s_ago = Some(current_mc_sol);
    }
    
    /// Check if MC velocity is decelerating (trend exhaustion)
    pub fn is_mc_velocity_decelerating(&self, current_mc_sol: f64) -> bool {
        if let (Some(mc_10s), Some(mc_20s)) = (self.mc_10s_ago, self.mc_20s_ago) {
            if mc_20s > 0.0 && mc_10s > mc_20s {
                // Calculate velocity for each 10s window
                let velocity_recent = (current_mc_sol - mc_10s) / 10.0;  // SOL/sec
                let velocity_prev = (mc_10s - mc_20s) / 10.0;            // SOL/sec
                
                // Check if velocity dropped >50%
                if velocity_prev > 0.0 && velocity_recent < (velocity_prev * 0.5) {
                    info!("📉 MC velocity deceleration detected: {:.2} → {:.2} SOL/s ({:.1}% drop)",
                          velocity_prev, velocity_recent, 
                          ((velocity_prev - velocity_recent) / velocity_prev * 100.0));
                    return true;
                }
            }
        }
        false
    }
    
    /// Get path-specific stop loss percentage
    pub fn get_stop_loss_pct(&self) -> f64 {
        match self.entry_path {
            EntryTrigger::RankBased => -20.0,      // Higher volatility, wider stop
            EntryTrigger::Momentum => -15.0,       // Standard stop
            EntryTrigger::CopyTrade => -10.0,      // Tight stop for quick scalps
            EntryTrigger::LateOpportunity => -15.0,
        }
    }
    
    /// Check if position should exit based on current price and features
    pub fn should_exit(&self, current_features: &MintFeatures, sol_price_usd: f64) -> Option<ExitReason> {
        let elapsed = self.entry_time.elapsed().as_secs();
        
        // Calculate current price from features
        let current_price_sol = current_features.current_price;
        
        // Calculate PnL percentage
        let price_change_pct = ((current_price_sol - self.entry_price_sol) / self.entry_price_sol.max(0.0001)) * 100.0;
        
        // Calculate absolute dollar profit FIRST (priority exit condition)
        let current_value_usd = self.tokens * current_price_sol * sol_price_usd;
        let realized_profit = current_value_usd - self.size_usd;
        
        info!("📊 Position Check: {} | Path: {:?} | Score: {:.1} | ⏱️  {}s | 📈 {:.2}% | 💵 ${:.2} profit | 📦 {} mempool buys", 
               &self.mint[..8], self.entry_path, self.early_score, elapsed, 
               price_change_pct, realized_profit, current_features.mempool_pending_buys);
        
        // ✅ PATH-SPECIFIC PROFIT TARGET (replaces fixed $1 target)
        let target_usd = self.get_profit_target_usd();
        if realized_profit >= target_usd {
            info!("✅ EXIT TRIGGER: Path-specific profit target reached ${:.2} (target: ${:.2})", 
                  realized_profit, target_usd);
            return Some(ExitReason::ProfitTarget {
                tier: 1,
                pnl_pct: price_change_pct,
                exit_percent: 100,
            });
        }
        
        // ✅ MC VELOCITY-BASED EXIT (for Momentum path)
        if matches!(self.entry_path, EntryTrigger::Momentum) {
            if self.is_mc_velocity_decelerating(current_features.mc_sol) {
                // Only exit if we're in profit or minimal loss
                if realized_profit >= -5.0 {
                    info!("✅ EXIT TRIGGER: MC velocity deceleration (trend exhaustion)");
                    return Some(ExitReason::ProfitTarget {
                        tier: 1,
                        pnl_pct: price_change_pct,
                        exit_percent: 100,
                    });
                }
            }
        }
        
        // Check percentage-based profit targets (backup, for high-percentage gains)
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
        
        // ✅ PATH-SPECIFIC STOP LOSS
        let stop_loss_threshold = self.get_stop_loss_pct();
        if price_change_pct <= stop_loss_threshold {
            info!("❌ EXIT TRIGGER: Path-specific stop loss hit ({:.1}% threshold)", stop_loss_threshold);
            return Some(ExitReason::StopLoss {
                pnl_pct: price_change_pct,
                exit_percent: 100, // Exit all
            });
        }
        
        // ✅ PRIORITY EXIT: No mempool activity (no buying pressure)
        // User requirement: "Bot will only stay if mempool shows volume"
        if current_features.mempool_pending_buys == 0 && elapsed > 15 {
            info!("✅ EXIT TRIGGER: No mempool activity (0 pending buys after {}s)", elapsed);
            return Some(ExitReason::NoMempoolActivity {
                elapsed_secs: elapsed,
                pnl_pct: price_change_pct,
                exit_percent: 100,
            });
        } else if current_features.mempool_pending_buys > 0 {
            info!("🔥 HOLDING: Mempool shows {} pending buys - volume still coming in!", 
                  current_features.mempool_pending_buys);
        }
        
        // Check time decay (safety backstop only)
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
    
    /// No mempool activity (no buying pressure)
    NoMempoolActivity {
        elapsed_secs: u64,
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
            ExitReason::NoMempoolActivity { elapsed_secs, pnl_pct, .. } => {
                format!("NO_MEMPOOL_ACTIVITY ({}s, {:+.1}%)", elapsed_secs, pnl_pct)
            }
            ExitReason::Emergency { reason, .. } => {
                format!("EMERGENCY ({})", reason)
            }
        }
    }
}

/// Position tracker manages all active positions
pub struct PositionTracker {
    /// Confirmed active positions
    positions: HashMap<String, ActivePosition>,
    /// Provisional positions awaiting confirmation
    provisional_positions: HashMap<String, ProvisionalPosition>,
    max_positions: usize,
}

impl PositionTracker {
    pub fn new(max_positions: usize) -> Self {
        Self {
            positions: HashMap::new(),
            provisional_positions: HashMap::new(),
            max_positions,
        }
    }
    
    /// Add a provisional position (SUBMITTED state)
    pub fn add_provisional(&mut self, mint: String, signature: String, expected_tokens: u64, 
                          expected_sol_lamports: u64, expected_slip_bps: u16, side: u8, 
                          mempool_pending_buys: u32) {
        // Heuristic: Low mempool competition = fast confirmation likely
        let fast_confirm = mempool_pending_buys < 3;
        
        info!("📤 Adding provisional position: {} | sig: {} | exp tokens: {} | mempool buys: {} | fast_confirm: {}", 
              &mint[..8], &signature[..8], expected_tokens, mempool_pending_buys, fast_confirm);
        
        let provisional = ProvisionalPosition {
            mint: mint.clone(),
            signature,
            submitted_at: Instant::now(),
            expected_tokens,
            expected_sol_lamports,
            expected_slip_bps,
            side,
            fast_confirm,
            state: PositionState::Submitted,
        };
        
        self.provisional_positions.insert(mint, provisional);
    }
    
    /// Confirm a provisional position (move to active positions)
    pub fn confirm_provisional(&mut self, mint: &str, actual_tokens: f64, actual_sol_lamports: u64,
                               entry_confidence: u8, trigger_source: String, sol_price_usd: f64) -> anyhow::Result<()> {
        if let Some(provisional) = self.provisional_positions.remove(mint) {
            let elapsed_ms = provisional.submitted_at.elapsed().as_millis();
            info!("✅ Confirming provisional position: {} (took {}ms)", &mint[..8], elapsed_ms);
            
            // Create active position from provisional
            let size_sol = actual_sol_lamports as f64 / 1e9;
            let size_usd = size_sol * sol_price_usd;
            let entry_price_sol = if actual_tokens > 0.0 {
                size_sol / actual_tokens
            } else {
                0.0
            };
            
            let position = ActivePosition {
                mint: mint.to_string(),
                entry_time: Instant::now(),
                entry_timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                size_sol,
                size_usd,
                entry_price_sol,
                tokens: actual_tokens,
                entry_confidence,
                profit_targets: (15.0, 30.0, 50.0), // Default targets
                stop_loss_pct: 15.0,
                max_hold_secs: 120,
                trigger_source,
                sell_retry_count: 0,
                // New fields for 1M+ MC hunting
                entry_path: EntryTrigger::RankBased, // Default
                early_score: 0.0,  // Will be populated from provisional
                entry_mc_sol: 0.0, // TODO: Get from features
                mc_10s_ago: None,
                mc_20s_ago: None,
            };
            
            self.add_position(position)?;
            Ok(())
        } else {
            anyhow::bail!("No provisional position found for mint: {}", mint);
        }
    }
    
    /// Mark provisional position as failed and remove it
    pub fn fail_provisional(&mut self, mint: &str, reason: &str) {
        if let Some(provisional) = self.provisional_positions.remove(mint) {
            let elapsed_ms = provisional.submitted_at.elapsed().as_millis();
            warn!("❌ Provisional position failed: {} (after {}ms) - reason: {}", 
                  &mint[..8], elapsed_ms, reason);
        }
    }
    
    /// Get provisional position by mint
    pub fn get_provisional(&self, mint: &str) -> Option<&ProvisionalPosition> {
        self.provisional_positions.get(mint)
    }
    
    /// Get provisional position count
    pub fn provisional_count(&self) -> usize {
        self.provisional_positions.len()
    }
    
    /// Check for timed-out provisional positions (should be called periodically)
    /// Returns list of mints that timed out
    /// Uses different timeouts: fast_confirm positions timeout faster (600ms), normal positions timeout at 1200ms
    pub fn check_provisional_timeouts(&mut self, normal_timeout_ms: u64, fast_timeout_ms: u64) -> Vec<String> {
        let mut timed_out = Vec::new();
        
        self.provisional_positions.retain(|mint, provisional| {
            let elapsed_ms = provisional.submitted_at.elapsed().as_millis() as u64;
            let timeout = if provisional.fast_confirm { fast_timeout_ms } else { normal_timeout_ms };
            
            if elapsed_ms > timeout {
                warn!("⏱️  Provisional position timed out: {} ({}ms > {}ms, fast_confirm: {})", 
                      &mint[..8], elapsed_ms, timeout, provisional.fast_confirm);
                timed_out.push(mint.clone());
                false // Remove from map
            } else {
                true // Keep
            }
        });
        
        timed_out
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
    
    /// Get mutable reference to a position (for updating MC velocity)
    pub fn get_position_mut(&mut self, mint: &str) -> Option<&mut ActivePosition> {
        self.positions.get_mut(mint)
    }
    
    /// Extend hold duration for a position (momentum/volume signals)
    /// Returns true if position was found and updated
    pub fn extend_hold_duration(&mut self, mint: &str, additional_secs: u64) -> bool {
        if let Some(pos) = self.positions.get_mut(mint) {
            pos.max_hold_secs = pos.max_hold_secs.saturating_add(additional_secs);
            info!("⏱️  Extended hold duration for {} by {}s → {}s total", 
                  &mint[..8], additional_secs, pos.max_hold_secs);
            true
        } else {
            false
        }
    }
    
    /// Adjust profit targets for a position (increase thresholds for momentum)
    /// Returns true if position was found and updated
    pub fn adjust_profit_targets(&mut self, mint: &str, multiplier: f64) -> bool {
        if let Some(pos) = self.positions.get_mut(mint) {
            let old_targets = pos.profit_targets;
            pos.profit_targets = (
                old_targets.0 * multiplier,
                old_targets.1 * multiplier,
                old_targets.2 * multiplier,
            );
            info!("📊 Adjusted profit targets for {} by {:.2}x → ({:.1}%, {:.1}%, {:.1}%)", 
                  &mint[..8], multiplier, pos.profit_targets.0, pos.profit_targets.1, pos.profit_targets.2);
            true
        } else {
            false
        }
    }
    
    /// Trigger early exit for a position (alpha wallet sell or negative volume spike)
    /// Returns true if position exists (caller should initiate sell)
    pub fn trigger_early_exit(&mut self, mint: &str) -> bool {
        if let Some(pos) = self.positions.get_mut(mint) {
            // Set max_hold_secs to 1 to force immediate exit on next check
            pos.max_hold_secs = 1;
            warn!("⚠️  Triggered early exit for {} (max_hold → 1s)", &mint[..8]);
            true
        } else {
            false
        }
    }
    
    /// Increment SELL retry counter and check if position should be force-removed
    /// Returns true if position exceeded max retries (3) and should be removed
    pub fn increment_sell_retry(&mut self, mint: &str) -> bool {
        if let Some(pos) = self.positions.get_mut(mint) {
            pos.sell_retry_count += 1;
            pos.sell_retry_count >= 3
        } else {
            false
        }
    }
    
    /// Check a specific position for exit signals
    /// Only checks CONFIRMED positions - provisional positions are ignored
    pub fn check_position(&self, mint: &str, features: &MintFeatures, sol_price_usd: f64) -> Option<(ExitReason, &ActivePosition)> {
        // Don't check provisional positions for exits
        if self.provisional_positions.contains_key(mint) {
            debug!("⏳ Skipping exit check for provisional position: {}", &mint[..8]);
            return None;
        }
        
        // Only check confirmed active positions
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
                sell_retry_count: 0,
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
            sell_retry_count: 0,
        };
        assert!(tracker.add_position(pos).is_err());
    }
}
