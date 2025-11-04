/// Real-time momentum tracker for confirmed transactions
/// 
/// Tracks rolling windows of buy/sell activity to detect momentum patterns
/// and volume spikes as they happen.

use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use tracing::debug;

use crate::types::TradeSide;

/// Single transaction event for rolling window tracking
#[derive(Clone)]
struct TxEvent {
    timestamp_ms: u64,
    side: TradeSide,
    amount_sol: f64,
    trader: String,
}

/// Rolling window stats for a single mint
struct MintWindow {
    /// Recent transactions (kept for 2000ms for 2s window tracking)
    events: VecDeque<TxEvent>,
    /// Last average volume (for spike detection)
    last_avg_volume_sol: f64,
    /// Last momentum signal sent timestamp (to avoid spam)
    last_momentum_signal_ms: u64,
    /// Last volume spike signal sent timestamp
    last_spike_signal_ms: u64,
}

impl MintWindow {
    fn new() -> Self {
        Self {
            events: VecDeque::new(),
            last_avg_volume_sol: 0.0,
            last_momentum_signal_ms: 0,
            last_spike_signal_ms: 0,
        }
    }
    
    /// Add a new transaction event
    fn add_event(&mut self, timestamp_ms: u64, side: TradeSide, amount_sol: f64, trader: String) {
        self.events.push_back(TxEvent {
            timestamp_ms,
            side,
            amount_sol,
            trader,
        });
    }
    
    /// Remove events older than cutoff_ms
    fn cleanup_old_events(&mut self, cutoff_ms: u64) {
        while let Some(event) = self.events.front() {
            if event.timestamp_ms < cutoff_ms {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
    
    /// Calculate buys in last N milliseconds
    fn buys_in_window(&self, window_start_ms: u64) -> (u16, f64, usize) {
        let mut buys = 0u16;
        let mut volume_sol = 0.0;
        let mut unique_buyers = std::collections::HashSet::new();
        
        for event in &self.events {
            if event.timestamp_ms >= window_start_ms {
                if matches!(event.side, TradeSide::Buy) {
                    buys += 1;
                    volume_sol += event.amount_sol;
                    unique_buyers.insert(&event.trader);
                }
            }
        }
        
        (buys, volume_sol, unique_buyers.len())
    }
    
    /// Calculate total volume in window
    fn total_volume_in_window(&self, window_start_ms: u64) -> (f64, u16) {
        let mut total_sol = 0.0;
        let mut tx_count = 0u16;
        
        for event in &self.events {
            if event.timestamp_ms >= window_start_ms {
                total_sol += event.amount_sol;
                tx_count += 1;
            }
        }
        
        (total_sol, tx_count)
    }
}

/// Main momentum tracker
pub struct MomentumTracker {
    /// Per-mint windows
    mints: HashMap<String, MintWindow>,
    /// Momentum detection threshold (buys in 500ms)
    momentum_threshold: u16,
    /// Volume spike multiplier (5x = 5.0)
    spike_multiplier: f32,
    /// Minimum time between signals (ms)
    signal_cooldown_ms: u64,
}

impl MomentumTracker {
    pub fn new(momentum_threshold: u16, spike_multiplier: f32, signal_cooldown_ms: u64) -> Self {
        Self {
            mints: HashMap::new(),
            momentum_threshold,
            spike_multiplier,
            signal_cooldown_ms,
        }
    }
    
    /// Record a new transaction
    pub fn record_trade(
        &mut self,
        mint: &str,
        side: TradeSide,
        amount_sol: f64,
        trader: &str,
    ) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let window = self.mints.entry(mint.to_string())
            .or_insert_with(MintWindow::new);
        
        window.add_event(now_ms, side, amount_sol, trader.to_string());
        
        // Cleanup events older than 2 seconds
        let cutoff_ms = now_ms.saturating_sub(2000);
        window.cleanup_old_events(cutoff_ms);
    }
    
    /// Check for momentum signal (returns Some if momentum detected and cooldown passed)
    pub fn check_momentum(&mut self, mint: &str) -> Option<MomentumSignal> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let window = self.mints.get_mut(mint)?;
        
        // Check cooldown
        if now_ms - window.last_momentum_signal_ms < self.signal_cooldown_ms {
            return None;
        }
        
        // Calculate buys in last 500ms
        let window_start_500ms = now_ms.saturating_sub(500);
        let (buys, volume_sol, unique_buyers) = window.buys_in_window(window_start_500ms);
        
        if buys >= self.momentum_threshold {
            window.last_momentum_signal_ms = now_ms;
            
            // Calculate confidence based on unique buyers and volume
            let buyer_diversity = (unique_buyers as f32 / buys.max(1) as f32 * 100.0) as u8;
            let volume_score = (volume_sol.min(10.0) / 10.0 * 50.0) as u8;
            let confidence = (buyer_diversity.min(50) + volume_score).min(100);
            
            debug!("📊 Momentum detected: {} | buys: {}, vol: {:.2}, buyers: {}, conf: {}",
                   &mint[..8], buys, volume_sol, unique_buyers, confidence);
            
            return Some(MomentumSignal {
                mint: mint.to_string(),
                buys_in_last_500ms: buys,
                volume_sol,
                unique_buyers: unique_buyers as u16,
                confidence,
            });
        }
        
        None
    }
    
    /// Check for volume spike (returns Some if spike detected and cooldown passed)
    pub fn check_volume_spike(&mut self, mint: &str) -> Option<VolumeSpikeSignal> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let window = self.mints.get_mut(mint)?;
        
        // Check cooldown
        if now_ms - window.last_spike_signal_ms < self.signal_cooldown_ms {
            return None;
        }
        
        // Calculate volume in last 2 seconds
        let window_start_2s = now_ms.saturating_sub(2000);
        let (current_volume, tx_count) = window.total_volume_in_window(window_start_2s);
        
        // Compare to last average
        if current_volume > window.last_avg_volume_sol * self.spike_multiplier as f64 
            && window.last_avg_volume_sol > 0.0 {
            
            window.last_spike_signal_ms = now_ms;
            
            // Calculate confidence based on spike magnitude
            let spike_ratio = current_volume / window.last_avg_volume_sol.max(0.01);
            let confidence = ((spike_ratio / 10.0).min(1.0) * 100.0) as u8;
            
            debug!("📈 Volume spike detected: {} | {:.2} SOL ({}x previous), {} txs, conf: {}",
                   &mint[..8], current_volume, spike_ratio, tx_count, confidence);
            
            return Some(VolumeSpikeSignal {
                mint: mint.to_string(),
                total_sol: current_volume as f32,
                tx_count,
                time_window_ms: 2000,
                confidence,
            });
        }
        
        // Update running average
        window.last_avg_volume_sol = (window.last_avg_volume_sol * 0.8) + (current_volume * 0.2);
        
        None
    }
    
