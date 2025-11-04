//! 📡 TxConfirmed Message - Mempool-watcher → Executor + Brain
//!
//! Sent when a watched signature appears in the confirmed transaction stream.
//! This provides 100-200ms confirmation with zero RPC calls.

use anyhow::{Result, Context};

/// TxConfirmed message from Mempool-watcher
/// 
/// MSG_TYPE = 26
/// SIZE = 128 bytes
/// 
/// Sent to BOTH Executor (port 45110) and Brain (port 45115) when signature confirmed
#[derive(Debug, Clone)]
pub struct TxConfirmed {
    pub msg_type: u8,
    pub signature: [u8; 64],
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub side: u8,  // 0=BUY, 1=SELL
    pub status: u8,  // 0=SUCCESS, 1=FAILED
    pub timestamp_ns: u64,
}

impl TxConfirmed {
    pub const MSG_TYPE: u8 = 26;
    pub const SIZE: usize = 128;
    
    pub const STATUS_SUCCESS: u8 = 0;
    pub const STATUS_FAILED: u8 = 1;
    
    /// Create new TxConfirmed message
    pub fn new(
        signature: [u8; 64],
        mint: [u8; 32],
        trade_id: [u8; 16],
        side: u8,
        status: u8,
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
            side,
            status,
            timestamp_ns,
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        
        buf[0] = self.msg_type;
        buf[1..65].copy_from_slice(&self.signature);
        buf[65..97].copy_from_slice(&self.mint);
        buf[97..113].copy_from_slice(&self.trade_id);
        buf[113] = self.side;
        buf[114] = self.status;
        buf[115..123].copy_from_slice(&self.timestamp_ns.to_le_bytes());
        // buf[123..128] = padding
        
        buf
    }
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("TxConfirmed requires {} bytes, got {}", Self::SIZE, data.len());
        }
        
        let msg_type = data[0];
        if msg_type != Self::MSG_TYPE {
            anyhow::bail!("Invalid msg_type: expected {}, got {}", Self::MSG_TYPE, msg_type);
        }
        
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&data[1..65]);
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[65..97]);
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[97..113]);
        
        let side = data[113];
        let status = data[114];
        
        let timestamp_ns = u64::from_le_bytes(
            data[115..123].try_into().context("Invalid timestamp_ns")?
        );
        
        Ok(Self {
            msg_type,
            signature,
            mint,
            trade_id,
            side,
            status,
            timestamp_ns,
        })
    }
    
    /// Get signature as base58 string
    pub fn signature_str(&self) -> String {
        bs58::encode(&self.signature).into_string()
    }
    
    /// Get mint as base58 string
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
    
    /// Get trade_id as hex string
    pub fn trade_id_str(&self) -> String {
        hex::encode(&self.trade_id)
    }
    
    /// Get status as string
    pub fn status_str(&self) -> &str {
        match self.status {
            Self::STATUS_SUCCESS => "SUCCESS",
            Self::STATUS_FAILED => "FAILED",
            _ => "UNKNOWN",
        }
    }
    
    /// Get side as string
    pub fn side_str(&self) -> &str {
        match self.side {
            0 => "BUY",
            1 => "SELL",
            _ => "UNKNOWN",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tx_confirmed_serialization() {
        let sig = [1u8; 64];
        let mint = [2u8; 32];
        let trade_id = [3u8; 16];
        
        let msg = TxConfirmed::new(sig, mint, trade_id, 0, TxConfirmed::STATUS_SUCCESS);
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), TxConfirmed::SIZE);
        assert_eq!(bytes[0], TxConfirmed::MSG_TYPE);
        
        let parsed = TxConfirmed::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.signature, sig);
        assert_eq!(parsed.mint, mint);
        assert_eq!(parsed.trade_id, trade_id);
        assert_eq!(parsed.side, 0);
        assert_eq!(parsed.status, TxConfirmed::STATUS_SUCCESS);
    }
}
