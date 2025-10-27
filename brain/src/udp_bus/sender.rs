//! �� Decision Bus UDP Sender
//!
//! Sends TradeDecision packets to the Execution bot on port 45110.
//! Non-blocking async sends with error logging and retry logic.

use tokio::net::UdpSocket;
use std::net::SocketAddr;
use std::sync::Arc;
use log::{info, warn, error, debug};
use anyhow::{Result, Context};
use crate::udp_bus::messages::TradeDecision;

/// UDP sender for TradeDecision packets
pub struct DecisionBusSender {
    socket: Arc<UdpSocket>,
    target_addr: SocketAddr,
    sent_count: std::sync::atomic::AtomicU64,
    error_count: std::sync::atomic::AtomicU64,
}

impl DecisionBusSender {
    /// Create new Decision Bus sender
    /// 
    /// Binds to a local port and sends to the target address.
    /// Default target: 127.0.0.1:45110 (Execution bot)
    pub async fn new(target_addr: SocketAddr) -> Result<Self> {
        // Bind to any available local port
        let socket = UdpSocket::bind("127.0.0.1:0")
            .await
            .context("Failed to bind UDP socket for Decision Bus sender")?;
        
        let local_addr = socket.local_addr()?;
        info!("📡 Decision Bus sender bound to {} → target {}", local_addr, target_addr);
        
        Ok(Self {
            socket: Arc::new(socket),
            target_addr,
            sent_count: std::sync::atomic::AtomicU64::new(0),
            error_count: std::sync::atomic::AtomicU64::new(0),
        })
    }
    
    /// Create with default target (127.0.0.1:45110)
    pub async fn new_default() -> Result<Self> {
        let target = "127.0.0.1:45110".parse()
            .context("Invalid default target address")?;
        Self::new(target).await
    }
    
    /// Send a TradeDecision packet
    /// 
    /// Non-blocking async send with error logging.
    /// Returns Ok(()) if sent successfully, Err if send failed.
    pub async fn send_decision(&self, decision: &TradeDecision) -> Result<()> {
        // Serialize to bytes
        let bytes = decision.to_bytes();
        
        // Send packet
        match self.socket.send_to(&bytes, self.target_addr).await {
            Ok(sent_bytes) => {
                if sent_bytes != bytes.len() {
                    warn!(
                        "⚠️ Partial send: {} bytes sent, {} expected for decision {}...",
                        sent_bytes,
                        bytes.len(),
                        hex::encode(&decision.mint[..4])
                    );
                }
                
                self.sent_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                
                debug!(
                    "📤 Sent decision: mint={}..., side={}, size={} lamps, conf={}, slip={}bps",
                    hex::encode(&decision.mint[..4]),
                    decision.side,
                    decision.size_lamports,
                    decision.confidence,
                    decision.slippage_bps
                );
                
                Ok(())
            }
            Err(e) => {
                self.error_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                error!(
                    "❌ Failed to send decision for mint {}...: {}",
                    hex::encode(&decision.mint[..4]),
                    e
                );
                Err(e.into())
            }
        }
    }
    
    /// Send decision with retry logic
    /// 
    /// Attempts to send up to `max_retries` times with exponential backoff.
    /// Returns Ok(()) if any attempt succeeds, Err if all fail.
    pub async fn send_with_retry(
        &self,
        decision: &TradeDecision,
        max_retries: u32,
    ) -> Result<()> {
        let mut last_error = None;
        
        for attempt in 0..max_retries {
            match self.send_decision(decision).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    if attempt < max_retries - 1 {
                        let delay_ms = 10 * (2_u64.pow(attempt));
                        debug!(
                            "🔄 Retry {} for mint {}... in {}ms",
                            attempt + 1,
                            hex::encode(&decision.mint[..4]),
                            delay_ms
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                    last_error = Some(e);
                }
            }
        }
        
        Err(last_error.unwrap())
    }
    
    /// Get statistics
    pub fn stats(&self) -> (u64, u64) {
        let sent = self.sent_count.load(std::sync::atomic::Ordering::Relaxed);
        let errors = self.error_count.load(std::sync::atomic::Ordering::Relaxed);
        (sent, errors)
    }
    
    /// Reset statistics
    pub fn reset_stats(&self) {
        self.sent_count.store(0, std::sync::atomic::Ordering::Relaxed);
        self.error_count.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Batch sender for multiple decisions
pub struct DecisionBatchSender {
    sender: DecisionBusSender,
}

impl DecisionBatchSender {
    /// Create new batch sender
    pub async fn new(target_addr: SocketAddr) -> Result<Self> {
        let sender = DecisionBusSender::new(target_addr).await?;
        Ok(Self { sender })
    }
    
    /// Send multiple decisions with rate limiting
    /// 
    /// Sends decisions with a delay between each to avoid overwhelming the receiver.
    /// Returns (success_count, error_count)
    pub async fn send_batch(
        &self,
        decisions: Vec<TradeDecision>,
        delay_ms: u64,
    ) -> (usize, usize) {
        let mut success = 0;
        let mut errors = 0;
        
        for (i, decision) in decisions.iter().enumerate() {
            match self.sender.send_decision(decision).await {
                Ok(()) => success += 1,
                Err(_) => errors += 1,
            }
            
            // Add delay between sends (except for last one)
            if i < decisions.len() - 1 && delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }
        
        info!(
            "📦 Batch send complete: {} success, {} errors",
            success, errors
        );
        
        (success, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;
    
    fn mock_decision() -> TradeDecision {
        TradeDecision {
            msg_type: 1,
            mint: Pubkey::new_unique().to_bytes(),
            side: 0,
            size_lamports: 10_000_000_000,
            slippage_bps: 150,
            confidence: 75,
            _padding: [0u8; 5],
        }
    }
    
    #[tokio::test]
    async fn test_sender_creation() {
        let target = "127.0.0.1:45110".parse().unwrap();
        let sender = DecisionBusSender::new(target).await;
        assert!(sender.is_ok(), "Sender should be created successfully");
    }
    
    #[tokio::test]
    async fn test_sender_default() {
        let sender = DecisionBusSender::new_default().await;
        assert!(sender.is_ok(), "Default sender should be created");
    }
    
    #[tokio::test]
    async fn test_stats_initialization() {
        let sender = DecisionBusSender::new_default().await.unwrap();
        let (sent, errors) = sender.stats();
        assert_eq!(sent, 0);
        assert_eq!(errors, 0);
    }
    
    #[tokio::test]
    async fn test_stats_reset() {
        let sender = DecisionBusSender::new_default().await.unwrap();
        sender.sent_count.store(10, std::sync::atomic::Ordering::Relaxed);
        sender.error_count.store(5, std::sync::atomic::Ordering::Relaxed);
        
        sender.reset_stats();
        let (sent, errors) = sender.stats();
        assert_eq!(sent, 0);
        assert_eq!(errors, 0);
    }
    
    #[tokio::test]
    async fn test_decision_serialization() {
        let decision = mock_decision();
        let bytes = decision.to_bytes();
        assert_eq!(bytes.len(), 52, "TradeDecision should be 52 bytes");
    }
    
    #[tokio::test]
    async fn test_batch_sender_creation() {
        let target = "127.0.0.1:45110".parse().unwrap();
        let batch_sender = DecisionBatchSender::new(target).await;
        assert!(batch_sender.is_ok(), "Batch sender should be created");
    }
    
    // Note: Full send tests would require a listening socket
    // These are integration tests that should be run with a test receiver
}
