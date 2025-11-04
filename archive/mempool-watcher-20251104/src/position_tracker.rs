//! 📊 Position Tracker - Tracks active positions from WatchSigEnhanced
//!
//! Stores position metadata and monitors real-time P&L for exit signal generation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, debug, warn};
use crate::watch_sig_enhanced::WatchSigEnhanced;
use crate::position_update::PositionUpdate;
use crate::exit_advice::ExitAdvice;

/// Tracked position with entry data and P&L monitoring
#[derive(Debug, Clone)]
pub struct TrackedPosition {
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub side: u8,  // 0=BUY, 1=SELL
    pub entry_time: std::time::Instant,
    pub entry_timestamp: u64,
    
    // Entry data
    pub entry_price_lamports: u64,
    pub size_sol: f64,
    pub slippage_bps: u16,
    pub fee_bps: u16,
    pub profit_target_usd: f64,
    pub stop_loss_usd: f64,
    
    // Current state
    pub last_update: std::time::Instant,
    pub current_price_lamports: u64,
    pub last_pnl_usd: f64,
    pub last_pnl_percent: f64,
    pub update_count: u32,
}

impl TrackedPosition {
    /// Create from WatchSigEnhanced
    pub fn from_watch_sig(watch: &WatchSigEnhanced) -> Self {
        let now = std::time::Instant::now();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            mint: watch.mint,
            trade_id: watch.trade_id,
            side: watch.side,
            entry_time: now,
            entry_timestamp: timestamp,
            entry_price_lamports: watch.entry_price_lamports,
            size_sol: (watch.size_sol_scaled as f64) / 1000.0,
            slippage_bps: watch.slippage_bps,
            fee_bps: watch.fee_bps,
            profit_target_usd: (watch.profit_target_cents as f64) / 100.0,
            stop_loss_usd: (watch.stop_loss_cents as f64) / 100.0,
            last_update: now,
            current_price_lamports: watch.entry_price_lamports,
            last_pnl_usd: 0.0,
            last_pnl_percent: 0.0,
            update_count: 0,
        }
    }
    
    /// Update with new market price and calculate P&L
    pub fn update_price(&mut self, new_price_lamports: u64, sol_price_usd: f64) -> (f64, f64) {
        self.current_price_lamports = new_price_lamports;
        self.last_update = std::time::Instant::now();
        self.update_count += 1;
        
        // Calculate P&L
        let (pnl_usd, pnl_percent) = self.calculate_pnl(sol_price_usd);
        self.last_pnl_usd = pnl_usd;
        self.last_pnl_percent = pnl_percent;
        
        (pnl_usd, pnl_percent)
    }
    
    /// Calculate realized P&L in USD and percentage
    fn calculate_pnl(&self, sol_price_usd: f64) -> (f64, f64) {
        if self.entry_price_lamports == 0 {
            return (0.0, 0.0);
        }
        
        // Price change ratio
        let price_ratio = self.current_price_lamports as f64 / self.entry_price_lamports as f64;
        
        // For BUY positions: gain when price goes up
        // For SELL positions: already exited, no tracking needed (this is for BUY only)
        let price_change_pct = (price_ratio - 1.0) * 100.0;
        
        // Calculate position values
        let entry_value_usd = self.size_sol * sol_price_usd;
        let current_value_usd = entry_value_usd * price_ratio;
        
        // Realized P&L (includes fees)
        let fees_usd = entry_value_usd * (self.fee_bps as f64 / 10000.0) * 2.0; // Entry + Exit
        let pnl_usd = current_value_usd - entry_value_usd - fees_usd;
        
        (pnl_usd, price_change_pct)
    }
    
    /// Check if profit target hit
    pub fn is_profit_target_hit(&self) -> bool {
        self.last_pnl_usd >= self.profit_target_usd
    }
    
    /// Check if stop loss hit
    pub fn is_stop_loss_hit(&self) -> bool {
        self.last_pnl_usd <= self.stop_loss_usd
    }
    
    /// Check if position should send update (5s elapsed OR price moved >5%)
    pub fn should_send_update(&self, last_sent: std::time::Instant, last_pnl_percent: f64) -> bool {
        let time_elapsed = self.last_update.duration_since(last_sent).as_secs();
        let pnl_change = (self.last_pnl_percent - last_pnl_percent).abs();
        
        time_elapsed >= 5 || pnl_change >= 5.0
    }
    
    /// Get mint as base58 string
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
}

