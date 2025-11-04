//! 🔍 Signature Tracker - Track Submitted Transactions
//!
//! Brain tracks transaction signatures for positions to detect confirmations.
//! This module provides:
//! 1. Signature → Position mapping (mint, trade_id, side, entry_price)
//! 2. RPC polling backup (2-second interval) for missed WebSocket events
//! 3. Stale signature cleanup (>90s without confirmation)
//!
//! Replaces dependency on mempool-watcher for confirmation tracking.

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

/// Transaction metadata for confirmation tracking
#[derive(Debug, Clone)]
pub struct TrackedTransaction {
    pub signature: [u8; 64],
    pub mint: [u8; 32],
    pub trade_id: String,
    pub side: u8, // 0=BUY, 1=SELL
    pub entry_price: f64,
    pub size_sol: f64,
    pub timestamp_ns: u64,
}

impl TrackedTransaction {
    pub const SIDE_BUY: u8 = 0;
    pub const SIDE_SELL: u8 = 1;

    /// Create new tracked transaction
    pub fn new(
        signature: [u8; 64],
        mint: [u8; 32],
        trade_id: String,
        side: u8,
        entry_price: f64,
        size_sol: f64,
    ) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        Self {
            signature,
            mint,
            trade_id,
            side,
            entry_price,
            size_sol,
            timestamp_ns,
        }
    }

    /// Get signature as base58 string
    pub fn signature_str(&self) -> String {
        bs58::encode(&self.signature).into_string()
    }

    /// Get mint as base58 string
    pub fn mint_str(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }

    /// Get side as string
    pub fn side_str(&self) -> &str {
        match self.side {
            Self::SIDE_BUY => "BUY",
            Self::SIDE_SELL => "SELL",
            _ => "UNKNOWN",
        }
    }

    /// Get age in seconds
    pub fn age_secs(&self) -> u64 {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        now_ns.saturating_sub(self.timestamp_ns) / 1_000_000_000
    }
}

/// Signature tracker for Brain's transaction monitoring
pub struct SignatureTracker {
    tracked: Arc<RwLock<HashMap<String, TrackedTransaction>>>,
}

impl SignatureTracker {
    /// Create new signature tracker
    pub fn new() -> Self {
        Self {
            tracked: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add transaction to tracking
    pub async fn track(&self, tx: TrackedTransaction) {
        let sig_str = tx.signature_str();
        let mut tracked = self.tracked.write().await;
        debug!(
            "📝 Tracking {} {} {:.3} SOL | mint: {} | age: {}s",
            tx.side_str(),
            &sig_str[..12],
            tx.size_sol,
            &tx.mint_str()[..12],
            tx.age_secs()
        );
        tracked.insert(sig_str, tx);
    }

    /// Check if signature is being tracked
    pub async fn is_tracked(&self, signature: &str) -> bool {
        let tracked = self.tracked.read().await;
        tracked.contains_key(signature)
    }

    /// Get tracked transaction
    pub async fn get(&self, signature: &str) -> Option<TrackedTransaction> {
        let tracked = self.tracked.read().await;
        tracked.get(signature).cloned()
    }

    /// Remove transaction from tracking (called on confirmation)
    pub async fn remove(&self, signature: &str) -> Option<TrackedTransaction> {
        let mut tracked = self.tracked.write().await;
        let result = tracked.remove(signature);
        if result.is_some() {
            debug!(
                "✅ Removed signature from tracking: {} (remaining: {})",
                &signature[..12],
                tracked.len()
            );
        }
        result
    }

    /// Get count of tracked signatures
    pub async fn count(&self) -> usize {
        let tracked = self.tracked.read().await;
        tracked.len()
    }

    /// Get all tracked signature strings (for RPC polling)
    pub async fn get_all_signatures(&self) -> Vec<String> {
        let tracked = self.tracked.read().await;
        tracked.keys().cloned().collect()
    }

    /// Get all tracked transactions (for debugging)
    pub async fn get_all(&self) -> Vec<TrackedTransaction> {
        let tracked = self.tracked.read().await;
        tracked.values().cloned().collect()
    }

    /// Clean up stale signatures (>max_age_secs without confirmation)
    pub async fn cleanup_stale(&self, max_age_secs: u64) -> usize {
        let mut tracked = self.tracked.write().await;
        let before_count = tracked.len();

        tracked.retain(|sig, tx| {
            let age_secs = tx.age_secs();
            if age_secs > max_age_secs {
                warn!(
                    "⏰ Signature STALE ({}s): {} {} | mint: {}",
                    age_secs,
                    tx.side_str(),
                    &sig[..12],
                    &tx.mint_str()[..12]
                );
                false
            } else {
                true
            }
        });

        let removed_count = before_count - tracked.len();
        if removed_count > 0 {
            info!("🗑️  Cleaned up {} stale signatures", removed_count);
        }
        removed_count
    }
}

/// Confirmation status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmationStatus {
    Success,
    Failed,
}

impl ConfirmationStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, ConfirmationStatus::Success)
    }

    pub fn as_str(&self) -> &str {
        match self {
            ConfirmationStatus::Success => "SUCCESS",
            ConfirmationStatus::Failed => "FAILED",
        }
    }
}

