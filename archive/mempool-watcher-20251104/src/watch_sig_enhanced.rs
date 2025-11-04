//! 📡 WatchSigEnhanced Message - Enhanced Executor → Mempool-watcher
//!
//! Enhanced version of WatchSignature that includes full trade metadata for profit calculation.
//! The Watcher stores this data and uses it to compute realized P&L when confirmation arrives.

use anyhow::{Result, Context};

/// WatchSigEnhanced message from Executor
/// 
/// MSG_TYPE = 28 (new type to differentiate from basic WatchSignature)
/// SIZE = 192 bytes (expanded from 128 to include trade metadata)
/// 
/// Sent from Executor to Mempool-watcher (Port 45130) immediately after tx submission.
/// This enhanced version includes all data needed for the Watcher to:
/// 1. Track confirmation status
/// 2. Calculate realized P&L when confirmed
/// 3. Generate ExitAdvice if profit target hit
#[derive(Debug, Clone)]
pub struct WatchSigEnhanced {
    // ===== Core Identification =====
    pub msg_type: u8,
    pub signature: [u8; 64],
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub side: u8,  // 0=BUY, 1=SELL
    pub timestamp_ns: u64,
    
    // ===== Trade Metadata (for profit calculation) =====
    /// Entry price in lamports per token
    pub entry_price_lamports: u64,
    
    /// Position size in SOL (scaled by 1000, e.g., 0.5 SOL = 500)
    pub size_sol_scaled: u32,
    
    /// Slippage tolerance in basis points (e.g., 150 = 1.5%)
    pub slippage_bps: u16,
    
    /// Fee in basis points (e.g., 30 = 0.3%)
    pub fee_bps: u16,
    
    /// Profit target in USD cents (e.g., 100 = $1.00)
    pub profit_target_cents: u32,
    
    /// Stop-loss in USD cents (negative, e.g., -50 = -$0.50)
    pub stop_loss_cents: i32,
}

impl WatchSigEnhanced {
    pub const MSG_TYPE: u8 = 28;
    pub const SIZE: usize = 192;
    
    pub const SIDE_BUY: u8 = 0;
    pub const SIDE_SELL: u8 = 1;
    
    /// Create new WatchSigEnhanced message
    pub fn new(
        signature: [u8; 64],
        mint: [u8; 32],
        trade_id: [u8; 16],
        side: u8,
        entry_price_lamports: u64,
        size_sol: f64,
        slippage_bps: u16,
        fee_bps: u16,
        profit_target_usd: f64,
        stop_loss_usd: f64,
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
            timestamp_ns,
            entry_price_lamports,
            size_sol_scaled: (size_sol * 1000.0) as u32,
            slippage_bps,
            fee_bps,
            profit_target_cents: (profit_target_usd * 100.0) as u32,
            stop_loss_cents: (stop_loss_usd * 100.0) as i32,
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![0u8; Self::SIZE];
        let mut offset = 0;
        
        // Core identification (123 bytes)
        buf[offset] = self.msg_type; offset += 1;
        buf[offset..offset+64].copy_from_slice(&self.signature); offset += 64;
        buf[offset..offset+32].copy_from_slice(&self.mint); offset += 32;
        buf[offset..offset+16].copy_from_slice(&self.trade_id); offset += 16;
        buf[offset] = self.side; offset += 1;
        buf[offset..offset+8].copy_from_slice(&self.timestamp_ns.to_le_bytes()); offset += 8;
        
        // Trade metadata (26 bytes)
        buf[offset..offset+8].copy_from_slice(&self.entry_price_lamports.to_le_bytes()); offset += 8;
        buf[offset..offset+4].copy_from_slice(&self.size_sol_scaled.to_le_bytes()); offset += 4;
        buf[offset..offset+2].copy_from_slice(&self.slippage_bps.to_le_bytes()); offset += 2;
        buf[offset..offset+2].copy_from_slice(&self.fee_bps.to_le_bytes()); offset += 2;
        buf[offset..offset+4].copy_from_slice(&self.profit_target_cents.to_le_bytes()); offset += 4;
        buf[offset..offset+4].copy_from_slice(&self.stop_loss_cents.to_le_bytes()); offset += 4;
        
        // Remaining bytes = padding (192 - 149 = 43 bytes)
        
        buf
    }
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("WatchSigEnhanced requires {} bytes, got {}", Self::SIZE, data.len());
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
        
        let timestamp_ns = u64::from_le_bytes(
            data[offset..offset+8].try_into().context("Invalid timestamp_ns")?
        ); offset += 8;
        
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
        
