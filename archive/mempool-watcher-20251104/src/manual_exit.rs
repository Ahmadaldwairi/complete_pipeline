//! 📡 ManualExitNotification - Mempool → Executor/Brain
//!
//! Sent when mempool detects user sold manually (not through executor).
//! Provides realized P&L for telegram notification and position cleanup.

use anyhow::Result;

/// ManualExitNotification message
/// MSG_TYPE = 33 (new)
/// SIZE = 128 bytes
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct ManualExitNotification {
    pub msg_type: u8,              // 33
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // Original trade ID from entry
    pub exit_signature: [u8; 64],  // Manual exit transaction signature
    pub timestamp: u64,            // Exit timestamp (unix seconds)
    
    // P&L Calculations
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
    pub const SIZE: usize = 128 + 64;  // 192 bytes total

    pub fn new(
        mint: [u8; 32],
        trade_id: [u8; 16],
        exit_signature: [u8; 64],
        entry_price_lamports: u64,
        exit_price_lamports: u64,
        size_sol: f32,
        realized_pnl_usd: f32,
        pnl_percent: f32,
        hold_time_secs: u32,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
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
            _padding: [0; 7],
        }
    }

    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        let mut offset = 0;

        buf[offset] = self.msg_type; offset += 1;
        buf[offset..offset+32].copy_from_slice(&self.mint); offset += 32;
        buf[offset..offset+16].copy_from_slice(&self.trade_id); offset += 16;
        buf[offset..offset+64].copy_from_slice(&self.exit_signature); offset += 64;
        buf[offset..offset+8].copy_from_slice(&self.timestamp.to_le_bytes()); offset += 8;
        buf[offset..offset+8].copy_from_slice(&self.entry_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+8].copy_from_slice(&self.exit_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+4].copy_from_slice(&self.size_sol.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.realized_pnl_usd.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.pnl_percent.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.hold_time_secs.to_le_bytes()); offset += 4;

        buf
    }

    /// Deserialize from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("ManualExitNotification requires {} bytes, got {}", Self::SIZE, data.len());
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

        let mut exit_signature = [0u8; 64];
        exit_signature.copy_from_slice(&data[offset..offset+64]); offset += 64;

        let timestamp = u64::from_le_bytes(data[offset..offset+8].try_into()?); offset += 8;
        let entry_price_lamports = u64::from_le_bytes(data[offset..offset+8].try_into()?); offset += 8;
        let exit_price_lamports = u64::from_le_bytes(data[offset..offset+8].try_into()?); offset += 8;
        let size_sol = f32::from_le_bytes(data[offset..offset+4].try_into()?); offset += 4;
        let realized_pnl_usd = f32::from_le_bytes(data[offset..offset+4].try_into()?); offset += 4;
        let pnl_percent = f32::from_le_bytes(data[offset..offset+4].try_into()?); offset += 4;
        let hold_time_secs = u32::from_le_bytes(data[offset..offset+4].try_into()?);

        Ok(Self {
            msg_type,
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
            _padding: [0; 7],
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

    /// Get exit signature as base58 string
    pub fn exit_sig_str(&self) -> String {
        bs58::encode(&self.exit_signature).into_string()
    }
}
