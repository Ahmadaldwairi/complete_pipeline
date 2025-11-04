//! 📻 Advice Bus UDP Receiver
//!
//! Listens for advice messages from WalletTracker and LaunchTracker on port 45100.
//! Processes: ExtendHold, WidenExit, LateOpportunity, CopyTrade, SolPriceUpdate

use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use log::{info, warn, error, debug};
use anyhow::{Result, Context};
use crate::udp_bus::messages::AdviceMessage;

/// Statistics for received messages
#[derive(Debug, Clone, Default)]
pub struct ReceiverStats {
    pub total_received: u64,
    pub extend_hold: u64,
    pub widen_exit: u64,
    pub late_opportunity: u64,
    pub copy_trade: u64,
    pub sol_price_update: u64,
    pub parse_errors: u64,
}

/// UDP receiver for Advice Bus messages
pub struct AdviceBusReceiver {
    socket: Arc<UdpSocket>,
    stats: Arc<ReceiverStats>,
    running: Arc<AtomicBool>,
    total_received: Arc<AtomicU64>,
    extend_hold_count: Arc<AtomicU64>,
    widen_exit_count: Arc<AtomicU64>,
    late_opportunity_count: Arc<AtomicU64>,
    copy_trade_count: Arc<AtomicU64>,
    sol_price_update_count: Arc<AtomicU64>,
    parse_error_count: Arc<AtomicU64>,
}

impl AdviceBusReceiver {
    /// Create new Advice Bus receiver
    /// 
    /// Binds to port 45100 to receive messages from WalletTracker and LaunchTracker.
    pub async fn new() -> Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:45100")
            .await
            .context("Failed to bind UDP socket for Advice Bus receiver on port 45100")?;
        
        info!("📻 Advice Bus receiver bound to 127.0.0.1:45100");
        