        let profit_target_cents = u32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid profit_target_cents")?
        ); offset += 4;
        
        let stop_loss_cents = i32::from_le_bytes(
            data[offset..offset+4].try_into().context("Invalid stop_loss_cents")?
        );
        
        Ok(Self {
            msg_type,
            signature,
            mint,
            trade_id,
            side,
            timestamp_ns,
            entry_price_lamports,
            size_sol_scaled,
            slippage_bps,
            fee_bps,
            profit_target_cents,
            stop_loss_cents,
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
    
    /// Get side as string
    pub fn side_str(&self) -> &str {
        match self.side {
            Self::SIDE_BUY => "BUY",
            Self::SIDE_SELL => "SELL",
            _ => "UNKNOWN",
        }
    }
    
    /// Get position size in SOL (unscaled)
    pub fn size_sol(&self) -> f64 {
        self.size_sol_scaled as f64 / 1000.0
    }
    
    /// Get profit target in USD (unscaled)
    pub fn profit_target_usd(&self) -> f64 {
        self.profit_target_cents as f64 / 100.0
    }
    
    /// Get stop-loss in USD (unscaled)
    pub fn stop_loss_usd(&self) -> f64 {
        self.stop_loss_cents as f64 / 100.0
    }
}

/// Enhanced signature tracker - stores WatchSigEnhanced with trade metadata
pub struct SignatureTrackerEnhanced {
    watched: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, WatchSigEnhanced>>>,
}

impl SignatureTrackerEnhanced {
    pub fn new() -> Self {
        Self {
            watched: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// Add signature with trade metadata to watch list
    pub async fn add(&self, watch: WatchSigEnhanced) {
        let sig_str = watch.signature_str();
        let mut watched = self.watched.write().await;
        log::debug!(
            "📝 Added enhanced watch: {} {} {:.3} SOL @ {} lamports (target: ${:.2}, stop: ${:.2})",
            watch.side_str(),
            &sig_str[..12],
            watch.size_sol(),
            watch.entry_price_lamports,
            watch.profit_target_usd(),
            watch.stop_loss_usd()
        );
        watched.insert(sig_str, watch);
    }
    
    /// Check if signature is being watched
    pub async fn is_watched(&self, signature: &str) -> bool {
        let watched = self.watched.read().await;
        watched.contains_key(signature)
    }
    
    /// Get watched signature data
    pub async fn get(&self, signature: &str) -> Option<WatchSigEnhanced> {
        let watched = self.watched.read().await;
        watched.get(signature).cloned()
    }
    
    /// Remove signature from watch list and return its data
    pub async fn remove(&self, signature: &str) -> Option<WatchSigEnhanced> {
        let mut watched = self.watched.write().await;
        let result = watched.remove(signature);
        if result.is_some() {
            log::debug!("✅ Removed signature from watch list: {} (remaining: {})", 
                       &signature[..12], watched.len());
        }
        result
    }
    
    /// Get count of watched signatures
    pub async fn count(&self) -> usize {
        let watched = self.watched.read().await;
        watched.len()
    }
    
    /// Get all watched signature strings (for RPC polling)
    pub async fn get_all_signatures(&self) -> Vec<String> {
        let watched = self.watched.read().await;
        watched.keys().cloned().collect()
    }
    
    /// Clean up old signatures (>60s without confirmation)
    pub async fn cleanup_stale(&self, max_age_secs: u64) {
        let mut watched = self.watched.write().await;
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        let before_count = watched.len();
        watched.retain(|sig, watch| {
            let age_ns = now_ns.saturating_sub(watch.timestamp_ns);
            let age_secs = age_ns / 1_000_000_000;
            if age_secs > max_age_secs {
                log::warn!("⏰ Removing stale signature: {} (age: {}s)", &sig[..12], age_secs);
                false
            } else {
                true
            }
        });
        
        let removed = before_count - watched.len();
        if removed > 0 {
            log::info!("🧹 Cleaned up {} stale signatures", removed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_watch_sig_enhanced_serialization() {
        let sig = [1u8; 64];
        let mint = [2u8; 32];
        let trade_id = [3u8; 16];
        
        let msg = WatchSigEnhanced::new(
            sig,
            mint,
            trade_id,
            WatchSigEnhanced::SIDE_BUY,
            1_000_000,  // entry_price_lamports
            0.5,        // size_sol
            150,        // slippage_bps
            30,         // fee_bps
            1.00,       // profit_target_usd
            -0.50,      // stop_loss_usd
        );
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), WatchSigEnhanced::SIZE);
        assert_eq!(bytes[0], WatchSigEnhanced::MSG_TYPE);
        
        let parsed = WatchSigEnhanced::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.signature, sig);
        assert_eq!(parsed.mint, mint);
        assert_eq!(parsed.trade_id, trade_id);
        assert_eq!(parsed.side, WatchSigEnhanced::SIDE_BUY);
        assert_eq!(parsed.entry_price_lamports, 1_000_000);
        assert_eq!(parsed.slippage_bps, 150);
        assert_eq!(parsed.fee_bps, 30);
        
        // Test scaled values
        assert!((parsed.size_sol() - 0.5).abs() < 0.001);
        assert!((parsed.profit_target_usd() - 1.00).abs() < 0.01);
        assert!((parsed.stop_loss_usd() - (-0.50)).abs() < 0.01);
    }
    
    #[test]
    fn test_message_size() {
        assert_eq!(WatchSigEnhanced::SIZE, 192);
        
        let msg = WatchSigEnhanced::new(
            [0u8; 64], [0u8; 32], [0u8; 16],
            0, 0, 0.0, 0, 0, 0.0, 0.0,
        );
        
        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), 192);
    }
}