/// Position tracker that monitors active positions
pub struct PositionTracker {
    positions: Arc<RwLock<HashMap<String, TrackedPosition>>>, // Key: mint base58
    last_update_sent: Arc<RwLock<HashMap<String, (std::time::Instant, f64)>>>, // Key: mint, Value: (time, last_pnl%)
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: Arc::new(RwLock::new(HashMap::new())),
            last_update_sent: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Add position from WatchSigEnhanced (only for BUY side)
    pub async fn add_position(&self, watch: WatchSigEnhanced) {
        // Only track BUY positions
        if watch.side != WatchSigEnhanced::SIDE_BUY {
            debug!("Skipping SELL position tracking: {}", bs58::encode(&watch.mint[..8]).into_string());
            return;
        }
        
        let position = TrackedPosition::from_watch_sig(&watch);
        let mint_str = position.mint_str();
        
        info!("📊 Tracking new position: {} | size={:.3} SOL | target=${:.2} | stop=${:.2}",
            &mint_str[..8], position.size_sol, position.profit_target_usd, position.stop_loss_usd);
        
        let mut positions = self.positions.write().await;
        positions.insert(mint_str.clone(), position);
        
        // Initialize last update tracking
        let mut last_updates = self.last_update_sent.write().await;
        last_updates.insert(mint_str, (std::time::Instant::now(), 0.0));
    }
    
    /// Update position price and return PositionUpdate if significant change
    /// Returns (PositionUpdate, Option<ExitAdvice>) - ExitAdvice if profit/loss threshold hit
    pub async fn update_position_price(
        &self,
        mint: &[u8; 32],
        new_price_lamports: u64,
        sol_price_usd: f64,
        mempool_pending_buys: u16,
        mempool_pending_sells: u16,
    ) -> Option<(PositionUpdate, Option<crate::exit_advice::ExitAdvice>)> {
        let mint_str = bs58::encode(mint).into_string();
        
        let mut positions = self.positions.write().await;
        let position = positions.get_mut(&mint_str)?;
        
        // Update price and calculate P&L
        let (pnl_usd, pnl_percent) = position.update_price(new_price_lamports, sol_price_usd);
        
        // Check if we should send update
        let last_updates = self.last_update_sent.read().await;
        let (last_sent_time, last_sent_pnl) = last_updates.get(&mint_str).copied().unwrap_or((
            std::time::Instant::now() - std::time::Duration::from_secs(10),
            0.0
        ));
        drop(last_updates);
        
        let should_send = position.should_send_update(last_sent_time, last_sent_pnl);
        
        if !should_send {
            return None;
        }
        
        // Update last sent time
        let mut last_updates = self.last_update_sent.write().await;
        last_updates.insert(mint_str.clone(), (std::time::Instant::now(), pnl_percent));
        drop(last_updates);
        
        // Calculate additional metrics
        let age_secs = position.entry_time.elapsed().as_secs();
        let price_velocity = if age_secs > 0 {
            pnl_percent / age_secs as f64 // % change per second
        } else {
            0.0
        };
        
        // Check exit conditions
        let profit_target_hit = position.is_profit_target_hit();
        let stop_loss_hit = position.is_stop_loss_hit();
        let no_mempool_activity = mempool_pending_buys == 0 && age_secs > 15;
        
        debug!("📈 Position update: {} | P&L: ${:.2} ({:.1}%) | target_hit: {} | stop_hit: {} | no_activity: {}",
            &mint_str[..8], pnl_usd, pnl_percent, profit_target_hit, stop_loss_hit, no_mempool_activity);
        
        // Create PositionUpdate message
        let current_value_sol = position.size_sol * (new_price_lamports as f64 / position.entry_price_lamports as f64);
        
        let update = PositionUpdate::new(
            *mint,
            position.trade_id,
            position.entry_price_lamports,
            new_price_lamports,
            position.size_sol as f32,
            current_value_sol as f32,
            pnl_usd as f32,
            pnl_percent as f32,
            mempool_pending_buys,
            mempool_pending_sells,
            price_velocity as f32,
            profit_target_hit,
            stop_loss_hit,
            no_mempool_activity,
        );
        
        // Create ExitAdvice if profit target or stop loss hit
        let hold_time_ms = position.entry_time.elapsed().as_millis() as u32;
        
        let exit_advice = if profit_target_hit {
            let reason = ExitAdvice::REASON_TARGET_HIT;
            let confidence = if pnl_percent >= 0.50 { 95 } else if pnl_percent >= 0.30 { 85 } else { 75 };
            
            Some(ExitAdvice::new(
                position.trade_id,
                *mint,
                reason,
                confidence,
                pnl_usd,
                position.entry_price_lamports,
                new_price_lamports,
                hold_time_ms,
            ))
        } else if stop_loss_hit {
            let reason = ExitAdvice::REASON_STOP_LOSS;
            let confidence = 100; // Maximum confidence on stop loss
            
            Some(ExitAdvice::new(
                position.trade_id,
                *mint,
                reason,
                confidence,
                pnl_usd,
                position.entry_price_lamports,
                new_price_lamports,
                hold_time_ms,
            ))
        } else {
            None
        };
        
        Some((update, exit_advice))
    }
    
