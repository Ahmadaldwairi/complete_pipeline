//! 📡 Confirmation Broadcaster with Δ-Window Context
//!
//! Handles transaction confirmation detection and broadcasts enhanced context to Brain + Executor.
//! Implements micro-buffer (150-250ms) to capture market momentum after our transaction.

use anyhow::Result;
use log::{debug, error, info, warn};
use std::net::UdpSocket;
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};

use crate::watch_sig_enhanced::WatchSigEnhanced;
use crate::tx_confirmed_context::TxConfirmedContext;
use crate::exit_advice::ExitAdvice;
use crate::decoder::TransactionDecoder;
use crate::alpha_wallet_manager::AlphaWalletManager;
use crate::position_tracker::PositionTracker;
use crate::udp_publisher::UdpPublisher;

/// Tracks market activity in a time window
#[derive(Debug, Clone, Default)]
struct MarketWindow {
    pub uniq_buyers: std::collections::HashSet<String>,
    pub vol_buy_sol: f64,
    pub vol_sell_sol: f64,
    pub tx_count: u16,
    pub alpha_hits: u8,
    pub price_samples: Vec<f64>,
}

/// Confirmation broadcaster with Δ-window analysis
pub struct ConfirmationBroadcaster {
    brain_socket: UdpSocket,
    executor_socket: UdpSocket,
    brain_addr: String,
    executor_addr: String,
    decoder: std::sync::Arc<TransactionDecoder>,
    alpha_manager: std::sync::Arc<tokio::sync::Mutex<AlphaWalletManager>>,
    position_tracker: Arc<PositionTracker>,
    udp_publisher: Arc<UdpPublisher>,
}

impl ConfirmationBroadcaster {
    pub fn new(
        bind_address: &str,
        brain_port: u16,
        executor_confirmed_port: u16,
        decoder: std::sync::Arc<TransactionDecoder>,
        alpha_manager: std::sync::Arc<tokio::sync::Mutex<AlphaWalletManager>>,
        position_tracker: Arc<PositionTracker>,
        udp_publisher: Arc<UdpPublisher>,
    ) -> Result<Self> {
        // Create separate sockets for Brain and Executor
        let brain_socket = UdpSocket::bind(format!("{}:0", bind_address))?;
        brain_socket.set_nonblocking(true)?;

        let executor_socket = UdpSocket::bind(format!("{}:0", bind_address))?;
        executor_socket.set_nonblocking(true)?;

        let brain_addr = format!("{}:{}", bind_address, brain_port);
        let executor_addr = format!("{}:{}", bind_address, executor_confirmed_port);

        Ok(Self {
            brain_socket,
            executor_socket,
            brain_addr,
            executor_addr,
            decoder,
            alpha_manager,
            position_tracker,
            udp_publisher,
        })
    }

