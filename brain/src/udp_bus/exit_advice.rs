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
    
    /// Get price change percentage
    pub fn price_change_percent(&self) -> f64 {
        if self.entry_price_lamports == 0 {
            return 0.0;
        }
        
        let price_diff = self.current_price_lamports as f64 - self.entry_price_lamports as f64;
        (price_diff / self.entry_price_lamports as f64) * 100.0
    }
}
