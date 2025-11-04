//! 📡 UDP Listener for WatchSignature messages from Executor
//!
//! Listens on port 45130 for signature registration requests.

use anyhow::Result;
use log::{debug, error, info};
use tokio::net::UdpSocket;
use std::sync::Arc;

use crate::watch_signature::{WatchSignature, SignatureTracker};

pub struct WatchSignatureListener {
    socket: UdpSocket,
    tracker: Arc<SignatureTracker>,
}

impl WatchSignatureListener {
    /// Create new listener on specified address
    pub async fn new(bind_addr: &str, tracker: Arc<SignatureTracker>) -> Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        info!("✅ WatchSignature Listener bound to {}", bind_addr);
        
        Ok(Self {
            socket,
            tracker,
        })
    }
    
    /// Start listening loop
    pub async fn listen(self) -> Result<()> {
        let mut buf = vec![0u8; 256];  // WatchSignature is 128 bytes, but buffer a bit more
        
        info!("🎧 WatchSignature Listener active - waiting for Executor messages");
        
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    debug!("📥 Received {} bytes from {}", len, addr);
                    
                    // Parse WatchSignature
                    match WatchSignature::from_bytes(&buf[..len]) {
                        Ok(watch) => {
                            let sig = watch.signature_str();
                            let mint = watch.mint_str();
                            let trade_id = watch.trade_id_str();
                            let side_str = if watch.side == 0 { "BUY" } else { "SELL" };
                            
                            info!("📝 Registered {} signature: {} | mint: {} | trade_id: {}",
                                  side_str, &sig[..12], &mint[..12], &trade_id[..8]);
                            
                            // Add to tracker
                            self.tracker.add(watch).await;
                        }
                        Err(e) => {
                            error!("❌ Failed to parse WatchSignature from {}: {}", addr, e);
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
