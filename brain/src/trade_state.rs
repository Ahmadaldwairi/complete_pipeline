//! 🎯 Trade State Machine - Prevents Duplicate Trades
//!
//! Tracks the state of each token (mint) to ensure:
//! - Only ONE BUY decision per mint at a time
//! - Only send SELL when we actually HOLD the position
//! - Proper state transitions: Idle → BuyPending → Holding → SellPending → Closed
//!
//! This is THE solution to the duplicate trade bug.

use std::collections::HashMap;
use std::time::{Instant, Duration};
use log::{info, warn, debug};

/// Trade state for a specific mint
#[derive(Debug, Clone, PartialEq)]
pub enum TradeState {
    /// No active trade for this mint
    Idle,
    
    /// BUY decision sent to Executor, awaiting confirmation
    BuyPending {
        trade_id: String,
        sent_at: Instant,
    },
    
    /// Position confirmed on-chain, holding tokens
    Holding {
        trade_id: String,
        confirmed_at: Instant,
        entry_price: f64,
    },
    
    /// SELL decision sent to Executor, awaiting confirmation
    SellPending {
        trade_id: String,
        sent_at: Instant,
    },
    
    /// Trade completed (sold or failed)
    Closed {
        trade_id: String,
        closed_at: Instant,
        reason: CloseReason,
    },
}

/// Reason for trade closure
#[derive(Debug, Clone, PartialEq)]
pub enum CloseReason {
    Confirmed,  // Successfully sold
    Failed,     // Transaction failed
    Timeout,    // No confirmation received in time
}

impl TradeState {
    /// Check if state allows sending BUY decision
    pub fn can_buy(&self) -> bool {
        matches!(self, TradeState::Idle)
    }
    
    /// Check if state allows sending SELL decision
    pub fn can_sell(&self) -> bool {
        matches!(self, TradeState::Holding { .. })
    }
    
    /// Check if state is pending (awaiting confirmation)
    pub fn is_pending(&self) -> bool {
        matches!(self, TradeState::BuyPending { .. } | TradeState::SellPending { .. })
    }
    
    /// Get trade_id if available
    pub fn trade_id(&self) -> Option<&str> {
        match self {
            TradeState::Idle => None,
            TradeState::BuyPending { trade_id, .. } => Some(trade_id),
            TradeState::Holding { trade_id, .. } => Some(trade_id),
            TradeState::SellPending { trade_id, .. } => Some(trade_id),
            TradeState::Closed { trade_id, .. } => Some(trade_id),
        }
    }
    
    /// Get age of current state
    pub fn age(&self) -> Duration {
        match self {
            TradeState::Idle => Duration::ZERO,
            TradeState::BuyPending { sent_at, .. } => sent_at.elapsed(),
            TradeState::Holding { confirmed_at, .. } => confirmed_at.elapsed(),
            TradeState::SellPending { sent_at, .. } => sent_at.elapsed(),
            TradeState::Closed { closed_at, .. } => closed_at.elapsed(),
        }
    }
}

/// State tracker for all active trades
pub struct TradeStateTracker {
    states: HashMap<String, TradeState>,  // mint -> state
    buy_timeout: Duration,
    sell_timeout: Duration,
}

impl TradeStateTracker {
    /// Create new tracker with timeout configurations
    pub fn new(buy_timeout_secs: u64, sell_timeout_secs: u64) -> Self {
        Self {
            states: HashMap::new(),
            buy_timeout: Duration::from_secs(buy_timeout_secs),
            sell_timeout: Duration::from_secs(sell_timeout_secs),
        }
    }
    
    /// Get current state for mint (default: Idle)
    pub fn get_state(&self, mint: &str) -> TradeState {
        self.states.get(mint).cloned().unwrap_or(TradeState::Idle)
    }
    
    /// Check if BUY is allowed for this mint
    pub fn can_buy(&self, mint: &str) -> bool {
        self.get_state(mint).can_buy()
    }
    
    /// Check if SELL is allowed for this mint
    pub fn can_sell(&self, mint: &str) -> bool {
        self.get_state(mint).can_sell()
    }
    
    /// Transition: Idle → BuyPending (when BUY decision sent)
    pub fn mark_buy_pending(&mut self, mint: String, trade_id: String) {
        let old_state = self.get_state(&mint);
        
        if !old_state.can_buy() {
            warn!("⚠️  Cannot transition to BuyPending from {:?} for mint {}", 
                  old_state, &mint[..12]);
            return;
        }
        
        let new_state = TradeState::BuyPending {
            trade_id: trade_id.clone(),
            sent_at: Instant::now(),
        };
        
        info!("🟡 {} → BuyPending (trade_id: {})", &mint[..12], &trade_id[..8]);
        self.states.insert(mint, new_state);
    }
    
