/*!
 * TradeClosed Message - Executor → Brain
 * 
 * Sent when a trade reaches its final terminal state (confirmed/failed/timeout).
 * Provides a definitive finalization signal for audit trails and state reconciliation.
 * This is sent AFTER TxConfirmed is processed and all internal state updates are complete.
 */

use anyhow::{Result, Context};
use std::net::UdpSocket;
use log::info;

/// TradeClosed message structure (type 28)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct TradeClosed {
    pub msg_type: u8,              // 28
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // UUID of trade (first 16 bytes)
    pub side: u8,                  // 0=BUY, 1=SELL
    pub final_status: u8,          // 0=CONFIRMED, 1=FAILED, 2=TIMEOUT
    pub timestamp_ns: u64,         // When trade was closed (nanoseconds)
    pub _padding: [u8; 6],         // Padding to 64 bytes
}

impl TradeClosed {
    pub const SIZE: usize = 64;
    pub const MSG_TYPE: u8 = 28;
    
    pub const STATUS_CONFIRMED: u8 = 0;
    pub const STATUS_FAILED: u8 = 1;
    pub const STATUS_TIMEOUT: u8 = 2;
    
    /// Create new TradeClosed message
    pub fn new(mint: [u8; 32], trade_id: &str, side: u8, final_status: u8) -> Self {
        let mut trade_id_bytes = [0u8; 16];
        // Take first 16 bytes of UUID (sufficient for uniqueness)
        let uuid_bytes = trade_id.as_bytes();
        let copy_len = uuid_bytes.len().min(16);
        trade_id_bytes[..copy_len].copy_from_slice(&uuid_bytes[..copy_len]);
        
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            trade_id: trade_id_bytes,
            side,
            final_status,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 6],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.mint);
        bytes.extend_from_slice(&self.trade_id);
        bytes.push(self.side);
        bytes.push(self.final_status);
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        bytes
    }
    
    /// Helper: get status as string
    pub fn status_str(&self) -> &'static str {
        match self.final_status {
            Self::STATUS_CONFIRMED => "CONFIRMED",
            Self::STATUS_FAILED => "FAILED",
            Self::STATUS_TIMEOUT => "TIMEOUT",
            _ => "UNKNOWN",
        }
    }
    
    /// Helper: get side as string
    pub fn side_str(&self) -> &'static str {
        if self.side == 0 { "BUY" } else { "SELL" }
    }
}

/// Send TradeClosed message to Brain
pub async fn send_trade_closed(
    brain_addr: &str,
    mint: [u8; 32],
    trade_id: &str,
    side: u8,
    final_status: u8
) -> Result<()> {
    let msg = TradeClosed::new(mint, trade_id, side, final_status);
    let bytes = msg.to_bytes();
    
    let socket = UdpSocket::bind("0.0.0.0:0")
        .context("Failed to bind UDP socket for TradeClosed")?;
    
    socket.send_to(&bytes, brain_addr)
        .context("Failed to send TradeClosed message")?;
    
    let mint_str = bs58::encode(&mint).into_string();
    info!("🏁 Sent TradeClosed to Brain: {} {} | mint={} trade_id={}",
          msg.side_str(),
          msg.status_str(),
          &mint_str[..12],
          &trade_id[..8]);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_trade_closed_size() {
        assert_eq!(TradeClosed::SIZE, 64);
        assert_eq!(std::mem::size_of::<TradeClosed>(), 64);
    }
    
    #[test]
    fn test_trade_closed_serialization() {
        let mint = [1u8; 32];
        let trade_id = "test-trade-123456";
        let msg = TradeClosed::new(mint, trade_id, 0, TradeClosed::STATUS_CONFIRMED);
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), TradeClosed::SIZE);
        assert_eq!(bytes[0], TradeClosed::MSG_TYPE);
        assert_eq!(&bytes[1..33], &mint);
    }
}