        Ok(Self {
            socket: Arc::new(socket),
            stats: Arc::new(ReceiverStats::default()),
            running: Arc::new(AtomicBool::new(false)),
            total_received: Arc::new(AtomicU64::new(0)),
            extend_hold_count: Arc::new(AtomicU64::new(0)),
            widen_exit_count: Arc::new(AtomicU64::new(0)),
            late_opportunity_count: Arc::new(AtomicU64::new(0)),
            copy_trade_count: Arc::new(AtomicU64::new(0)),
            sol_price_update_count: Arc::new(AtomicU64::new(0)),
            parse_error_count: Arc::new(AtomicU64::new(0)),
        })
    }
    
    /// Start receiving messages
    /// 
    /// Returns a channel receiver that will receive parsed AdviceMessage instances.
    /// Messages are received in a background task and forwarded to the channel.
    pub async fn start(&self) -> mpsc::Receiver<AdviceMessage> {
        let (tx, rx) = mpsc::channel(1000); // Buffer up to 1000 messages
        
        self.running.store(true, Ordering::Relaxed);
        
        let socket = self.socket.clone();
        let running = self.running.clone();
        let total_received = self.total_received.clone();
        let extend_hold_count = self.extend_hold_count.clone();
        let widen_exit_count = self.widen_exit_count.clone();
        let late_opportunity_count = self.late_opportunity_count.clone();
        let copy_trade_count = self.copy_trade_count.clone();
        let sol_price_update_count = self.sol_price_update_count.clone();
        let parse_error_count = self.parse_error_count.clone();
        
        tokio::spawn(async move {
            let mut buf = [0u8; 1024]; // Large enough for any advice message
            
            info!("🎧 Started listening for Advice Bus messages...");
            
            while running.load(Ordering::Relaxed) {
                match socket.recv_from(&mut buf).await {
                    Ok((len, addr)) => {
                        total_received.fetch_add(1, Ordering::Relaxed);
                        
                        debug!("📨 Received {} bytes from {}", len, addr);
                        
                        // Parse message
                        match AdviceMessage::from_bytes(&buf[..len]) {
                            Some(msg) => {
                                // Update message type counters
                                match &msg {
                                    AdviceMessage::ExtendHold(_) => {
                                        extend_hold_count.fetch_add(1, Ordering::Relaxed);
                                        debug!("⏰ ExtendHold advice received");
                                    }
                                    AdviceMessage::WidenExit(_) => {
                                        widen_exit_count.fetch_add(1, Ordering::Relaxed);
                                        debug!("📊 WidenExit advice received");
                                    }
                                    AdviceMessage::LateOpportunity(advice) => {
                                        late_opportunity_count.fetch_add(1, Ordering::Relaxed);
                                        // Copy packed struct fields to avoid unaligned access
                                        let age = advice.age_seconds;
                                        let vol = advice.vol_60s_sol;
                                        let buyers = advice.buyers_60s;
                                        let score = advice.follow_through_score;
                                        info!(
                                            "🕐 LateOpportunity: mint={}..., age={}s, vol={:.1} SOL, buyers={}, score={}",
                                            hex::encode(&advice.mint[..4]),
                                            age, vol, buyers, score
                                        );
                                    }
                                    AdviceMessage::CopyTrade(advice) => {
                                        copy_trade_count.fetch_add(1, Ordering::Relaxed);
                                        // Copy packed struct fields to avoid unaligned access
                                        let side = advice.side;
                                        let size = advice.size_sol;
                                        let tier = advice.wallet_tier;
                                        let conf = advice.wallet_confidence;
                                        info!(
                                            "🎭 CopyTrade: wallet={}..., mint={}..., side={}, size={:.2} SOL, tier={}, conf={}",
                                            hex::encode(&advice.wallet[..4]),
                                            hex::encode(&advice.mint[..4]),
                                            side, size, tier, conf
                                        );
                                    }
                                    AdviceMessage::SolPriceUpdate(update) => {
                                        sol_price_update_count.fetch_add(1, Ordering::Relaxed);
                                        // Copy packed struct fields to avoid unaligned access
                                        let price = update.price_usd;
                                        let source = update.source;
                                        debug!("💵 SOL price update: ${:.2} from source {}", price, source);
                                    }
                                    AdviceMessage::MomentumOpportunity(advice) => {
                                        let vol = advice.vol_5s_sol();
                                        let buyers = advice.buyers_2s;
                                        let score = advice.score;
                                        info!(
                                            "⚡ MomentumOpportunity: mint={}..., vol_5s={:.2} SOL, buyers_2s={}, score={}",
                                            hex::encode(&advice.mint[..4]),
                                            vol, buyers, score
                                        );
                                    }
                                    AdviceMessage::RankOpportunity(advice) => {
                                        let rank = advice.rank;
                                        let score = advice.score;
                                        info!(
                                            "🏆 RankOpportunity: mint={}..., rank={}, score={}",
                                            hex::encode(&advice.mint[..4]),
                                            rank, score
                                        );
                                    }
                                    AdviceMessage::MempoolHeat(heat) => {
                                        let tx_rate = heat.tx_rate as f64 / 100.0;
                                        let whale_activity = heat.whale_activity as f64 / 100.0;
                                        let bot_density = heat.bot_density as f64 / 10000.0;
                                        debug!(
                                            "🌡️  MempoolHeat: score={}, tx_rate={:.2}/s, whale={:.2} SOL, bot={:.1}%",
                                            heat.heat_score, tx_rate, whale_activity, bot_density * 100.0
                                        );
                                    }
                                    AdviceMessage::TradeSubmitted(submitted) => {
                                        let sig_str = bs58::encode(&submitted.signature[..]).into_string();
                                        let mint_str = bs58::encode(&submitted.mint).into_string();
                                        // Copy values to avoid unaligned reference in packed struct
                                        let exp_tokens = submitted.expected_tokens;
                                        let exp_sol = submitted.expected_sol_lamports;
                                        info!(
                                            "📤 TradeSubmitted: {} {} - sig: {} - expected tokens: {}, sol: {} lamports",
                                            if submitted.side == 0 { "BUY" } else { "SELL" },
                                            mint_str, sig_str, exp_tokens, exp_sol
                                        );
                                    }
                                    AdviceMessage::TradeConfirmed(confirmed) => {
                                        let sig_str = bs58::encode(&confirmed.signature[..]).into_string();
                                        let mint_str = bs58::encode(&confirmed.mint).into_string();
                                        // Copy values to avoid unaligned reference in packed struct
                                        let act_tokens = confirmed.actual_tokens;
                                        let act_sol = confirmed.actual_sol_lamports;
                                        let fees = confirmed.total_fees_lamports;
                                        let fast = confirmed.fast_confirm;
                                        info!(
                                            "✅ TradeConfirmed: {} {} - sig: {} - tokens: {}, sol: {} lamports, fees: {} lamports, fast: {}",
                                            if confirmed.side == 0 { "BUY" } else { "SELL" },
                                            mint_str, sig_str, act_tokens, act_sol, fees, fast == 1
                                        );
                                    }
                                    AdviceMessage::TradeFailed(failed) => {
                                        let mint_str = bs58::encode(&failed.mint).into_string();
                                        let reason = String::from_utf8_lossy(&failed.reason_str)
                                            .trim_end_matches('\0')
                                            .to_string();
                                        let sig_str = if failed.has_signature == 1 {
                                            bs58::encode(&failed.signature[..]).into_string()
                                        } else {
                                            "N/A".to_string()
                                        };
                                        info!(
                                            "❌ TradeFailed: {} {} - sig: {} - reason: {}",
                                            if failed.side == 0 { "BUY" } else { "SELL" },
                                            mint_str, sig_str, reason
                                        );
                                    }
                                    AdviceMessage::MomentumDetected(momentum) => {
                                        let mint_str = bs58::encode(&momentum.mint).into_string();
                                        let buys = momentum.buys_in_last_500ms;
                                        let volume = momentum.volume_sol;
                                        let buyers = momentum.unique_buyers;
                                        let conf = momentum.confidence;
                                        info!(
                                            "📊 MomentumDetected: {} | buys: {}, vol: {:.2} SOL, buyers: {}, conf: {}",
                                            &mint_str[..8], buys, volume, buyers, conf
                                        );
                                    }
                                    AdviceMessage::VolumeSpike(spike) => {
                                        let mint_str = bs58::encode(&spike.mint).into_string();
                                        let total = spike.total_sol;
                                        let window = spike.time_window_ms;
                                        let count = spike.tx_count;
                                        let conf = spike.confidence;
                                        info!(
                                            "📈 VolumeSpike: {} | {:.2} SOL in {}ms, {} txs, conf: {}",
                                            &mint_str[..8], total, window, count, conf
                                        );
                                    }
                                    AdviceMessage::WalletActivity(wallet) => {
                                        let mint_str = bs58::encode(&wallet.mint).into_string();
                                        let wallet_str = bs58::encode(&wallet.wallet).into_string();
                                        let action = wallet.action;
                                        let size = wallet.size_sol;
                                        let tier = wallet.wallet_tier;
                                        let conf = wallet.confidence;
                                        info!(
                                            "👤 WalletActivity: {} | {} | {} | {:.2} SOL, tier: {}, conf: {}",
                                            &mint_str[..8], &wallet_str[..8],
                                            if action == 0 { "BUY" } else { "SELL" },
                                            size, tier, conf
                                        );
                                    }
                                    AdviceMessage::ExitAck(ack) => {
                                        let mint_str = bs58::encode(&ack.mint).into_string();
                                        let trade_id_str = String::from_utf8_lossy(&ack.trade_id).trim_end_matches('\0').to_string();
                                        info!(
                                            "✅ ExitAck received: mint={} trade_id={}",
                                            &mint_str[..12],
                                            &trade_id_str[..8]
                                        );
                                    }
                                    AdviceMessage::EnterAck(ack) => {
                                        let mint_str = bs58::encode(&ack.mint).into_string();
                                        let trade_id_str = String::from_utf8_lossy(&ack.trade_id).trim_end_matches('\0').to_string();
                                        info!(
                                            "✅ EnterAck received: mint={} trade_id={}",
                                            &mint_str[..12],
                                            &trade_id_str[..8]
                                        );
                                    }
                                    AdviceMessage::TxConfirmed(confirmed) => {
                                        let mint_str = bs58::encode(&confirmed.mint).into_string();
                                        let sig_str = bs58::encode(&confirmed.signature).into_string();
                                        let side = if confirmed.side == 0 { "BUY" } else { "SELL" };
                                        let status = if confirmed.is_success() { "SUCCESS" } else { "FAILED" };
                                        info!(
                                            "✅ TxConfirmed: {} {} | mint={} | sig={}",
                                            side, status,
                                            &mint_str[..12],
                                            &sig_str[..12]
                                        );
                                    }
                                    AdviceMessage::TradeClosed(closed) => {
                                        let mint_str = bs58::encode(&closed.mint).into_string();
                                        let trade_id_str = String::from_utf8_lossy(&closed.trade_id).trim_end_matches('\0').to_string();
                                        let side = if closed.side == 0 { "BUY" } else { "SELL" };
                                        let status = match closed.final_status {
                                            0 => "CONFIRMED",
                                            1 => "FAILED",
                                            2 => "TIMEOUT",
                                            _ => "UNKNOWN",
                                        };
                                        info!(
                                            "🏁 TradeClosed: {} {} | mint={} trade_id={}",
                                            side, status,
                                            &mint_str[..12],
                                            &trade_id_str[..8]
                                        );
                                    }
                                    AdviceMessage::WindowMetrics(metrics) => {
                                        let mint_str = bs58::encode(&metrics.mint).into_string();
                                        let volume = metrics.volume_sol();
                                        // Copy packed fields before use
                                        let buyers = metrics.unique_buyers_1s;
                                        let price_change = metrics.price_change_bps_2s;
                                        let alpha_hits = metrics.alpha_wallet_hits_10s;
                                        debug!(
                                            "📊 WindowMetrics: {} | vol_1s: {:.2} SOL, buyers_1s: {}, Δprice_2s: {}bps, alpha_10s: {}",
                                            &mint_str[..12],
                                            volume,
                                            buyers,
                                            price_change,
                                            alpha_hits
                                        );
                                    }
                                    AdviceMessage::PositionUpdate(update) => {
                                        let mint_str = bs58::encode(&update.mint).into_string();
                                        let pnl = update.realized_pnl_usd;
                                        let pnl_pct = update.pnl_percent;
                                        debug!(
                                            "📊 PositionUpdate: {} | P&L: ${:.2} ({:.1}%)",
                                            &mint_str[..8],
                                            pnl,
                                            pnl_pct
                                        );
                                    }
                                }
                                
                                // Forward to channel
                                if let Err(e) = tx.send(msg).await {
                                    warn!("⚠️ Failed to forward advice message: channel closed - {}", e);
                                    break;
                                }
                            }
                            None => {
                                parse_error_count.fetch_add(1, Ordering::Relaxed);
                                warn!(
                                    "⚠️ Failed to parse advice message: {} bytes, type={}",
                                    len,
                                    if !buf.is_empty() { buf[0] } else { 0 }
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ UDP receive error: {}", e);
                        // Continue receiving despite errors
                    }
                }
            }
            
            info!("🛑 Advice Bus receiver stopped");
        });
        
        rx
    }
    
    /// Stop receiving messages
    pub fn stop(&self) {
        info!("🛑 Stopping Advice Bus receiver...");
        self.running.store(false, Ordering::Relaxed);
    }
    
    /// Get current statistics
    pub fn stats(&self) -> ReceiverStats {
        ReceiverStats {
            total_received: self.total_received.load(Ordering::Relaxed),
            extend_hold: self.extend_hold_count.load(Ordering::Relaxed),
            widen_exit: self.widen_exit_count.load(Ordering::Relaxed),
            late_opportunity: self.late_opportunity_count.load(Ordering::Relaxed),
            copy_trade: self.copy_trade_count.load(Ordering::Relaxed),
            sol_price_update: self.sol_price_update_count.load(Ordering::Relaxed),
            parse_errors: self.parse_error_count.load(Ordering::Relaxed),
        }
    }
    
    /// Reset statistics
    pub fn reset_stats(&self) {
        self.total_received.store(0, Ordering::Relaxed);
        self.extend_hold_count.store(0, Ordering::Relaxed);
        self.widen_exit_count.store(0, Ordering::Relaxed);
        self.late_opportunity_count.store(0, Ordering::Relaxed);
        self.copy_trade_count.store(0, Ordering::Relaxed);
        self.sol_price_update_count.store(0, Ordering::Relaxed);
        self.parse_error_count.store(0, Ordering::Relaxed);
    }
    
    /// Print statistics summary
    pub fn print_stats(&self) {
        let stats = self.stats();
        info!("📊 Advice Bus Statistics:");
        info!("   Total received: {}", stats.total_received);
        info!("   ExtendHold: {}", stats.extend_hold);
        info!("   WidenExit: {}", stats.widen_exit);
        info!("   LateOpportunity: {}", stats.late_opportunity);
        info!("   CopyTrade: {}", stats.copy_trade);
        info!("   SolPriceUpdate: {}", stats.sol_price_update);
        info!("   Parse errors: {}", stats.parse_errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_receiver_stats_initialization() {
        let receiver = AdviceBusReceiver::new().await;
        // Note: This test may fail if port 45100 is already in use
        // In production, use a different port for testing
        if receiver.is_err() {
            println!("Port 45100 already in use, skipping test");
            return;
        }
        
        let receiver = receiver.unwrap();
        let stats = receiver.stats();
        assert_eq!(stats.total_received, 0);
        assert_eq!(stats.copy_trade, 0);
    }
    
    #[test]
    fn test_receiver_stats_struct() {
        let stats = ReceiverStats::default();
        assert_eq!(stats.total_received, 0);
        assert_eq!(stats.parse_errors, 0);
    }
}