    /// Transition: BuyPending → Holding (when TxConfirmed SUCCESS received)
    pub fn mark_holding(&mut self, mint: String, trade_id: String, entry_price: f64) {
        let old_state = self.get_state(&mint);
        
        match old_state {
            TradeState::BuyPending { trade_id: expected_id, .. } => {
                if expected_id != trade_id {
                    warn!("⚠️  trade_id mismatch: expected {}, got {}", 
                          &expected_id[..8], &trade_id[..8]);
                    return;
                }
                
                let new_state = TradeState::Holding {
                    trade_id: trade_id.clone(),
                    confirmed_at: Instant::now(),
                    entry_price,
                };
                
                info!("🟢 {} → Holding (trade_id: {}, price: {:.10})", 
                      &mint[..12], &trade_id[..8], entry_price);
                self.states.insert(mint, new_state);
            }
            _ => {
                warn!("⚠️  Cannot transition to Holding from {:?} for mint {}", 
                      old_state, &mint[..12]);
            }
        }
    }
    
    /// Transition: Holding → SellPending (when SELL decision sent)
    pub fn mark_sell_pending(&mut self, mint: String, trade_id: String) {
        let old_state = self.get_state(&mint);
        
        match old_state {
            TradeState::Holding { .. } => {
                let new_state = TradeState::SellPending {
                    trade_id: trade_id.clone(),
                    sent_at: Instant::now(),
                };
                
                info!("🟠 {} → SellPending (trade_id: {})", &mint[..12], &trade_id[..8]);
                self.states.insert(mint, new_state);
            }
            _ => {
                warn!("⚠️  Cannot transition to SellPending from {:?} for mint {}", 
                      old_state, &mint[..12]);
            }
        }
    }
    
    /// Transition: SellPending → Closed (when TxConfirmed received or timeout)
    pub fn mark_closed(&mut self, mint: String, trade_id: String, reason: CloseReason) {
        let new_state = TradeState::Closed {
            trade_id: trade_id.clone(),
            closed_at: Instant::now(),
            reason: reason.clone(),
        };
        
        info!("⚫ {} → Closed (trade_id: {}, reason: {:?})", 
              &mint[..12], &trade_id[..8], reason);
        self.states.insert(mint, new_state);
    }
    
    /// Transition: BuyPending → Closed (when BUY fails)
    pub fn mark_buy_failed(&mut self, mint: String, trade_id: String) {
        let old_state = self.get_state(&mint);
        
        if matches!(old_state, TradeState::BuyPending { .. }) {
            self.mark_closed(mint, trade_id, CloseReason::Failed);
        } else {
            warn!("⚠️  Cannot mark BUY failed from {:?} for mint {}", 
                  old_state, &mint[..12]);
        }
    }
    
    /// Check for timed-out pending states and mark as failed
    pub fn check_timeouts(&mut self) {
        let mut timeouts = Vec::new();
        
        for (mint, state) in &self.states {
            match state {
                TradeState::BuyPending { trade_id, sent_at } => {
                    if sent_at.elapsed() > self.buy_timeout {
                        warn!("⏰ BUY timeout for {} ({}s)", &mint[..12], sent_at.elapsed().as_secs());
                        timeouts.push((mint.clone(), trade_id.clone(), CloseReason::Timeout));
                    }
                }
                TradeState::SellPending { trade_id, sent_at } => {
                    if sent_at.elapsed() > self.sell_timeout {
                        warn!("⏰ SELL timeout for {} ({}s)", &mint[..12], sent_at.elapsed().as_secs());
                        timeouts.push((mint.clone(), trade_id.clone(), CloseReason::Timeout));
                    }
                }
                _ => {}
            }
        }
        
        // Apply timeouts
        for (mint, trade_id, reason) in timeouts {
            self.mark_closed(mint, trade_id, reason);
        }
    }
    
    /// Clean up old Closed states (>5 minutes old)
    pub fn cleanup_old_states(&mut self) {
        let cutoff = Duration::from_secs(300); // 5 minutes
        
        self.states.retain(|mint, state| {
            if let TradeState::Closed { closed_at, .. } = state {
                if closed_at.elapsed() > cutoff {
                    debug!("🧹 Cleaning up old closed state for {}", &mint[..12]);
                    return false;
                }
            }
            true
        });
    }
    
