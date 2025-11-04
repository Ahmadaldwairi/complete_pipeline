//! 📡 TxConfirmedContext Message - Enhanced confirmation with Δ-window market context
//!
//! Received from Mempool-watcher when a watched signature is confirmed,
//! WITH additional market data collected in a 150-250ms window AFTER the transaction.

use anyhow::{Result, Context};

/// TxConfirmedContext message from Mempool-watcher
/// 
/// MSG_TYPE = 27 (enhanced version of TxConfirmed)
/// SIZE = 192 bytes
/// 
/// Contains Δ-window market context for instant decision-making
#[derive(Debug, Clone)]
pub struct TxConfirmedContext {
    // ===== Core Identification =====
    pub msg_type: u8,
    pub signature: [u8; 64],
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub side: u8,  // 0=BUY, 1=SELL
    pub status: u8,  // 0=SUCCESS, 1=FAILED
    pub slot: u64,
    pub timestamp_ns: u64,
    
    // ===== Δ-Window Context (150-250ms after our tx) =====
    pub trail_ms: u16,
    pub same_slot_after: u16,
    pub next_slot_count: u16,
    pub uniq_buyers_delta: u16,
    pub vol_buy_sol_delta: u32,  // Scaled by 1000
    pub vol_sell_sol_delta: u32,  // Scaled by 1000
    pub price_change_bps_delta: i16,  // Signed basis points
    pub alpha_hits_delta: u8,
    
    // ===== Entry Trade Data =====
    pub entry_price_lamports: u64,
    pub size_sol_scaled: u32,  // Scaled by 1000
    pub slippage_bps: u16,
    pub fee_bps: u16,
    pub realized_pnl_cents: i32,  // Signed USD cents
}

impl TxConfirmedContext {
    pub const MSG_TYPE: u8 = 27;
    pub const SIZE: usize = 192;
    
    pub const STATUS_SUCCESS: u8 = 0;
    pub const STATUS_FAILED: u8 = 1;
    
