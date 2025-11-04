//! 📡 Enhanced UDP Listener for WatchSigEnhanced messages from Executor
//!
//! Listens on port 45130 for enhanced signature registration with trade metadata.
//! Supports both basic WatchSignature (msg_type 25) and WatchSigEnhanced (msg_type 28).

use anyhow::Result;
use log::{debug, error, info};
use tokio::net::UdpSocket;
use std::sync::Arc;

use crate::watch_signature::{WatchSignature, SignatureTracker};
use crate::watch_sig_enhanced::{WatchSigEnhanced, SignatureTrackerEnhanced};
use crate::position_tracker::PositionTracker;

pub struct WatchSignatureListenerEnhanced {
    socket: UdpSocket,
    basic_tracker: Arc<SignatureTracker>,
    enhanced_tracker: Arc<SignatureTrackerEnhanced>,
    position_tracker: Arc<PositionTracker>,
}

impl WatchSignatureListenerEnhanced {
    /// Create new listener on specified address
    pub async fn new(
        bind_addr: &str,
        basic_tracker: Arc<SignatureTracker>,
        enhanced_tracker: Arc<SignatureTrackerEnhanced>,
        position_tracker: Arc<PositionTracker>,
    ) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        info!("✅ Enhanced WatchSignature Listener bound to {}", bind_addr);
        
        Ok(Self {
            socket,
            basic_tracker,
            enhanced_tracker,
            position_tracker,
        })
    }
    
    /// Start listening loop
    pub async fn listen(self) -> Result<()> {
        let mut buf = vec![0u8; 256];  // Support up to 256 bytes (WatchSigEnhanced is 192)
        
        info!("🎧 Enhanced WatchSignature Listener active - waiting for Executor messages");
        
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    debug!("📥 Received {} bytes from {}", len, addr);
                    
                    if len == 0 {
                        continue;
                    }
                    
                    // Check message type (first byte)
                    let msg_type = buf[0];
                    
                    match msg_type {
                        // Basic WatchSignature (msg_type 25, 128 bytes)
                        25 => {
                            match WatchSignature::from_bytes(&buf[..len]) {
                                Ok(watch) => {
                                    let sig = watch.signature_str();
                                    let mint = watch.mint_str();
                                    let trade_id = watch.trade_id_str();
                                    let side_str = if watch.side == 0 { "BUY" } else { "SELL" };
                                    
                                    info!("📝 [BASIC] {} signature: {} | mint: {} | trade_id: {}",
                                          side_str, &sig[..12], &mint[..12], &trade_id[..8]);
                                    
                                    self.basic_tracker.add(watch).await;
                                }
                                Err(e) => {
                                    error!("❌ Failed to parse WatchSignature from {}: {}", addr, e);
                                }
                            }
                        }
                        
                        // Enhanced WatchSigEnhanced (msg_type 28, 192 bytes)
                        28 => {
                            match WatchSigEnhanced::from_bytes(&buf[..len]) {
                                Ok(watch) => {
                                    let sig = watch.signature_str();
                                    let mint = watch.mint_str();
                                    let trade_id = watch.trade_id_str();
                                    
                                    info!(
                                        "📝 [ENHANCED] {} {} | mint: {} | trade_id: {} | size: {:.3} SOL @ {} lamports | target: ${:.2}",
                                        watch.side_str(),
                                        &sig[..12],
                                        &mint[..12],
                                        &trade_id[..8],
                                        watch.size_sol(),
                                        watch.entry_price_lamports,
                                        watch.profit_target_usd(),
                                    );
                                    
                                    // Add to enhanced tracker
                                    self.enhanced_tracker.add(watch.clone()).await;
                                    
                                    // Add to position tracker (for P&L monitoring)
                                    self.position_tracker.add_position(watch).await;
                                }
                                Err(e) => {
                                    error!("❌ Failed to parse WatchSigEnhanced from {}: {}", addr, e);
                                }
                            }
                        }
                        
                        _ => {
                            error!("❌ Unknown message type {} from {}", msg_type, addr);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to receive UDP packet: {}", e);
                }
            }
        }
    }
}