    /// Find stale pending states that need reconciliation
    /// Returns list of (mint, trade_id, age_duration) that should be checked on-chain
    pub fn get_stale_pending_states(&self, stale_threshold_secs: u64) -> Vec<(String, String, Duration)> {
        let stale_threshold = Duration::from_secs(stale_threshold_secs);
        let mut stale = Vec::new();
        
        for (mint, state) in &self.states {
            match state {
                TradeState::BuyPending { trade_id, sent_at } => {
                    let age = sent_at.elapsed();
                    if age > stale_threshold {
                        warn!("🔍 STALE BuyPending detected: {} (age={}s, trade_id={})", 
                              &mint[..12], age.as_secs(), &trade_id[..8]);
                        stale.push((mint.clone(), trade_id.clone(), age));
                    }
                }
                TradeState::SellPending { trade_id, sent_at } => {
                    let age = sent_at.elapsed();
                    if age > stale_threshold {
                        warn!("🔍 STALE SellPending detected: {} (age={}s, trade_id={})", 
                              &mint[..12], age.as_secs(), &trade_id[..8]);
                        stale.push((mint.clone(), trade_id.clone(), age));
                    }
                }
                _ => {}
            }
        }
        
        stale
    }
    
    /// Reconcile a state after blockchain query
    /// Call this after verifying transaction status on-chain
    pub fn reconcile_state(&mut self, mint: String, trade_id: String, confirmed: bool) {
        let current_state = self.get_state(&mint);
        
        match current_state {
            TradeState::BuyPending { .. } => {
                if confirmed {
                    info!("✅ RECONCILED: BUY for {} was confirmed on-chain (missed notification)", &mint[..12]);
                    self.mark_holding(mint, trade_id, 0.0);  // Price unknown during reconciliation
                } else {
                    warn!("❌ RECONCILED: BUY for {} failed/not found on-chain", &mint[..12]);
                    self.mark_buy_failed(mint, trade_id);
                }
            }
            TradeState::SellPending { .. } => {
                if confirmed {
                    info!("✅ RECONCILED: SELL for {} was confirmed on-chain (missed notification)", &mint[..12]);
                    self.mark_closed(mint, trade_id, CloseReason::Confirmed);
                } else {
                    warn!("❌ RECONCILED: SELL for {} failed/not found on-chain", &mint[..12]);
                    self.mark_closed(mint, trade_id, CloseReason::Failed);
                }
            }
            _ => {
                debug!("ℹ️  Reconciliation skipped for {} - state is {:?}", &mint[..12], current_state);
            }
        }
    }
    
    /// Get count of states by type
    pub fn get_stats(&self) -> (usize, usize, usize, usize) {
        let mut buy_pending = 0;
        let mut holding = 0;
        let mut sell_pending = 0;
        let mut closed = 0;
        
        for state in self.states.values() {
            match state {
                TradeState::Idle => {}
                TradeState::BuyPending { .. } => buy_pending += 1,
                TradeState::Holding { .. } => holding += 1,
                TradeState::SellPending { .. } => sell_pending += 1,
                TradeState::Closed { .. } => closed += 1,
            }
        }
        
        (buy_pending, holding, sell_pending, closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_state_transitions() {
        let mut tracker = TradeStateTracker::new(10, 15);
        let mint = "mint123".to_string();
        let trade_id = "trade456".to_string();
        
        // Initial state should be Idle
        assert!(tracker.can_buy(&mint));
        assert!(!tracker.can_sell(&mint));
        
        // Mark BuyPending
        tracker.mark_buy_pending(mint.clone(), trade_id.clone());
        assert!(!tracker.can_buy(&mint));
        assert!(!tracker.can_sell(&mint));
        
        // Mark Holding
        tracker.mark_holding(mint.clone(), trade_id.clone(), 0.001);
        assert!(!tracker.can_buy(&mint));
        assert!(tracker.can_sell(&mint));
        
        // Mark SellPending
        tracker.mark_sell_pending(mint.clone(), trade_id.clone());
        assert!(!tracker.can_buy(&mint));
        assert!(!tracker.can_sell(&mint));
        
        // Mark Closed
        tracker.mark_closed(mint.clone(), trade_id.clone(), CloseReason::Confirmed);
        assert!(!tracker.can_buy(&mint)); // Closed state blocks new BUY
    }
}
