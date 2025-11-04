// ============================================================================
// CONFIRMATION TASK - Background polling for TPU transaction confirmations
// ============================================================================
// Polls getSignatureStatuses with exponential backoff to detect confirmations
// Sends TradeConfirmed or TradeFailed messages to brain via UDP

use log::{debug, info, warn, error};
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use crate::advice_bus::Advisory;

/// Pending transaction awaiting confirmation
#[derive(Debug, Clone)]
pub struct PendingTx {
    pub mint: [u8; 32],
    pub signature: Signature,
    pub side: u8, // 0=BUY, 1=SELL
    pub expected_tokens: u64,
    pub expected_sol_lamports: u64,
    pub submitted_at: Instant,
    pub poll_count: u32,
}

/// Confirmation task state
pub struct ConfirmationTask {
    rpc_client: Arc<RpcClient>,
    brain_socket: Arc<std::net::UdpSocket>,
    pending_txs: Arc<RwLock<HashMap<Signature, PendingTx>>>,
    poll_intervals: Vec<u64>, // Milliseconds: [100, 200, 400, 800]
    max_wait_ms: u64, // 1200ms
}

impl ConfirmationTask {
    pub fn new(
        rpc_client: Arc<RpcClient>,
        brain_socket: Arc<std::net::UdpSocket>,
        poll_intervals: Vec<u64>,
        max_wait_ms: u64,
    ) -> Self {
        Self {
            rpc_client,
            brain_socket,
            pending_txs: Arc::new(RwLock::new(HashMap::new())),
            poll_intervals,
            max_wait_ms,
        }
    }

    /// Add a transaction to track for confirmation
    pub async fn track_transaction(
        &self,
        mint: [u8; 32],
        signature: Signature,
        side: u8,
        expected_tokens: u64,
        expected_sol_lamports: u64,
    ) {
        let pending = PendingTx {
            mint,
            signature,
            side,
            expected_tokens,
            expected_sol_lamports,
            submitted_at: Instant::now(),
            poll_count: 0,
        };
        
        debug!("📌 Tracking tx: {} ({})", 
               signature, 
               if side == 0 { "BUY" } else { "SELL" });
        
        self.pending_txs.write().await.insert(signature, pending);
    }

