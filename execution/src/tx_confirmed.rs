//! 📡 TxConfirmed Message - Mempool-watcher → Executor + Brain
//!
//! Received when mempool-watcher confirms a transaction on-chain.
//! This is the SOURCE OF TRUTH for transaction confirmation.

use anyhow::{Result, Context};

/// TxConfirmed message from Mempool-watcher
/// 
/// MSG_TYPE = 26
/// SIZE = 128 bytes
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
    
    /// Get trade_id as hex string (first 16 bytes of UUID)
    pub fn trade_id_hex(&self) -> String {
        hex::encode(&self.trade_id)
    }
    
    /// Get trade_id as u128 (for deduplication)
    pub fn trade_id(&self) -> u128 {
        u128::from_le_bytes(self.trade_id)
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
    
    /// Check if transaction succeeded
    pub fn is_success(&self) -> bool {
        self.status == Self::STATUS_SUCCESS
    }
    
    /// Check if transaction failed
    pub fn is_failure(&self) -> bool {
        self.status == Self::STATUS_FAILED
    }
}
