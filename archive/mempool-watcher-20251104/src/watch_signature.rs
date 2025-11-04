//! 📡 WatchSignature Message - Executor → Mempool-watcher (Port 45130)
//!
//! Executor sends this after submitting a transaction to register it for confirmation tracking.
//! Mempool-watcher stores the signature and monitors Yellowstone/WebSocket for confirmation.

use anyhow::{Result, Context};

/// WatchSignature message from Executor
/// 
/// MSG_TYPE = 25
/// SIZE = 128 bytes
#[derive(Debug, Clone)]
pub struct WatchSignature {
    pub msg_type: u8,
    pub signature: [u8; 64],
    pub mint: [u8; 32],
    pub trade_id: [u8; 16],
    pub side: u8,  // 0=BUY, 1=SELL
    pub timestamp_ns: u64,
}

impl WatchSignature {
    pub const MSG_TYPE: u8 = 25;
    pub const SIZE: usize = 128;
    
    /// Parse WatchSignature from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < Self::SIZE {
            anyhow::bail!("WatchSignature requires {} bytes, got {}", Self::SIZE, data.len());
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
        
        let timestamp_ns = u64::from_le_bytes(
            data[114..122].try_into().context("Invalid timestamp_ns")?
        );
        
        Ok(Self {
            msg_type,
            signature,
            mint,
            trade_id,
            side,
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
}

/// Signature tracker - stores watched signatures awaiting confirmation
pub struct SignatureTracker {
    watched: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, WatchSignature>>>,
}

impl SignatureTracker {
    pub fn new() -> Self {
        Self {
            watched: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// Add signature to watch list
    pub async fn add(&self, watch: WatchSignature) {
        let sig_str = watch.signature_str();
        let mut watched = self.watched.write().await;
        watched.insert(sig_str.clone(), watch);
        log::debug!("📝 Added signature to watch list: {} (total: {})", &sig_str[..12], watched.len());
    }
    
    /// Check if signature is being watched
    pub async fn is_watched(&self, signature: &str) -> bool {
        let watched = self.watched.read().await;
        watched.contains_key(signature)
    }
    
    /// Remove signature from watch list and return its data
    pub async fn remove(&self, signature: &str) -> Option<WatchSignature> {
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
