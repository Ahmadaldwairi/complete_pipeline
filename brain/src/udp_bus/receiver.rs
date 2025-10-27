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
use crate::udp_bus::messages::{AdviceMessage, AdviceMessageType};

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