    /// Remove position (when exit confirmed)
    pub async fn remove_position(&self, mint: &[u8; 32]) {
        let mint_str = bs58::encode(mint).into_string();
        
        let mut positions = self.positions.write().await;
        if let Some(pos) = positions.remove(&mint_str) {
            info!("📊 Stopped tracking position: {} | final P&L: ${:.2} ({:.1}%)",
                &mint_str[..8], pos.last_pnl_usd, pos.last_pnl_percent);
        }
        
        let mut last_updates = self.last_update_sent.write().await;
        last_updates.remove(&mint_str);
    }
    
    /// Check if a position is tracked by mint string
    pub async fn has_position(&self, mint_str: &str) -> bool {
        let positions = self.positions.read().await;
        positions.contains_key(mint_str)
    }
    
    /// Remove position by mint string (for manual exits)
    pub async fn remove_position_by_str(&self, mint_str: &str) {
        let mut positions = self.positions.write().await;
        if let Some(pos) = positions.remove(mint_str) {
            info!("📊 Stopped tracking position: {} | final P&L: ${:.2} ({:.1}%)",
                &mint_str[..8], pos.last_pnl_usd, pos.last_pnl_percent);
        }
        
        let mut last_updates = self.last_update_sent.write().await;
        last_updates.remove(mint_str);
    }
    
    /// Get position count
    pub async fn count(&self) -> usize {
        self.positions.read().await.len()
    }
    
    /// Check all positions for stale data (>60s since last price update)
    pub async fn check_stale_positions(&self) {
        let positions = self.positions.read().await;
        let now = std::time::Instant::now();
        
        for (mint, pos) in positions.iter() {
            let age_secs = now.duration_since(pos.last_update).as_secs();
            if age_secs > 60 {
                warn!("⚠️  Stale position: {} | no price update for {}s", &mint[..8], age_secs);
            }
        }
    }
    
    /// Get all position updates for periodic broadcasting
    /// Returns Vec of PositionUpdate messages for all tracked positions
    pub async fn get_all_updates(&self, sol_price_usd: f64) -> Vec<PositionUpdate> {
        let positions = self.positions.read().await;
        let mut updates = Vec::new();
        
        for pos in positions.values() {
            // Calculate current value in SOL based on price ratio
            let price_ratio = pos.current_price_lamports as f64 / pos.entry_price_lamports as f64;
            let current_value_sol = pos.size_sol * price_ratio;
            
            // Use the PositionUpdate::new() constructor which handles timestamp and padding
            let update = PositionUpdate::new(
                pos.mint,
                pos.trade_id,
                pos.entry_price_lamports,
                pos.current_price_lamports,
                pos.size_sol as f32,
                current_value_sol as f32,
                pos.last_pnl_usd as f32,
                pos.last_pnl_percent as f32,
                0, // mempool_pending_buys - updated on next trade
                0, // mempool_pending_sells - updated on next trade
                0.0, // price_velocity - calculated on next trade
                pos.is_profit_target_hit(),
                pos.is_stop_loss_hit(),
                false, // no_activity flag - needs mempool context
            );
            
            updates.push(update);
        }
        
        updates
    }
    
    /// Check if a SELL transaction is a manual exit (not from our executor)
    /// Returns ManualExitNotification if manual exit detected
    pub async fn check_manual_exit(
        &self,
        mint: &[u8; 32],
        exit_signature: &[u8; 64],
        exit_price_lamports: u64,
        sol_price_usd: f64,
    ) -> Option<crate::manual_exit::ManualExitNotification> {
        let positions = self.positions.read().await;
        
        // Find position by mint
        let mint_str = bs58::encode(mint).into_string();
        let pos = positions.get(&mint_str)?;
        
        // Calculate P&L
        let price_ratio = exit_price_lamports as f64 / pos.entry_price_lamports as f64;
        let exit_value_sol = pos.size_sol * price_ratio;
        
        // Fees: 0.3% pump.fun fee on entry + exit
        let fees_sol = pos.size_sol * (pos.fee_bps as f64 / 10000.0) * 2.0;
        let net_profit_sol = exit_value_sol - pos.size_sol - fees_sol;
        let realized_pnl_usd = net_profit_sol * sol_price_usd;
        let pnl_percent = (price_ratio - 1.0) * 100.0;
        
        // Calculate hold time
        let hold_time_secs = pos.entry_time.elapsed().as_secs() as u32;
        
        info!("🚨 MANUAL EXIT DETECTED: {} | P&L: ${:.2} ({:.1}%) | hold: {}s",
              &mint_str[..8], realized_pnl_usd, pnl_percent, hold_time_secs);
        
        Some(crate::manual_exit::ManualExitNotification::new(
            *mint,
            pos.trade_id,
            *exit_signature,
            pos.entry_price_lamports,
            exit_price_lamports,
            pos.size_sol as f32,
            realized_pnl_usd as f32,
            pnl_percent as f32,
            hold_time_secs,
        ))
    }
}

impl Default for PositionTracker {
    fn default() -> Self {
        Self::new()
    }
}