    pub const SIDE_BUY: u8 = 0;
    pub const SIDE_SELL: u8 = 1;
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("TxConfirmedContext requires {} bytes, got {}", Self::SIZE, data.len());
        }
        
        let mut offset = 0;
        
        let msg_type = data[offset]; offset += 1;
        if msg_type != Self::MSG_TYPE {
            anyhow::bail!("Invalid msg_type: expected {}, got {}", Self::MSG_TYPE, msg_type);
        }
        
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&data[offset..offset+64]); offset += 64;
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[offset..offset+32]); offset += 32;
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[offset..offset+16]); offset += 16;
        
        let side = data[offset]; offset += 1;
        let status = data[offset]; offset += 1;
        
        let slot = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid slot")?
        ); offset += 8;
        
        let timestamp_ns = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid timestamp_ns")?
        ); offset += 8;
        
        // Δ-window context
        let trail_ms = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid trail_ms")?
        ); offset += 2;
        
        let same_slot_after = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid same_slot_after")?
        ); offset += 2;
        
        let next_slot_count = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid next_slot_count")?
        ); offset += 2;
        
        let uniq_buyers_delta = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid uniq_buyers_delta")?
        ); offset += 2;
        
        let vol_buy_sol_delta = u32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid vol_buy_sol_delta")?
        ); offset += 4;
        
        let vol_sell_sol_delta = u32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid vol_sell_sol_delta")?
        ); offset += 4;
        
        let price_change_bps_delta = i16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid price_change_bps_delta")?
        ); offset += 2;
        
        let alpha_hits_delta = data[offset]; offset += 1;
        
        // Entry trade data
        let entry_price_lamports = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid entry_price_lamports")?
        ); offset += 8;
        
        let size_sol_scaled = u32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid size_sol_scaled")?
        ); offset += 4;
        
        let slippage_bps = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid slippage_bps")?
        ); offset += 2;
        
        let fee_bps = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid fee_bps")?
        ); offset += 2;
        
        let realized_pnl_cents = i32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid realized_pnl_cents")?
        );
        
        Ok(Self {
            msg_type,
            signature,
            mint,
            trade_id,
            side,
            status,
            slot,
            timestamp_ns,
            trail_ms,
            same_slot_after,
            next_slot_count,
            uniq_buyers_delta,
            vol_buy_sol_delta,
            vol_sell_sol_delta,
            price_change_bps_delta,
            alpha_hits_delta,
            entry_price_lamports,
            size_sol_scaled,
            slippage_bps,
            fee_bps,
            realized_pnl_cents,
        })
    }
    
    // ===== Convenience Methods =====
    
    pub fn signature_str(&self) -> String {
        bs58::encode(&self.signature).into_string()
    }
    
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
    
    pub fn trade_id_str(&self) -> String {
        hex::encode(&self.trade_id)
    }
    
    pub fn trade_id(&self) -> u128 {
        u128::from_le_bytes(self.trade_id)
    }
    
    pub fn status_str(&self) -> &str {
        match self.status {
            Self::STATUS_SUCCESS => "SUCCESS",
            Self::STATUS_FAILED => "FAILED",
            _ => "UNKNOWN",
        }
    }
    
    pub fn side_str(&self) -> &str {
        match self.side {
            Self::SIDE_BUY => "BUY",
            Self::SIDE_SELL => "SELL",
            _ => "UNKNOWN",
        }
    }
    
    pub fn is_buy(&self) -> bool {
        self.side == Self::SIDE_BUY
    }
    
    pub fn is_sell(&self) -> bool {
        self.side == Self::SIDE_SELL
    }
    
    pub fn is_success(&self) -> bool {
        self.status == Self::STATUS_SUCCESS
    }
    
    /// Get buy volume in SOL (unscaled)
    pub fn vol_buy_sol(&self) -> f64 {
        self.vol_buy_sol_delta as f64 / 1000.0
    }
    
    /// Get sell volume in SOL (unscaled)
    pub fn vol_sell_sol(&self) -> f64 {
        self.vol_sell_sol_delta as f64 / 1000.0
    }
    
    /// Get position size in SOL (unscaled)
    pub fn size_sol(&self) -> f64 {
        self.size_sol_scaled as f64 / 1000.0
    }
    
    /// Get realized P&L in USD (unscaled)
    pub fn realized_pnl_usd(&self) -> f64 {
        self.realized_pnl_cents as f64 / 100.0
    }
    
    /// Get price change as percentage
    pub fn price_change_percent(&self) -> f64 {
        self.price_change_bps_delta as f64 / 100.0
    }
    
    /// Check if we hit profit target (P&L > 0)
    pub fn is_profit_target_hit(&self) -> bool {
        self.realized_pnl_cents > 0
    }
    
    /// Check if momentum is building (more buyers than sellers)
    pub fn is_momentum_building(&self) -> bool {
        self.vol_buy_sol_delta > self.vol_sell_sol_delta
    }
    
    /// Check if position is fading (more sellers than buyers)
    pub fn is_fading(&self) -> bool {
        self.vol_sell_sol_delta > self.vol_buy_sol_delta
    }
    
    /// Check if alpha wallets are active in Δ-window
    pub fn has_alpha_activity(&self) -> bool {
        self.alpha_hits_delta > 0
    }
    
    /// Calculate net volume direction (positive = buying pressure)
    pub fn net_volume_sol(&self) -> f64 {
        self.vol_buy_sol() - self.vol_sell_sol()
    }
    
    /// Check if strong buying surge (>= 5 unique buyers and net positive volume)
    pub fn has_strong_buying_surge(&self) -> bool {
        self.uniq_buyers_delta >= 5 && self.is_momentum_building()
    }
    
    /// Check if strong selling pressure (sell volume > 2x buy volume)
    pub fn has_strong_selling_pressure(&self) -> bool {
        self.vol_sell_sol_delta > self.vol_buy_sol_delta * 2
    }
}
