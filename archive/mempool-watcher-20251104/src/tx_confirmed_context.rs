//! 📡 TxConfirmedContext Message - Enhanced confirmation with Δ-window market context
//!
//! Sent when a watched signature appears in the confirmed transaction stream,
//! WITH additional market data collected in a 150-250ms window AFTER the tx.
//! This enables instant decision-making by the Brain without additional queries.

use anyhow::{Result, Context};

/// TxConfirmedContext message from Mempool-watcher
/// 
/// MSG_TYPE = 27 (new type to differentiate from basic TxConfirmed)
/// SIZE = 192 bytes (expanded from 128 to include Δ-window fields)
/// 
/// Sent to BOTH Executor (port 45110) and Brain (port 45115) when signature confirmed
/// 
/// # Δ-window (Delta Window)
/// After detecting our tx in slot S, the watcher buffers for 150-250ms to capture:
/// - Transactions after ours in the same slot S
/// - Early transactions in slot S+1
/// This provides "momentum" or "fade" context for instant decision-making.
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
    /// Actual micro-buffer duration in milliseconds (150-250)
    pub trail_ms: u16,
    
    /// Count of transactions AFTER ours in the same slot S
    pub same_slot_after: u16,
    
    /// Count of transactions in next slot S+1 within Δ window
    pub next_slot_count: u16,
    
    /// Unique buyer wallets detected in Δ window
    pub uniq_buyers_delta: u16,
    
    /// Total SOL volume from buy transactions in Δ window (scaled by 1000, e.g., 5.5 SOL = 5500)
    pub vol_buy_sol_delta: u32,
    
    /// Total SOL volume from sell transactions in Δ window (scaled by 1000)
    pub vol_sell_sol_delta: u32,
    
    /// Price change in basis points since our transaction (signed, e.g., +150 = +1.5%, -200 = -2.0%)
    pub price_change_bps_delta: i16,
    
    /// Number of transactions from known alpha wallets in Δ window
    pub alpha_hits_delta: u8,
    
    // ===== Optional: Entry Trade Data (from WatchSig) =====
    /// Entry price in lamports per token (for profit calculation)
    pub entry_price_lamports: u64,
    
    /// Position size in SOL (scaled by 1000, e.g., 0.5 SOL = 500)
    pub size_sol_scaled: u32,
    
    /// Slippage tolerance in basis points (from original order)
    pub slippage_bps: u16,
    
    /// Fee paid in basis points
    pub fee_bps: u16,
    
    /// Estimated realized P&L in USD cents (signed, e.g., +150 = $1.50 profit)
    pub realized_pnl_cents: i32,
}

impl TxConfirmedContext {
    pub const MSG_TYPE: u8 = 27;
    pub const SIZE: usize = 192;
    
    pub const STATUS_SUCCESS: u8 = 0;
    pub const STATUS_FAILED: u8 = 1;
    
    pub const SIDE_BUY: u8 = 0;
    pub const SIDE_SELL: u8 = 1;
    