/// Confirmation event
#[derive(Debug, Clone)]
pub struct ConfirmationEvent {
    pub signature: String,
    pub mint: String,
    pub trade_id: String,
    pub side: u8,
    pub status: ConfirmationStatus,
    pub entry_price: f64,
    pub size_sol: f64,
}

impl ConfirmationEvent {
    /// Get side as string
    pub fn side_str(&self) -> &str {
        match self.side {
            0 => "BUY",
            1 => "SELL",
            _ => "UNKNOWN",
        }
    }
}

/// RPC poller for signature confirmations (backup for gRPC)
pub struct RpcPoller {
    tracker: Arc<SignatureTracker>,
    rpc_url: String,
    poll_interval_secs: u64,
}

impl RpcPoller {
    /// Create new RPC poller
    pub fn new(tracker: Arc<SignatureTracker>, rpc_url: String, poll_interval_secs: u64) -> Self {
        Self {
            tracker,
            rpc_url,
            poll_interval_secs,
        }
    }

    /// Start RPC polling task
    pub async fn start(
        self: Arc<Self>,
        confirmation_handler: impl Fn(ConfirmationEvent) -> Result<()> + Send + Sync + 'static,
    ) {
        info!(
            "🔄 RPC signature polling started (interval: {}s, endpoint: {})",
            self.poll_interval_secs, self.rpc_url
        );

        let handler = Arc::new(confirmation_handler);
        let mut tick = interval(Duration::from_secs(self.poll_interval_secs));

        loop {
            tick.tick().await;

            let signatures = self.tracker.get_all_signatures().await;
            if signatures.is_empty() {
                continue;
            }

            debug!("🔍 RPC polling {} signatures", signatures.len());

            // Parse signatures
            let mut sig_objects = Vec::new();
            for sig_str in &signatures {
                match Signature::from_str(sig_str) {
                    Ok(sig) => sig_objects.push(sig),
                    Err(e) => {
                        warn!(
                            "⚠️  Invalid signature format: {} - {}",
                            &sig_str[..12],
                            e
                        );
                        continue;
                    }
                }
            }

            if sig_objects.is_empty() {
                continue;
            }

            // Create RPC client
            let rpc_client = RpcClient::new(&self.rpc_url);

            // Batch query signature statuses
            match rpc_client.get_signature_statuses(&sig_objects) {
                Ok(response) => {
                    for (idx, status_opt) in response.value.iter().enumerate() {
                        if let Some(status) = status_opt {
                            // Check if confirmed or finalized
                            if status.confirmation_status.is_some() {
                                let sig_str = &signatures[idx];

                                info!(
                                    "✅ RPC POLL: Signature {} confirmed",
                                    &sig_str[..12]
                                );

                                // Remove from tracker
                                if let Some(tx) = self.tracker.remove(sig_str).await {
                                    let confirmation_status = if status.err.is_some() {
                                        ConfirmationStatus::Failed
                                    } else {
                                        ConfirmationStatus::Success
                                    };

                                    let event = ConfirmationEvent {
                                        signature: sig_str.clone(),
                                        mint: tx.mint_str(),
                                        trade_id: tx.trade_id.clone(),
                                        side: tx.side,
                                        status: confirmation_status,
                                        entry_price: tx.entry_price,
                                        size_sol: tx.size_sol,
                                    };

                                    if let Err(e) = handler(event) {
                                        error!(
                                            "❌ Error handling RPC confirmation: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("⚠️  RPC signature status query failed: {}", e);
                }
            }

            // Cleanup stale signatures (>90s)
            self.tracker.cleanup_stale(90).await;
        }
    }
}