    /// Periodic cleanup of inactive mints
    pub fn cleanup_inactive_mints(&mut self, max_age_ms: u64) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        self.mints.retain(|_mint, window| {
            // Keep if has recent events
            window.events.back()
                .map(|e| now_ms - e.timestamp_ms < max_age_ms)
                .unwrap_or(false)
        });
    }
}

/// Momentum detection signal
#[derive(Debug, Clone)]
pub struct MomentumSignal {
    pub mint: String,
    pub buys_in_last_500ms: u16,
    pub volume_sol: f64,
    pub unique_buyers: u16,
    pub confidence: u8,
}

/// Volume spike detection signal
#[derive(Debug, Clone)]
pub struct VolumeSpikeSignal {
    pub mint: String,
    pub total_sol: f32,
    pub tx_count: u16,
    pub time_window_ms: u16,
    pub confidence: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_momentum_detection() {
        let mut tracker = MomentumTracker::new(3, 5.0, 5000);
        
        // Record 4 buys (should trigger momentum)
        tracker.record_trade("mint1", TradeSide::Buy, 1.0, "wallet1");
        tracker.record_trade("mint1", TradeSide::Buy, 2.0, "wallet2");
        tracker.record_trade("mint1", TradeSide::Buy, 1.5, "wallet3");
        tracker.record_trade("mint1", TradeSide::Buy, 0.5, "wallet4");
        
        let signal = tracker.check_momentum("mint1");
        assert!(signal.is_some());
        
        let sig = signal.unwrap();
        assert_eq!(sig.buys_in_last_500ms, 4);
        assert_eq!(sig.unique_buyers, 4);
    }
}
