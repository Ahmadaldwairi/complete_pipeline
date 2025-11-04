/// Real-time sliding window tracker for market metrics
/// 
/// Tracks rolling windows of trading activity to calculate:
/// - volume_sol_1s: SOL volume in last 1 second
/// - unique_buyers_1s: Unique buyers in last 1 second
/// - price_change_bps_2s: Price change over 2 seconds (basis points)
/// - alpha_wallet_hits_10s: Alpha wallet buys in last 10 seconds

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// Single trade event for window tracking
#[derive(Clone, Debug)]
struct TradeEvent {
    timestamp_ms: u64,
    amount_sol: f64,
    price: f64,
    trader: String,
    is_alpha: bool,
}

/// Window metrics for a single mint
#[derive(Clone, Debug)]
pub struct WindowMetrics {
    pub mint: String,
    pub volume_sol_1s: f64,
    pub unique_buyers_1s: u16,
    pub price_change_bps_2s: i16,
    pub alpha_wallet_hits_10s: u8,
    pub timestamp_ms: u64,
    /// Market cap in SOL (for velocity calculation)
    pub mc_sol: f64,
    /// Market cap 10 seconds ago (for velocity)
    pub mc_10s_ago: Option<f64>,
    /// Market cap 30 seconds ago (for velocity)
    pub mc_30s_ago: Option<f64>,
    /// MC velocity in SOL/min (calculated from 30s window)
    pub mc_velocity_sol_per_min: f64,
}

/// Per-mint sliding window tracker
struct MintWindow {
    /// Recent trades (kept for 10s max)
    events: VecDeque<TradeEvent>,
    /// Last price (for calculating change)
    last_price: f64,
    /// Last metrics sent (to avoid spam)
    last_metrics_sent_ms: u64,
    /// MC history for velocity calculation (timestamp_ms, mc_sol)
    mc_history: VecDeque<(u64, f64)>,
}

impl MintWindow {
    fn new() -> Self {
        Self {
            events: VecDeque::new(),
            last_price: 0.0,
            last_metrics_sent_ms: 0,
            mc_history: VecDeque::new(),
        }
    }
    
