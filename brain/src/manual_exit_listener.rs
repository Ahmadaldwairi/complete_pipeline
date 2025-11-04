//! 💰 Manual Exit Listener - Receives ManualExitNotification from Mempool (Port 45135)
//!
//! When user manually exits via Phantom wallet, mempool sends notification to clean up position

use anyhow::{Context, Result};
use log::{debug, error, info};
use tokio::net::UdpSocket;

/// MSG_TYPE 33 - Manual Exit Notification from Mempool
/// Sent when user manually sells tracked position via wallet
#[repr(C, packed)]
pub struct ManualExitNotification {
    pub msg_type: u8,  // 33
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub exit_signature: [u8; 64],
    pub timestamp: u64,
    pub entry_price_lamports: u64,
    pub exit_price_lamports: u64,
    pub size_sol: f32,
    pub realized_pnl_usd: f32,
    pub pnl_percent: f32,
    pub hold_time_secs: u32,
    pub _padding: [u8; 7],
}

impl ManualExitNotification {
    pub const MSG_TYPE: u8 = 33;
    pub const SIZE: usize = 192;
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            anyhow::bail!("ManualExitNotification requires {} bytes, got {}", Self::SIZE, data.len());
        }
        
        if data[0] != Self::MSG_TYPE {
            anyhow::bail!("Invalid msg_type: expected {}, got {}", Self::MSG_TYPE, data[0]);
        }
        
        let mut offset = 1;
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[offset..offset+32]);
        offset += 32;
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[offset..offset+16]);
        offset += 16;
        
        let mut exit_signature = [0u8; 64];
        exit_signature.copy_from_slice(&data[offset..offset+64]);
        offset += 64;
        
        let timestamp = u64::from_le_bytes(data[offset..offset+8].try_into()?);
        offset += 8;
        
        let entry_price_lamports = u64::from_le_bytes(data[offset..offset+8].try_into()?);
        offset += 8;
        
        let exit_price_lamports = u64::from_le_bytes(data[offset..offset+8].try_into()?);
        offset += 8;
        
        let size_sol = f32::from_le_bytes(data[offset..offset+4].try_into()?);
        offset += 4;
        
        let realized_pnl_usd = f32::from_le_bytes(data[offset..offset+4].try_into()?);
        offset += 4;
        
        let pnl_percent = f32::from_le_bytes(data[offset..offset+4].try_into()?);
        offset += 4;
        
        let hold_time_secs = u32::from_le_bytes(data[offset..offset+4].try_into()?);
        
        Ok(Self {
            msg_type: Self::MSG_TYPE,
            mint,
            trade_id,
            exit_signature,
            timestamp,
            entry_price_lamports,
            exit_price_lamports,
            size_sol,
            realized_pnl_usd,
            pnl_percent,
            hold_time_secs,
            _padding: [0u8; 7],
        })
    }
    
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
}

/// Manual Exit Listener for Brain - receives notifications on port 45135
pub struct ManualExitListener;

impl ManualExitListener {
    /// Start listening for manual exit notifications
    /// Returns a channel receiver for manual exit events
    pub async fn listen(bind_addr: &str) -> Result<tokio::sync::mpsc::UnboundedReceiver<String>> {
        let socket = UdpSocket::bind(bind_addr)
            .await
            .context(format!("Failed to bind ManualExit listener to {}", bind_addr))?;
        
        info!("💰 ManualExit listener bound to {}", bind_addr);
        
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        
        tokio::spawn(async move {
            let mut buf = vec![0u8; ManualExitNotification::SIZE + 100];
            
            loop {
                match socket.recv(&mut buf).await {
                    Ok(len) => {
                        if len < ManualExitNotification::SIZE {
                            debug!("Received undersized packet: {} bytes", len);
                            continue;
                        }
                        
                        match ManualExitNotification::from_bytes(&buf[..ManualExitNotification::SIZE]) {
                            Ok(notification) => {
                                // Copy values to avoid packed struct alignment issues
                                let pnl_usd = notification.realized_pnl_usd;
                                let pnl_pct = notification.pnl_percent;
                                let hold_time = notification.hold_time_secs;
                                let mint_str = notification.mint_str();
                                
                                info!("💰 Manual exit detected for cleanup: {} | P&L: ${:.2} ({:.1}%) | Hold: {}s",
                                      &mint_str[..8], pnl_usd, pnl_pct, hold_time);
                                
                                // Send mint string for position cleanup
                                if let Err(e) = tx.send(mint_str.clone()) {
                                    error!("❌ Failed to send manual exit to handler: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("❌ Failed to parse ManualExitNotification: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("❌ Socket receive error: {}", e);
                    }
                }
            }
        });
        
        Ok(rx)
    }
}