    /// Background task that polls for confirmations
    pub async fn run(self: Arc<Self>) {
        info!("✅ Confirmation task started (exponential backoff: {:?}ms, max: {}ms)", 
              self.poll_intervals, self.max_wait_ms);
        
        loop {
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            let mut pending = self.pending_txs.write().await;
            let mut to_remove = Vec::new();
            
            for (sig, tx) in pending.iter_mut() {
                let elapsed_ms = tx.submitted_at.elapsed().as_millis() as u64;
                
                // Check for timeout
                if elapsed_ms > self.max_wait_ms {
                    warn!("⏱️  Tx timeout: {} ({}ms > {}ms)", 
                          sig, elapsed_ms, self.max_wait_ms);
                    
                    self.send_trade_failed(
                        &tx.mint,
                        sig,
                        tx.side,
                        1, // reason_code: TIMEOUT
                        true, // has_signature
                        "Confirmation timeout",
                    );
                    
                    to_remove.push(*sig);
                    continue;
                }
                
                // Exponential backoff: only poll at specific intervals
                let next_poll_ms = if tx.poll_count < self.poll_intervals.len() as u32 {
                    self.poll_intervals[tx.poll_count as usize]
                } else {
                    *self.poll_intervals.last().unwrap()
                };
                
                // Check if it's time to poll again
                let time_since_last_poll = elapsed_ms - 
                    self.poll_intervals.iter()
                        .take(tx.poll_count as usize)
                        .sum::<u64>();
                
                if time_since_last_poll < next_poll_ms {
                    continue; // Not time to poll yet
                }
                
                tx.poll_count += 1;
                
                // Poll RPC for signature status
                debug!("🔍 Polling tx (attempt {}): {}", tx.poll_count, sig);
                
                match self.rpc_client.get_signature_statuses(&[*sig]) {
                    Ok(response) => {
                        if let Some(Some(status)) = response.value.first() {
                            if status.err.is_some() {
                                // Transaction failed
                                error!("❌ Tx failed on-chain: {} - {:?}", sig, status.err);
                                
                                self.send_trade_failed(
                                    &tx.mint,
                                    sig,
                                    tx.side,
                                    2, // reason_code: ON_CHAIN_ERROR
                                    true,
                                    &format!("{:?}", status.err),
                                );
                                
                                to_remove.push(*sig);
                            } else if status.confirmation_status.is_some() {
                                // Transaction confirmed!
                                info!("✅ Tx confirmed: {} ({}ms, {} polls)", 
                                      sig, elapsed_ms, tx.poll_count);
                                
                                self.send_trade_confirmed(
                                    &tx.mint,
                                    sig,
                                    tx.side,
                                    tx.expected_tokens, // TODO: Get actual from transaction
                                    tx.expected_sol_lamports,
                                    0, // total_fees_lamports
                                    status.slot,
                                    elapsed_ms < 500, // fast_confirm if under 500ms
                                );
                                
                                to_remove.push(*sig);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("⚠️  RPC error polling {}: {}", sig, e);
                        // Continue trying on RPC errors
                    }
                }
            }
            
            // Remove confirmed/failed/timed-out transactions
            for sig in to_remove {
                pending.remove(&sig);
            }
        }
    }

    /// Send TradeConfirmed message to brain
    fn send_trade_confirmed(
        &self,
        mint: &[u8; 32],
        signature: &Signature,
        side: u8,
        actual_tokens: u64,
        actual_sol_lamports: u64,
        total_fees_lamports: u64,
        slot: u64,
        fast_confirm: bool,
    ) {
        // Convert Signature to bytes
        let sig_bytes = signature.as_ref();
        let mut signature_array = [0u8; 64];
        signature_array[..sig_bytes.len()].copy_from_slice(sig_bytes);
        
        let advisory = Advisory::TradeConfirmed {
            mint: *mint,
            signature: signature_array,
            side,
            actual_tokens,
            actual_sol: actual_sol_lamports,
            total_fees: total_fees_lamports,
            compute_units: 0, // Not available from status
            fast_confirm: fast_confirm as u8,
            tx_status: 1, // SUCCESS
            confirmed_ts_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 6],
        };
        
        let bytes = advisory.to_bytes();
        if let Err(e) = self.brain_socket.send_to(&bytes, "127.0.0.1:45111") {
            error!("Failed to send TradeConfirmed: {}", e);
        } else {
            debug!("📤 Sent TradeConfirmed: {} {} (slot: {})", 
                   bs58::encode(mint).into_string()[..12].to_string(),
                   if side == 0 { "BUY" } else { "SELL" },
                   slot);
        }
    }

    /// Send TradeFailed message to brain
    fn send_trade_failed(
        &self,
        mint: &[u8; 32],
        signature: &Signature,
        side: u8,
        reason_code: u8,
        has_signature: bool,
        reason_str: &str,
    ) {
        // Convert Signature to bytes
        let sig_bytes = signature.as_ref();
        let mut signature_array = [0u8; 64];
        signature_array[..sig_bytes.len()].copy_from_slice(sig_bytes);
        
        // Convert reason string to bytes
        let mut reason_bytes = [0u8; 64];
        let reason_truncated = &reason_str.as_bytes()[..reason_str.len().min(64)];
        reason_bytes[..reason_truncated.len()].copy_from_slice(reason_truncated);
        
        let advisory = Advisory::TradeFailed {
            mint: *mint,
            signature: signature_array,
            side,
            reason_code,
            has_signature: has_signature as u8,
            reason_str: reason_bytes,
            failed_ts_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 6],
        };
        
        let bytes = advisory.to_bytes();
        if let Err(e) = self.brain_socket.send_to(&bytes, "127.0.0.1:45111") {
            error!("Failed to send TradeFailed: {}", e);
        } else {
            debug!("📤 Sent TradeFailed: {} {} - {}", 
                   bs58::encode(mint).into_string()[..12].to_string(),
                   if side == 0 { "BUY" } else { "SELL" },
                   reason_str);
        }
    }
}
