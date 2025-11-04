//! 📡 WatchSigEnhanced Sender - Executor → Mempool-watcher
//!
//! Sends enhanced watch signature messages with trade metadata for position tracking.
//! Enables mempool-watcher to calculate P&L and generate exit signals.

use anyhow::{Result, Context};
use std::net::UdpSocket;
use log::{info, warn, debug};

/// WatchSigEnhanced message struct (matches mempool-watcher)
/// MSG_TYPE = 28, SIZE = 192 bytes
#[derive(Debug, Clone)]
pub struct WatchSigEnhanced {
    pub msg_type: u8,
    pub signature: [u8; 64],
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub side: u8,  // 0=BUY, 1=SELL
    pub timestamp_ns: u64,
    pub entry_price_lamports: u64,
    pub size_sol_scaled: u32,  // SOL * 1000
    pub slippage_bps: u16,
    pub fee_bps: u16,
    pub profit_target_cents: u32,  // USD * 100
    pub stop_loss_cents: i32,      // USD * 100 (negative)
}

impl WatchSigEnhanced {
    pub const MSG_TYPE: u8 = 28;
    pub const SIZE: usize = 192;
    
    /// Create new WatchSigEnhanced for a BUY trade
    pub fn new_buy(
        signature: [u8; 64],
        mint: [u8; 32],
        trade_id: [u8; 16],
        entry_price_lamports: u64,
        size_sol: f64,
        slippage_bps: u16,
        profit_target_usd: f64,
        stop_loss_usd: f64,
    ) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        Self {
            msg_type: Self::MSG_TYPE,
            signature,
            mint,
            trade_id,
            side: 0, // BUY
            timestamp_ns,
            entry_price_lamports,
            size_sol_scaled: (size_sol * 1000.0) as u32,
            slippage_bps,
            fee_bps: 30, // 0.3% typical pump.fun fee
            profit_target_cents: (profit_target_usd * 100.0) as u32,
            stop_loss_cents: (stop_loss_usd * 100.0) as i32,
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        let mut offset = 0;
        
        buf[offset] = self.msg_type; offset += 1;
        buf[offset..offset+64].copy_from_slice(&self.signature); offset += 64;
        buf[offset..offset+32].copy_from_slice(&self.mint); offset += 32;
        buf[offset..offset+16].copy_from_slice(&self.trade_id); offset += 16;
        buf[offset] = self.side; offset += 1;
        buf[offset..offset+8].copy_from_slice(&self.timestamp_ns.to_le_bytes()); offset += 8;
        buf[offset..offset+8].copy_from_slice(&self.entry_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+4].copy_from_slice(&self.size_sol_scaled.to_le_bytes()); offset += 4;
        buf[offset..offset+2].copy_from_slice(&self.slippage_bps.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.fee_bps.to_le_bytes()); offset += 2;
        buf[offset..offset+4].copy_from_slice(&self.profit_target_cents.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.stop_loss_cents.to_le_bytes()); offset += 4;
        
        buf
    }
}

/// Sends WatchSigEnhanced messages to mempool-watcher
pub struct WatchSigEnhancedSender {
    socket: UdpSocket,
    target_addr: String,
}

impl WatchSigEnhancedSender {
    /// Create new sender targeting mempool-watcher on port 45130
    pub fn new() -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket for WatchSigEnhanced sender")?;
        
        socket.set_nonblocking(true)
            .context("Failed to set socket to non-blocking")?;
        
        Ok(Self {
            socket,
            target_addr: "127.0.0.1:45130".to_string(),
        })
    }
    
    /// Send WatchSigEnhanced message to mempool-watcher
    pub fn send(&self, msg: &WatchSigEnhanced) -> Result<()> {
        let bytes = msg.to_bytes();
        
        match self.socket.send_to(&bytes, &self.target_addr) {
            Ok(sent) => {
                if sent != WatchSigEnhanced::SIZE {
                    warn!("⚠️ WatchSigEnhanced: Expected to send {} bytes, sent {}", 
                          WatchSigEnhanced::SIZE, sent);
                }
                
                let mint_str = bs58::encode(&msg.mint).into_string();
                let sig_str = bs58::encode(&msg.signature).into_string();
                debug!("📤 Sent WatchSigEnhanced: {} | sig: {} | target: ${:.2} | stop: ${:.2}",
                       &mint_str[..8], &sig_str[..8],
                       msg.profit_target_cents as f64 / 100.0,
                       msg.stop_loss_cents as f64 / 100.0);
                
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Non-blocking socket, this is expected
                debug!("📤 WatchSigEnhanced queued (non-blocking)");
                Ok(())
            }
            Err(e) => {
                warn!("❌ Failed to send WatchSigEnhanced: {}", e);
                Err(e.into())
            }
        }
    }
}
