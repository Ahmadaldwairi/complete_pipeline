//! 📡 ExitAdvice Message - Mempool-watcher → Brain
//!
//! Sent when Watcher detects profit target hit or stop-loss triggered.
//! Brain receives this and generates SELL decision if conditions are favorable.

use anyhow::{Result, Context};

/// ExitAdvice message from Mempool-watcher to Brain
/// 
/// MSG_TYPE = 30 (new type for exit advice from profit estimation)
/// SIZE = 96 bytes
/// 
/// Sent from Mempool-watcher to Brain (Port 4001) when:
/// 1. Profit target reached (realized_pnl >= profit_target)
/// 2. Stop-loss triggered (realized_pnl <= stop_loss)
/// 
/// Brain uses this to make SELL decisions, considering:
/// - Current market momentum
/// - Position hold time
/// - Exit slippage requirements
#[derive(Debug, Clone)]
pub struct ExitAdvice {
    pub msg_type: u8,              // 30
    pub trade_id: [u8; 16],        // Trade identifier
    pub mint: [u8; 32],            // Token mint
    pub reason: u8,                // Exit reason: 0=target_hit, 1=stop_loss, 2=fade_detected
    pub confidence: u8,            // Confidence score 0-100
    pub realized_pnl_cents: i32,   // Current realized P&L in USD cents
    pub entry_price_lamports: u64, // Entry price for reference
    pub current_price_lamports: u64, // Current price
    pub hold_time_ms: u32,         // Time since entry (milliseconds)
    pub timestamp_ns: u64,         // When advice generated
    pub _padding: [u8; 8],
}

impl ExitAdvice {
    pub const MSG_TYPE: u8 = 30;
    pub const SIZE: usize = 96;
    
    // Exit reason codes
    pub const REASON_TARGET_HIT: u8 = 0;
    pub const REASON_STOP_LOSS: u8 = 1;
    pub const REASON_FADE_DETECTED: u8 = 2;
    
    /// Create new ExitAdvice message
    pub fn new(
        trade_id: [u8; 16],
        mint: [u8; 32],
        reason: u8,
        confidence: u8,
        realized_pnl_usd: f64,
        entry_price_lamports: u64,
        current_price_lamports: u64,
        hold_time_ms: u32,
    ) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        Self {
            msg_type: Self::MSG_TYPE,
            trade_id,
            mint,
            reason,
            confidence,
            realized_pnl_cents: (realized_pnl_usd * 100.0) as i32,
            entry_price_lamports,
            current_price_lamports,
            hold_time_ms,
            timestamp_ns,
            _padding: [0u8; 8],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.trade_id);
        bytes.extend_from_slice(&self.mint);
        bytes.push(self.reason);
        bytes.push(self.confidence);
        bytes.extend_from_slice(&self.realized_pnl_cents.to_le_bytes());
        bytes.extend_from_slice(&self.entry_price_lamports.to_le_bytes());
        bytes.extend_from_slice(&self.current_price_lamports.to_le_bytes());
        bytes.extend_from_slice(&self.hold_time_ms.to_le_bytes());
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        
        bytes
    }
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("ExitAdvice requires {} bytes, got {}", Self::SIZE, data.len());
        }
        
        let mut offset = 0;
        
        let msg_type = data[offset];
        offset += 1;
        
        if msg_type != Self::MSG_TYPE {
            anyhow::bail!("Invalid msg_type: expected {}, got {}", Self::MSG_TYPE, msg_type);
        }
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[offset..offset+16]);
        offset += 16;
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[offset..offset+32]);
        offset += 32;
        
        let reason = data[offset];
        offset += 1;
        
        let confidence = data[offset];
        offset += 1;
        
        let realized_pnl_cents = i32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid realized_pnl_cents")?
        );
        offset += 4;
        
        let entry_price_lamports = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid entry_price_lamports")?
        );
        offset += 8;
        
        let current_price_lamports = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid current_price_lamports")?
        );
        offset += 8;
        
        let hold_time_ms = u32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid hold_time_ms")?
        );
        offset += 4;
        
        let timestamp_ns = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid timestamp_ns")?
        );
        
        Ok(Self {
            msg_type,
            trade_id,
            mint,
            reason,
            confidence,
            realized_pnl_cents,
            entry_price_lamports,
            current_price_lamports,
            hold_time_ms,
            timestamp_ns,
            _padding: [0u8; 8],
        })
    }
    
    // ===== Convenience Methods =====
    
    /// Get trade_id as hex string
    pub fn trade_id_str(&self) -> String {
        hex::encode(&self.trade_id)
    }
    
    /// Get mint as base58 string
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
    
    /// Get reason as string
    pub fn reason_str(&self) -> &str {
        match self.reason {
            Self::REASON_TARGET_HIT => "target_hit",
            Self::REASON_STOP_LOSS => "stop_loss",
            Self::REASON_FADE_DETECTED => "fade_detected",
            _ => "unknown",
        }
    }
    
    /// Get realized P&L in USD (unscaled)
    pub fn realized_pnl_usd(&self) -> f64 {
        self.realized_pnl_cents as f64 / 100.0
    }
    
    /// Get hold time in seconds
    pub fn hold_time_secs(&self) -> f64 {
        self.hold_time_ms as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_exit_advice_serialization() {
        let trade_id = [1u8; 16];
        let mint = [2u8; 32];
        
        let msg = ExitAdvice::new(
            trade_id,
            mint,
            ExitAdvice::REASON_TARGET_HIT,
            95,
            2.50, // $2.50 profit
            1000000, // entry price
            1250000, // current price (+25%)
            5000, // 5 seconds
        );
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), ExitAdvice::SIZE);
        assert_eq!(bytes[0], ExitAdvice::MSG_TYPE);
        
        let parsed = ExitAdvice::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.trade_id, trade_id);
        assert_eq!(parsed.mint, mint);
        assert_eq!(parsed.reason, ExitAdvice::REASON_TARGET_HIT);
        assert_eq!(parsed.confidence, 95);
        assert_eq!(parsed.realized_pnl_cents, 250); // $2.50 = 250 cents
        assert_eq!(parsed.entry_price_lamports, 1000000);
        assert_eq!(parsed.current_price_lamports, 1250000);
        assert_eq!(parsed.hold_time_ms, 5000);
    }
    
    #[test]
    fn test_exit_advice_helpers() {
        let msg = ExitAdvice::new(
            [3u8; 16],
            [4u8; 32],
            ExitAdvice::REASON_STOP_LOSS,
            80,
            -0.50, // -$0.50 loss
            1000000,
            900000,
            3000,
        );
        
        assert_eq!(msg.reason_str(), "stop_loss");
        assert_eq!(msg.realized_pnl_usd(), -0.50);
        assert_eq!(msg.hold_time_secs(), 3.0);
    }
    
    #[test]
    fn test_message_size() {
        // Ensure message fits in optimal UDP size
        assert_eq!(ExitAdvice::SIZE, 96);
        assert!(ExitAdvice::SIZE < 512); // Well under UDP optimal threshold
    }
}