    /// Broadcast confirmation with Δ-window context
    /// 
    /// This is the main entry point that:
    /// 1. Waits for micro-buffer window (150-250ms)
    /// 2. Collects market data during that window
    /// 3. Calculates profit estimation
    /// 4. Broadcasts TxConfirmedContext to BOTH Brain and Executor
    pub async fn broadcast_with_context(
        &self,
        watch_sig: WatchSigEnhanced,
        slot: u64,
        status: u8,
        current_price_lamports: u64,
        mint_str: &str,
    ) -> Result<()> {
        let start = Instant::now();
        
        // Random micro-buffer between 150-250ms to avoid collisions
        let buffer_ms = {
            use rand::Rng;
            rand::thread_rng().gen_range(150..=250)
        };
        
        info!(
            "⏱️  Starting Δ-window capture for {} (buffer: {}ms)",
            &watch_sig.signature_str()[..12],
            buffer_ms
        );
        
        // Wait for the buffer period while collecting data
        let window_data = self.collect_window_data(
            mint_str,
            slot,
            buffer_ms,
        ).await?;
        
        let actual_trail_ms = start.elapsed().as_millis() as u16;
        
        // Calculate realized P&L
        let realized_pnl_usd = self.calculate_pnl(
            &watch_sig,
            current_price_lamports,
        );
        
        // Calculate price change in basis points
        let price_change_bps = if watch_sig.entry_price_lamports > 0 {
            let price_diff = current_price_lamports as i64 - watch_sig.entry_price_lamports as i64;
            ((price_diff as f64 / watch_sig.entry_price_lamports as f64) * 10000.0) as i16
        } else {
            0
        };
        
        // Create TxConfirmedContext with all Δ-window data
        let tx_confirmed_ctx = TxConfirmedContext::new(
            watch_sig.signature,
            watch_sig.mint,
            watch_sig.trade_id,
            watch_sig.side,
            status,
            slot,
            // Δ-window fields
            actual_trail_ms,
            window_data.same_slot_after,
            window_data.next_slot_count,
            window_data.uniq_buyers.len() as u16,
            window_data.vol_buy_sol,
            window_data.vol_sell_sol,
            price_change_bps,
            window_data.alpha_hits,
            // Entry data (from WatchSig)
            watch_sig.entry_price_lamports,
            watch_sig.size_sol(),
            watch_sig.slippage_bps,
            watch_sig.fee_bps,
            realized_pnl_usd,
        );
        
        let bytes = tx_confirmed_ctx.to_bytes();
        
        // ===== SINGLE BROADCAST to BOTH endpoints =====
        
        // Send to Executor
        match self.executor_socket.send_to(&bytes, &self.executor_addr) {
            Ok(_) => {
                debug!("📤 Sent TxConfirmedContext to Executor");
            }
            Err(e) => {
                error!("❌ Failed to send to Executor: {}", e);
            }
        }
        
        // Send to Brain
        match self.brain_socket.send_to(&bytes, &self.brain_addr) {
            Ok(_) => {
                debug!("📤 Sent TxConfirmedContext to Brain");
            }
            Err(e) => {
                error!("❌ Failed to send to Brain: {}", e);
            }
        }
        
        // ===== UPDATE POSITION TRACKER with current price =====
        // If this is a trade for a tracked position, update the price
        let sol_price_usd = 150.0; // TODO: Get from oracle/cache
        let mempool_buys = window_data.uniq_buyers.len() as u16;
        let mempool_sells = if window_data.vol_sell_sol > 0.0 { 1 } else { 0 }; // Simplification
        
        if let Some((position_update, exit_advice_opt)) = self.position_tracker.update_position_price(
            &watch_sig.mint,
            current_price_lamports,
            sol_price_usd,
            mempool_buys,
            mempool_sells,
        ).await {
            // Position update triggered - send to Brain
            // Copy packed fields to avoid unaligned references
            let pnl_usd = position_update.realized_pnl_usd;
            let pnl_pct = position_update.pnl_percent;
            
            if let Err(e) = self.udp_publisher.send_position_update(&position_update) {
                error!("❌ Failed to send PositionUpdate: {}", e);
            } else {
                debug!("📊 Sent PositionUpdate for tracked position: P&L ${:.2} ({:.1}%)",
                       pnl_usd, pnl_pct);
            }
            
            // If we have ExitAdvice, send it to Brain
            if let Some(exit_advice) = exit_advice_opt {
                if let Err(e) = self.udp_publisher.send_exit_advice(&exit_advice) {
                    error!("❌ Failed to send ExitAdvice: {}", e);
                } else {
                    info!("🚨 Sent ExitAdvice to Brain: {} | P&L ${:.2} | confidence: {}",
                          exit_advice.reason_str(), exit_advice.realized_pnl_usd(), exit_advice.confidence);
                }
            }
        }
        
        // Log summary
        info!(
            "✅ Broadcast complete: {} {} | Δ: {}ms | buyers: {} | vol: {:.2}/{:.2} SOL | price: {:+.2}% | pnl: ${:.2} | alpha: {}",
            tx_confirmed_ctx.side_str(),
            tx_confirmed_ctx.status_str(),
            actual_trail_ms,
            window_data.uniq_buyers.len(),
            window_data.vol_buy_sol,
            window_data.vol_sell_sol,
            price_change_bps as f64 / 100.0,
            realized_pnl_usd,
            window_data.alpha_hits,
        );
        
        // ===== PROFIT TARGET CHECK (BUY positions only) =====
        if watch_sig.side == WatchSigEnhanced::SIDE_BUY {
            // Check if profit target hit
            if realized_pnl_usd >= watch_sig.profit_target_usd() && watch_sig.profit_target_usd() > 0.0 {
                info!(
                    "🎯 PROFIT TARGET HIT! {} | target: ${:.2} | realized: ${:.2}",
                    &watch_sig.signature_str()[..12],
                    watch_sig.profit_target_usd(),
                    realized_pnl_usd
                );
                
                // Calculate hold time
                let hold_time_ms = start.elapsed().as_millis() as u32;
                
                // Send ExitAdvice to Brain
                let exit_advice = ExitAdvice::new(
                    watch_sig.trade_id,
                    watch_sig.mint,
                    ExitAdvice::REASON_TARGET_HIT,
                    95, // High confidence for profit target
                    realized_pnl_usd,
                    watch_sig.entry_price_lamports,
                    current_price_lamports,
                    hold_time_ms,
                );
                
                let exit_bytes = exit_advice.to_bytes();
                
                match self.brain_socket.send_to(&exit_bytes, &self.brain_addr) {
                    Ok(_) => {
                        info!("📤 Sent ExitAdvice to Brain (target_hit)");
                    }
                    Err(e) => {
                        error!("❌ Failed to send ExitAdvice to Brain: {}", e);
                    }
                }
            }
            // Check if stop-loss triggered
            else if watch_sig.stop_loss_usd() < 0.0 && realized_pnl_usd <= watch_sig.stop_loss_usd() {
                warn!(
                    "🛑 STOP-LOSS TRIGGERED! {} | stop: ${:.2} | realized: ${:.2}",
                    &watch_sig.signature_str()[..12],
                    watch_sig.stop_loss_usd(),
                    realized_pnl_usd
                );
                
                // Calculate hold time
                let hold_time_ms = start.elapsed().as_millis() as u32;
                
                // Send ExitAdvice to Brain
                let exit_advice = ExitAdvice::new(
                    watch_sig.trade_id,
                    watch_sig.mint,
                    ExitAdvice::REASON_STOP_LOSS,
                    90, // High confidence for stop-loss
                    realized_pnl_usd,
                    watch_sig.entry_price_lamports,
                    current_price_lamports,
                    hold_time_ms,
                );
                
                let exit_bytes = exit_advice.to_bytes();
                
                match self.brain_socket.send_to(&exit_bytes, &self.brain_addr) {
                    Ok(_) => {
                        warn!("📤 Sent ExitAdvice to Brain (stop_loss)");
                    }
                    Err(e) => {
                        error!("❌ Failed to send ExitAdvice to Brain: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Collect market data during the Δ-window
    /// 
    /// NOTE: This is a simplified implementation that sleeps for the buffer period.
    /// In production, this would integrate with Yellowstone gRPC to:
    /// 1. Subscribe to transactions for the specific mint
    /// 2. Capture all transactions in same slot after ours
    /// 3. Capture transactions in next slot(s) within time window
    /// 4. Decode transactions to extract buyer wallets, volumes, prices
    /// 5. Check buyer wallets against alpha wallet list
    /// 
    /// For now, we return empty WindowData as a placeholder.
    async fn collect_window_data(
        &self,
        _mint: &str,            // Will be used for Yellowstone subscription
        _our_slot: u64,         // Will be used to filter same-slot transactions
        buffer_ms: u64,
    ) -> Result<WindowData> {
        let start = Instant::now();
        let data = WindowData::default();
        
        // Sleep for the buffer period
        sleep(Duration::from_millis(buffer_ms)).await;
        
        // TODO: Real implementation would:
        // 1. Subscribe to Yellowstone transaction stream for this mint
        // 2. Collect transactions during buffer period
        // 3. Parse swap instructions to get buyer addresses, volumes, prices
        // 4. Check against alpha wallet database
        // 5. Track same-slot vs next-slot transactions
        
        debug!(
            "📊 Δ-window collection complete: {}ms (placeholder - no real data collected yet)",
            start.elapsed().as_millis()
        );
        
        Ok(data)
    }

    /// Calculate realized P&L based on entry and current price
    fn calculate_pnl(
        &self,
        watch_sig: &WatchSigEnhanced,
        current_price_lamports: u64,
    ) -> f64 {
        if watch_sig.entry_price_lamports == 0 {
            return 0.0;
        }
        
        let size_sol = watch_sig.size_sol();
        
        // Calculate position size in tokens
        // tokens = SOL / price_per_token
        let tokens = (size_sol * 1_000_000_000.0) / watch_sig.entry_price_lamports as f64;
        
        // Calculate value at current price
        let current_value_lamports = tokens * current_price_lamports as f64;
        let entry_value_lamports = size_sol * 1_000_000_000.0;
        
        // P&L in lamports
        let pnl_lamports = current_value_lamports - entry_value_lamports;
        
        // Convert to USD (assuming 1 SOL = $150 for now - should use real price feed)
        const SOL_USD: f64 = 150.0;
        let pnl_sol = pnl_lamports / 1_000_000_000.0;
        let pnl_usd = pnl_sol * SOL_USD;
        
        // Subtract fees
        let fee_usd = size_sol * SOL_USD * (watch_sig.fee_bps as f64 / 10000.0);
        
        pnl_usd - fee_usd
    }
}

/// Data collected during Δ-window
#[derive(Debug, Default)]
struct WindowData {
    pub uniq_buyers: std::collections::HashSet<String>,
    pub vol_buy_sol: f64,
    pub vol_sell_sol: f64,
    pub same_slot_after: u16,
    pub next_slot_count: u16,
    pub alpha_hits: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pnl_calculation() {
        // Test basic P&L calculation
        // Entry: 0.1 SOL at 1M lamports per token = 100 tokens
        // Exit: 100 tokens at 1.5M lamports per token = 0.15 SOL
        // Profit: 0.05 SOL = $7.50 (at $150/SOL) - fees
        
        let watch_sig = WatchSigEnhanced::new(
            [0u8; 64],
            [0u8; 32],
            [0u8; 16],
            0,
            1_000_000,  // 1M lamports per token
            0.1,        // 0.1 SOL position
            150,        // 1.5% slippage
            30,         // 0.3% fee
            1.0,        // $1 profit target
            -0.5,       // -$0.50 stop loss
        );
        
        let decoder = std::sync::Arc::new(TransactionDecoder::new(10.0)); // 10 SOL whale threshold
        let alpha_manager = std::sync::Arc::new(tokio::sync::Mutex::new(
            AlphaWalletManager::new("test.db".to_string())
        ));
        
        // Create mock position tracker and udp publisher for test
        let position_tracker = Arc::new(PositionTracker::new());
        let udp_publisher = Arc::new(UdpPublisher::new("127.0.0.1", 45115, 45131).unwrap());
        
        let broadcaster = ConfirmationBroadcaster::new(
            "127.0.0.1",
            45115,
            45110,
            decoder,
            alpha_manager,
            position_tracker,
            udp_publisher,
        ).unwrap();
        
        let pnl = broadcaster.calculate_pnl(&watch_sig, 1_500_000);
        
        // Should be approximately $7.50 - $0.045 (0.3% fee on 0.1 SOL * $150) = ~$7.455
        assert!(pnl > 7.0 && pnl < 8.0, "P&L should be around $7.50, got {}", pnl);
    }
}