    /// Create new TxConfirmedContext with Δ-window data
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        signature: [u8; 64],
        mint: [u8; 32],
        trade_id: [u8; 16],
        side: u8,
        status: u8,
        slot: u64,
        // Δ-window fields
        trail_ms: u16,
        same_slot_after: u16,
        next_slot_count: u16,
        uniq_buyers_delta: u16,
        vol_buy_sol_delta: f64,
        vol_sell_sol_delta: f64,
        price_change_bps_delta: i16,
        alpha_hits_delta: u8,
        // Entry data (optional, from WatchSig)
        entry_price_lamports: u64,
        size_sol: f64,
        slippage_bps: u16,
        fee_bps: u16,
        realized_pnl_usd: f64,
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
            slot,
            timestamp_ns,
            trail_ms,
            same_slot_after,
            next_slot_count,
            uniq_buyers_delta,
            vol_buy_sol_delta: (vol_buy_sol_delta * 1000.0) as u32,
            vol_sell_sol_delta: (vol_sell_sol_delta * 1000.0) as u32,
            price_change_bps_delta,
            alpha_hits_delta,
            entry_price_lamports,
            size_sol_scaled: (size_sol * 1000.0) as u32,
            slippage_bps,
            fee_bps,
            realized_pnl_cents: (realized_pnl_usd * 100.0) as i32,
        }
    }
    
    /// Serialize to bytes for UDP transmission (192 bytes)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        let mut offset = 0;
        
        // Core identification (123 bytes)
        buf[offset] = self.msg_type; offset += 1;
        buf[offset..offset+64].copy_from_slice(&self.signature); offset += 64;
        buf[offset..offset+32].copy_from_slice(&self.mint); offset += 32;
        buf[offset..offset+16].copy_from_slice(&self.trade_id); offset += 16;
        buf[offset] = self.side; offset += 1;
        buf[offset] = self.status; offset += 1;
        buf[offset..offset+8].copy_from_slice(&self.slot.to_le_bytes()); offset += 8;
        buf[offset..offset+8].copy_from_slice(&self.timestamp_ns.to_le_bytes()); offset += 8;
        
        // Δ-window context (26 bytes)
        buf[offset..offset+2].copy_from_slice(&self.trail_ms.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.same_slot_after.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.next_slot_count.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.uniq_buyers_delta.to_le_bytes()); offset += 2;
        buf[offset..offset+4].copy_from_slice(&self.vol_buy_sol_delta.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.vol_sell_sol_delta.to_le_bytes()); offset += 4;
        buf[offset..offset+2].copy_from_slice(&self.price_change_bps_delta.to_le_bytes()); offset += 2;
        buf[offset] = self.alpha_hits_delta; offset += 1;
        
        // Entry trade data (27 bytes)
        buf[offset..offset+8].copy_from_slice(&self.entry_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+4].copy_from_slice(&self.size_sol_scaled.to_le_bytes()); offset += 4;
        buf[offset..offset+2].copy_from_slice(&self.slippage_bps.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.fee_bps.to_le_bytes()); offset += 2;
        buf[offset..offset+4].copy_from_slice(&self.realized_pnl_cents.to_le_bytes()); offset += 4;
        
        // Remaining bytes = padding (192 - 176 = 16 bytes)
        // Padding bytes stay zero
        
        buf
    }
    
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
            Self::SIDE_BUY => "BUY",
            Self::SIDE_SELL => "SELL",
            _ => "UNKNOWN",
        }
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
    
    /// Check if alpha wallets are active
    pub fn has_alpha_activity(&self) -> bool {
        self.alpha_hits_delta > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tx_confirmed_context_serialization() {
        let sig = [1u8; 64];
        let mint = [2u8; 32];
        let trade_id = [3u8; 16];
        
        let msg = TxConfirmedContext::new(
            sig,
            mint,
            trade_id,
            TxConfirmedContext::SIDE_BUY,
            TxConfirmedContext::STATUS_SUCCESS,
            12345678,
            // Δ-window data
            200,  // trail_ms
            5,    // same_slot_after
            3,    // next_slot_count
            8,    // uniq_buyers_delta
            2.5,  // vol_buy_sol_delta
            1.2,  // vol_sell_sol_delta
            150,  // price_change_bps_delta (+1.5%)
            2,    // alpha_hits_delta
            // Entry data
            1_000_000,  // entry_price_lamports
            0.5,        // size_sol
            150,        // slippage_bps
            30,         // fee_bps
            1.25,       // realized_pnl_usd
        );
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), TxConfirmedContext::SIZE);
        assert_eq!(bytes[0], TxConfirmedContext::MSG_TYPE);
        
        let parsed = TxConfirmedContext::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.signature, sig);
        assert_eq!(parsed.mint, mint);
        assert_eq!(parsed.trade_id, trade_id);
        assert_eq!(parsed.side, TxConfirmedContext::SIDE_BUY);
        assert_eq!(parsed.status, TxConfirmedContext::STATUS_SUCCESS);
        assert_eq!(parsed.slot, 12345678);
        assert_eq!(parsed.trail_ms, 200);
        assert_eq!(parsed.same_slot_after, 5);
        assert_eq!(parsed.uniq_buyers_delta, 8);
        assert_eq!(parsed.alpha_hits_delta, 2);
        
        // Test scaled values
        assert!((parsed.vol_buy_sol() - 2.5).abs() < 0.001);
        assert!((parsed.vol_sell_sol() - 1.2).abs() < 0.001);
        assert!((parsed.size_sol() - 0.5).abs() < 0.001);
        assert!((parsed.realized_pnl_usd() - 1.25).abs() < 0.01);
        
        // Test helper methods
        assert!(parsed.is_profit_target_hit());
        assert!(parsed.is_momentum_building());
        assert!(!parsed.is_fading());
        assert!(parsed.has_alpha_activity());
    }
    
    #[test]
    fn test_message_size() {
        // Verify message is exactly 192 bytes
        assert_eq!(TxConfirmedContext::SIZE, 192);
        
        let msg = TxConfirmedContext::new(
            [0u8; 64], [0u8; 32], [0u8; 16],
            0, 0, 0,
            0, 0, 0, 0, 0.0, 0.0, 0, 0,
            0, 0.0, 0, 0, 0.0,
        );
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), 192);
    }
}
