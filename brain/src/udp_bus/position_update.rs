//! 📡 PositionUpdate Message - Mempool-watcher → Brain (Port 45131)
//!
//! Sent when mempool detects significant position value changes.
//! Brain uses this data to make exit decisions based on P&L and strategy rules.
//!
//! MSG_TYPE = 32 (new)
//! Frequency: Sent on every confirmed trade for this mint OR every 5 seconds if price moved >5%

use anyhow::{Result, Context};

#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct PositionUpdate {
    pub msg_type: u8,              // 32
    pub mint: [u8; 32],            // Token mint address
    pub trade_id: [u8; 16],        // Original trade ID from entry
    pub timestamp: u64,            // Unix timestamp (seconds)
    
    // ===== Position Metadata =====
    pub entry_price_lamports: u64, // Original entry price (from WatchSigEnhanced)
    pub current_price_lamports: u64, // Current market price
    pub entry_size_sol: f32,       // Original position size in SOL
    pub current_value_sol: f32,    // Current position value in SOL
    
    // ===== P&L Calculations =====
    pub realized_pnl_usd: f32,     // Profit/loss in USD
    pub pnl_percent: f32,          // Percentage gain/loss
    
    // ===== Market Context =====
    pub mempool_pending_buys: u16, // Current pending buy txs
    pub mempool_pending_sells: u16, // Current pending sell txs
    pub price_velocity: f32,       // Price change in last 10s (%)
    
    // ===== Exit Signals (Brain decides, but mempool provides data) =====
    pub profit_target_hit: u8,     // 0=no, 1=yes (from WatchSigEnhanced target)
    pub stop_loss_hit: u8,         // 0=no, 1=yes (from WatchSigEnhanced stop)
    pub no_mempool_activity: u8,   // 0=no, 1=yes (no buys in last 15s)
    
    pub _padding: [u8; 7],         // Padding for alignment
}

impl PositionUpdate {
    pub const MSG_TYPE: u8 = 32;
    pub const SIZE: usize = 160;
    
    /// Create new PositionUpdate
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mint: [u8; 32],
        trade_id: [u8; 16],
        entry_price_lamports: u64,
        current_price_lamports: u64,
        entry_size_sol: f32,
        current_value_sol: f32,
        realized_pnl_usd: f32,
        pnl_percent: f32,
        mempool_pending_buys: u16,
        mempool_pending_sells: u16,
        price_velocity: f32,
        profit_target_hit: bool,
        stop_loss_hit: bool,
        no_mempool_activity: bool,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            trade_id,
            timestamp,
            entry_price_lamports,
            current_price_lamports,
            entry_size_sol,
            current_value_sol,
            realized_pnl_usd,
            pnl_percent,
            mempool_pending_buys,
            mempool_pending_sells,
            price_velocity,
            profit_target_hit: if profit_target_hit { 1 } else { 0 },
            stop_loss_hit: if stop_loss_hit { 1 } else { 0 },
            no_mempool_activity: if no_mempool_activity { 1 } else { 0 },
            _padding: [0u8; 7],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        let mut offset = 0;
        
        buf[offset] = self.msg_type; offset += 1;
        buf[offset..offset+32].copy_from_slice(&self.mint); offset += 32;
        buf[offset..offset+16].copy_from_slice(&self.trade_id); offset += 16;
        buf[offset..offset+8].copy_from_slice(&self.timestamp.to_le_bytes()); offset += 8;
        
        buf[offset..offset+8].copy_from_slice(&self.entry_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+8].copy_from_slice(&self.current_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+4].copy_from_slice(&self.entry_size_sol.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.current_value_sol.to_le_bytes()); offset += 4;
        
        buf[offset..offset+4].copy_from_slice(&self.realized_pnl_usd.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.pnl_percent.to_le_bytes()); offset += 4;
        
        buf[offset..offset+2].copy_from_slice(&self.mempool_pending_buys.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.mempool_pending_sells.to_le_bytes()); offset += 2;
        buf[offset..offset+4].copy_from_slice(&self.price_velocity.to_le_bytes()); offset += 4;
        
        buf[offset] = self.profit_target_hit; offset += 1;
        buf[offset] = self.stop_loss_hit; offset += 1;
        buf[offset] = self.no_mempool_activity; offset += 1;
        
        buf
    }
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("PositionUpdate requires {} bytes, got {}", Self::SIZE, data.len());
        }
        
        let mut offset = 0;
        
        let msg_type = data[offset]; offset += 1;
        if msg_type != Self::MSG_TYPE {
            anyhow::bail!("Invalid msg_type: expected {}, got {}", Self::MSG_TYPE, msg_type);
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[offset..offset+32]); offset += 32;
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[offset..offset+16]); offset += 16;
        
        let timestamp = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid timestamp")?
        ); offset += 8;
        
        let entry_price_lamports = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid entry_price_lamports")?
        ); offset += 8;
        
        let current_price_lamports = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid current_price_lamports")?
        ); offset += 8;
        
        let entry_size_sol = f32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid entry_size_sol")?
        ); offset += 4;
        
        let current_value_sol = f32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid current_value_sol")?
        ); offset += 4;
        
        let realized_pnl_usd = f32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid realized_pnl_usd")?
        ); offset += 4;
        
        let pnl_percent = f32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid pnl_percent")?
        ); offset += 4;
        
        let mempool_pending_buys = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid mempool_pending_buys")?
        ); offset += 2;
        
        let mempool_pending_sells = u16::from_le_bytes(
            data[offset..offset+2].try_into().context("Invalid mempool_pending_sells")?
        ); offset += 2;
        
        let price_velocity = f32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid price_velocity")?
        ); offset += 4;
        
        let profit_target_hit = data[offset]; offset += 1;
        let stop_loss_hit = data[offset]; offset += 1;
        let no_mempool_activity = data[offset];
        
        Ok(Self {
            msg_type,
            mint,
            trade_id,
            timestamp,
            entry_price_lamports,
            current_price_lamports,
            entry_size_sol,
            current_value_sol,
            realized_pnl_usd,
            pnl_percent,
            mempool_pending_buys,
            mempool_pending_sells,
            price_velocity,
            profit_target_hit,
            stop_loss_hit,
            no_mempool_activity,
            _padding: [0u8; 7],
        })
    }
    
    /// Get mint as base58 string
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
    
    /// Get trade_id as hex string
    pub fn trade_id_str(&self) -> String {
        hex::encode(&self.trade_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_position_update_serialization() {
        let mint = [1u8; 32];
        let trade_id = [2u8; 16];
        
        let update = PositionUpdate::new(
            mint,
            trade_id,
            1_000_000,      // entry price
            1_500_000,      // current price (+50%)
            0.5,            // 0.5 SOL position
            0.75,           // now worth 0.75 SOL
            75.0,           // $75 profit
            50.0,           // 50% gain
            10,             // 10 pending buys
            2,              // 2 pending sells
            5.5,            // 5.5% price velocity
            true,           // profit target hit
            false,          // stop loss not hit
            false,          // mempool has activity
        );
        
        let bytes = update.to_bytes();
        assert_eq!(bytes.len(), PositionUpdate::SIZE);
        assert_eq!(bytes[0], PositionUpdate::MSG_TYPE);
        
        let parsed = PositionUpdate::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.entry_price_lamports, 1_000_000);
        assert_eq!(parsed.current_price_lamports, 1_500_000);
        assert_eq!(parsed.profit_target_hit, 1);
        assert_eq!(parsed.stop_loss_hit, 0);
    }
}