    /// Add a new trade event
    fn add_event(&mut self, timestamp_ms: u64, amount_sol: f64, price: f64, trader: String, is_alpha: bool) {
        self.events.push_back(TradeEvent {
            timestamp_ms,
            amount_sol,
            price,
            trader,
            is_alpha,
        });
        
        self.last_price = price;
        
        // Clean up events older than 10s
        let cutoff = timestamp_ms.saturating_sub(10_000);
        while let Some(event) = self.events.front() {
            if event.timestamp_ms < cutoff {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }
    
    /// Update market cap history for velocity calculation
    fn update_mc_history(&mut self, timestamp_ms: u64, mc_sol: f64) {
        self.mc_history.push_back((timestamp_ms, mc_sol));
        
        // Keep only last 60 seconds of MC history
        let cutoff = timestamp_ms.saturating_sub(60_000);
        while let Some((ts, _)) = self.mc_history.front() {
            if *ts < cutoff {
                self.mc_history.pop_front();
            } else {
                break;
            }
        }
    }
    
    /// Calculate metrics for current window
    fn calculate_metrics(&self, mint: &str, current_time_ms: u64, current_mc_sol: f64) -> WindowMetrics {
        let cutoff_1s = current_time_ms.saturating_sub(1_000);
        let cutoff_2s = current_time_ms.saturating_sub(2_000);
        let cutoff_10s = current_time_ms.saturating_sub(10_000);
        
        let mut volume_1s = 0.0;
        let mut buyers_1s = HashSet::new();
        let mut price_2s_ago = None;
        let mut alpha_hits_10s = 0u8;
        
        for event in &self.events {
            // 1s metrics
            if event.timestamp_ms >= cutoff_1s {
                volume_1s += event.amount_sol;
                buyers_1s.insert(event.trader.clone());
            }
            
            // 2s price tracking (find oldest price in 2s window)
            if event.timestamp_ms >= cutoff_2s && price_2s_ago.is_none() {
                price_2s_ago = Some(event.price);
            }
            
            // 10s alpha tracking
            if event.timestamp_ms >= cutoff_10s && event.is_alpha {
                alpha_hits_10s = alpha_hits_10s.saturating_add(1);
            }
        }
        
        // Calculate price change in basis points
        let price_change_bps = if let Some(old_price) = price_2s_ago {
            if old_price > 0.0 {
                let change_pct = ((self.last_price - old_price) / old_price) * 100.0;
                let bps = (change_pct * 100.0) as i16;
                bps.clamp(-9999, 9999)
            } else {
                0
            }
        } else {
            0
        };
        
        // Calculate MC velocity from history
        let mc_10s_ago = self.mc_history.iter()
            .rev()
            .find(|(ts, _)| *ts <= current_time_ms.saturating_sub(10_000))
            .map(|(_, mc)| *mc);
        
        let mc_30s_ago = self.mc_history.iter()
            .rev()
            .find(|(ts, _)| *ts <= current_time_ms.saturating_sub(30_000))
            .map(|(_, mc)| *mc);
        
        // Calculate velocity in SOL/min from 30s window
        let mc_velocity = if let Some(mc_old) = mc_30s_ago {
            if mc_old > 0.0 {
                let delta_mc = current_mc_sol - mc_old;
                let delta_seconds = 30.0;
                (delta_mc / delta_seconds) * 60.0 // Convert to SOL/min
            } else {
                0.0
            }
        } else {
            0.0
        };
        
        WindowMetrics {
            mint: mint.to_string(),
            volume_sol_1s: volume_1s,
            unique_buyers_1s: buyers_1s.len().min(u16::MAX as usize) as u16,
            price_change_bps_2s: price_change_bps,
            alpha_wallet_hits_10s: alpha_hits_10s.min(255),
            timestamp_ms: current_time_ms,
            mc_sol: current_mc_sol,
            mc_10s_ago,
            mc_30s_ago,
            mc_velocity_sol_per_min: mc_velocity,
        }
    }
    
    /// Check if metrics should be sent (throttle to avoid spam)
    fn should_send_metrics(&self, current_time_ms: u64, min_interval_ms: u64) -> bool {
        current_time_ms - self.last_metrics_sent_ms >= min_interval_ms
    }
    
    /// Mark metrics as sent
    fn mark_metrics_sent(&mut self, timestamp_ms: u64) {
        self.last_metrics_sent_ms = timestamp_ms;
    }
}

/// Global window tracker for all mints
pub struct WindowTracker {
    /// Per-mint windows
    windows: HashMap<String, MintWindow>,
    /// Minimum interval between metric sends (milliseconds)
    send_interval_ms: u64,
    /// Minimum activity threshold (trades in last 2s) to send metrics
    min_activity_threshold: usize,
}

impl WindowTracker {
    /// Create new window tracker
    /// 
    /// # Arguments
    /// * `send_interval_ms` - Minimum time between metric sends (default: 500ms)
    /// * `min_activity_threshold` - Minimum trades in 2s window to send (default: 3)
    pub fn new(send_interval_ms: u64, min_activity_threshold: usize) -> Self {
        Self {
            windows: HashMap::new(),
            send_interval_ms,
            min_activity_threshold,
        }
    }
    
    /// Create with default settings
    pub fn new_default() -> Self {
        Self::new(500, 3) // Send every 500ms, require 3+ trades
    }
    
    /// Add a trade to the tracker
    pub fn add_trade(
        &mut self,
        mint: &str,
        timestamp_ms: u64,
        amount_sol: f64,
        price: f64,
        trader: &str,
        is_alpha: bool,
    ) {
        let window = self.windows
            .entry(mint.to_string())
            .or_insert_with(MintWindow::new);
        
        window.add_event(timestamp_ms, amount_sol, price, trader.to_string(), is_alpha);
    }
    
    /// Update market cap for velocity tracking
    pub fn update_mc(
        &mut self,
        mint: &str,
        timestamp_ms: u64,
        mc_sol: f64,
    ) {
        let window = self.windows
            .entry(mint.to_string())
            .or_insert_with(MintWindow::new);
        
        window.update_mc_history(timestamp_ms, mc_sol);
    }
    
    /// Get metrics if they should be sent
    /// 
    /// Returns Some(metrics) if:
    /// 1. Enough time has passed since last send
    /// 2. There's sufficient activity (trades in last 2s)
    /// 
    /// # Arguments
    /// * `mint` - Token mint address
    /// * `current_mc_sol` - Current market cap in SOL
    pub fn get_metrics_if_ready(&mut self, mint: &str, current_mc_sol: f64) -> Option<WindowMetrics> {
        let window = self.windows.get_mut(mint)?;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;
        
        // Check if enough time has passed
        if !window.should_send_metrics(now, self.send_interval_ms) {
            return None;
        }
        
        // Check activity threshold (trades in last 2s)
        let cutoff_2s = now.saturating_sub(2_000);
        let recent_trades = window.events.iter()
            .filter(|e| e.timestamp_ms >= cutoff_2s)
            .count();
        
        if recent_trades < self.min_activity_threshold {
            return None;
        }
        
        // Calculate and return metrics
        let metrics = window.calculate_metrics(mint, now, current_mc_sol);
        window.mark_metrics_sent(now);
        
        debug!(
            "📊 WindowMetrics: {} | vol_1s: {:.2} SOL, buyers_1s: {}, Δprice_2s: {}bps, alpha_10s: {}, MC: {:.0} SOL, velocity: {:.0} SOL/min",
            &mint[..12.min(mint.len())],
            metrics.volume_sol_1s,
            metrics.unique_buyers_1s,
            metrics.price_change_bps_2s,
            metrics.alpha_wallet_hits_10s,
            metrics.mc_sol,
            metrics.mc_velocity_sol_per_min
        );
        
        Some(metrics)
    }
    
    /// Clean up old windows to prevent memory leaks
    /// Call periodically (e.g., every minute)
    pub fn cleanup_old_windows(&mut self, max_idle_sec: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        let cutoff_ms = now.saturating_sub(max_idle_sec) * 1000;
        
        self.windows.retain(|mint, window| {
            if let Some(last_event) = window.events.back() {
                if last_event.timestamp_ms < cutoff_ms {
                    debug!("🧹 Cleaned up idle window for {}", &mint[..12.min(mint.len())]);
                    return false;
                }
            }
            true
        });
    }
    
    /// Get current window count (for monitoring)
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_window_tracker_basic() {
        let mut tracker = WindowTracker::new_default();
        let mint = "test_mint_123";
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        // Add some trades
        tracker.add_trade(mint, now, 1.0, 0.0001, "trader1", false);
        tracker.add_trade(mint, now + 100, 2.0, 0.00011, "trader2", false);
        tracker.add_trade(mint, now + 200, 1.5, 0.00012, "trader3", true);
        
        // Should not send yet (not enough time passed)
        assert!(tracker.get_metrics_if_ready(mint, 100.0).is_none());
    }
    
    #[test]
    fn test_price_change_calculation() {
        let mut window = MintWindow::new();
        let now = 1000000;
        
        // Add trades with price increase
        window.add_event(now, 1.0, 0.001, "t1".to_string(), false);
        window.add_event(now + 1000, 1.0, 0.0011, "t2".to_string(), false);
        
        let metrics = window.calculate_metrics("test", now + 2000, 100.0);
        
        // Price went from 0.001 to 0.0011 = 10% increase = 1000bps
        assert_eq!(metrics.price_change_bps_2s, 1000);
    }
    
    #[test]
    fn test_unique_buyers_count() {
        let mut window = MintWindow::new();
        let now = 1000000;
        
        // Add trades from same and different buyers
        window.add_event(now, 1.0, 0.001, "buyer1".to_string(), false);
        window.add_event(now + 100, 1.0, 0.001, "buyer1".to_string(), false);
        window.add_event(now + 200, 1.0, 0.001, "buyer2".to_string(), false);
        window.add_event(now + 300, 1.0, 0.001, "buyer3".to_string(), false);
        
        let metrics = window.calculate_metrics("test", now + 500, 100.0);
        
        // Should count 3 unique buyers in 1s window
        assert_eq!(metrics.unique_buyers_1s, 3);
    }
}