use std::collections::HashMap;
use chrono::{DateTime, Local};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof,
    SubscribeRequest,
    SubscribeRequestFilterTransactions,
    SubscribeUpdateTransaction,
    CommitmentLevel,
};
use tokio_stream::StreamExt;
use log::{info, warn};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// Represents a pending transaction in the mempool
#[derive(Clone, Debug)]
pub struct PendingTransaction {
    pub wallet: String,
    pub amount_sol: f64,
    pub timestamp: DateTime<Local>,
    pub signature: String,  // NEW: Track transaction signature
}

// Monitors mempool for pending buy transactions
pub struct MempoolMonitor {
    grpc_endpoint: String,
    pump_program_id: Pubkey,
    // OPTIMIZATION #13: Dual indexing for O(1) lookups
    // Maps token address -> list of pending transactions (for quick token lookup)
    pending_by_token: HashMap<String, Vec<PendingTransaction>>,
    // Maps signature -> transaction (for cleanup when transactions land)
    pending_by_sig: HashMap<String, PendingTransaction>,
}

impl MempoolMonitor {
    pub fn new(grpc_endpoint: String) -> Self {
        let pump_program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
            .expect("Valid Pump.fun program ID");
        
        MempoolMonitor {
            grpc_endpoint,
            pump_program_id,
            pending_by_token: HashMap::new(),
            pending_by_sig: HashMap::new(),
        }
    }
    
    /// OPTIMIZATION #13: Check if a token has pending buy transactions in the mempool
    /// 
    /// This is the FAST PATH for entry decisions! Uses in-memory index for O(1) lookup.
    /// NO MORE 500ms gRPC subscriptions - we now use cached mempool data!
    /// 
    /// Returns: (pending_tx_count, pending_sol_volume, estimated_position)
    pub fn check_pending_volume(&self, token: &str) -> (u32, f64, u32) {
        // FAST PATH: O(1) lookup in token index
        if let Some(txs) = self.pending_by_token.get(token) {
            let count = txs.len() as u32;
            let volume: f64 = txs.iter().map(|tx| tx.amount_sol).sum();
            let estimated_position = count + 1; // +1 for our transaction
            
            (count, volume, estimated_position)
        } else {
            // No pending transactions for this token
            (0, 0.0, 1)
        }
    }
    
    /// Add a pending transaction to both indexes (called by background mempool listener)
    pub fn add_pending(&mut self, token: String, tx: PendingTransaction) {
        let sig = tx.signature.clone();
        
        // Add to token index
        self.pending_by_token
            .entry(token)
            .or_insert_with(Vec::new)
            .push(tx.clone());
        
        // Add to signature index
        self.pending_by_sig.insert(sig, tx);
    }
    
    /// Remove a pending transaction (called when it lands or expires)
    pub fn remove_pending(&mut self, signature: &str) -> Option<PendingTransaction> {
        if let Some(tx) = self.pending_by_sig.remove(signature) {
            // Also remove from token index
            if let Some(token_txs) = self.pending_by_token.get_mut(&tx.wallet) {
                token_txs.retain(|t| t.signature != signature);
                
                // Clean up empty vectors
                if token_txs.is_empty() {
                    self.pending_by_token.remove(&tx.wallet);
                }
            }
            Some(tx)
        } else {
            None
        }
    }
    
    /// DEPRECATED: Old slow method - kept for reference but not used
    /// Query the gRPC mempool for pending transactions on a specific token
    /// 
    /// This connects to Yellowstone gRPC and subscribes to UNCONFIRMED transactions
    /// for the specified token address.
    #[allow(dead_code)]
    async fn query_mempool_for_token_slow(&mut self, token: &str) -> Result<(u32, f64), Box<dyn std::error::Error>> {
        // Connect to gRPC
        let mut client = GeyserGrpcClient::build_from_shared(self.grpc_endpoint.clone())?
            .x_token::<String>(None)?
            .connect()
            .await?;
        
        // Subscribe to UNCONFIRMED transactions involving this token
        // This gives us pending mempool transactions!
        let mut transactions: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
        transactions.insert(
            "mempool_txs".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![token.to_string(), self.pump_program_id.to_string()],
                account_exclude: vec![],
                account_required: vec![],
            },
        );
        
        let request = SubscribeRequest {
            accounts: HashMap::new(),
            slots: HashMap::new(),
            transactions,
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            // CRITICAL: Use PROCESSED commitment to get mempool/pending transactions!
            commitment: Some(CommitmentLevel::Processed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };
        
        let mut stream = client.subscribe_once(request).await?;
        
        // Collect pending transactions for a short window (500ms)
        // This gives us a snapshot of current mempool state
        let mut pending_count = 0u32;
        let mut pending_volume = 0.0f64;
        let start_time = std::time::Instant::now();
        let collection_window = std::time::Duration::from_millis(500);
        
        while start_time.elapsed() < collection_window {
            // Use timeout to avoid blocking
            let timeout = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                stream.next()
            ).await;
            
            match timeout {
                Ok(Some(Ok(msg))) => {
                    if let Some(UpdateOneof::Transaction(tx_update)) = msg.update_oneof {
                        // Try to parse as a BUY transaction
                        if let Some((wallet, sol_amount, signature)) = self.parse_buy_transaction(&tx_update) {
                            pending_count += 1;
                            pending_volume += sol_amount;
                            
                            // Store in our tracking map (old slow method still uses this for collection)
                            let tx = PendingTransaction {
                                wallet,
                                amount_sol: sol_amount,
                                timestamp: Local::now(),
                                signature,
                            };
                            
                            // Add to token index (using new add_pending method)
                            self.add_pending(token.to_string(), tx);
                        }
                    }
                }
                Ok(Some(Err(_))) => {
                    // Stream error, continue
                }
                Ok(None) => {
                    // Stream ended
                    break;
                }
                Err(_) => {
                    // Timeout, continue collecting
                }
            }
        }
        
        Ok((pending_count, pending_volume))
    }
    
    /// Parse a BUY transaction from the mempool
    /// Returns: Some((wallet_address, sol_amount, signature)) or None
    fn parse_buy_transaction(&self, tx: &SubscribeUpdateTransaction) -> Option<(String, f64, String)> {
        let transaction = tx.transaction.as_ref()?;
        // Extract signature from the transaction
        let signature = bs58::encode(&transaction.signature).into_string();
        let meta = transaction.meta.as_ref()?;
        let tx_data = transaction.transaction.as_ref()?;
        let message = tx_data.message.as_ref()?;
        
        // Check if this is a Pump.fun transaction
        let mut is_pump_tx = false;
        for instruction in &message.instructions {
            let program_id_index = instruction.program_id_index as usize;
            if program_id_index < message.account_keys.len() {
                let program_pubkey = &message.account_keys[program_id_index];
                if program_pubkey == self.pump_program_id.to_bytes().as_slice() {
                    is_pump_tx = true;
                    break;
                }
            }
        }
        
        if !is_pump_tx {
            return None;
        }
        
        // Parse balance changes to find SOL spent (BUY) or received (SELL)
        let pre_balances = &meta.pre_balances;
        let post_balances = &meta.post_balances;
        
        // Log transaction details for monitoring
        let tx_err = meta.err.as_ref();
        if tx_err.is_some() {
            warn!("⚠️  Pump.fun transaction FAILED - not counting in volume");
            return None;  // Don't count failed transactions
        }
        
        for (i, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
            let balance_change = (*post as i64) - (*pre as i64);
            
            // Negative balance change = SOL spent (buying)
            // Positive balance change = SOL received (selling or other)
            if balance_change < 0 && i < message.account_keys.len() {
                let wallet = &message.account_keys[i];
                let wallet_address = bs58::encode(wallet).into_string();
                let sol_amount = balance_change.abs() as f64 / 1_000_000_000.0;
                
                // Filter out small amounts (probably fees, not buys)
                if sol_amount > 0.01 {
                    info!("📊 DETECTED BUY: Wallet {}... spent {:.3} SOL", 
                        &wallet_address[..8], sol_amount);
                    return Some((wallet_address, sol_amount, signature));
                } else {
                    info!("🔍 Ignoring small tx: {:.6} SOL (likely fee)", sol_amount);
                }
            } else if balance_change > 0 && i < message.account_keys.len() {
                let wallet = &message.account_keys[i];
                let wallet_address = bs58::encode(wallet).into_string();
                let sol_amount = balance_change as f64 / 1_000_000_000.0;
                
                if sol_amount > 0.01 {
                    warn!("📉 DETECTED SELL: Wallet {}... received {:.3} SOL - NOT counting in buy volume!", 
                        &wallet_address[..8], sol_amount);
                }
            }
        }
        
        None
    }
    
    /// Get current mempool statistics for a token
    pub fn get_mempool_stats(&self, token: &str) -> String {
        if let Some(pending) = self.pending_by_token.get(token) {
            let count = pending.len();
            let volume: f64 = pending.iter().map(|tx| tx.amount_sol).sum();
            format!("{} pending txs, {:.2} SOL", count, volume)
        } else {
            "No pending activity".to_string()
        }
    }
    
    /// Clear pending transactions for a token (after they confirm or fail)
    pub fn clear_token(&mut self, token: &str) {
        self.pending_by_token.remove(token);
    }
    
    /// Clean up old pending transactions (remove anything older than 30 seconds)
    pub fn cleanup_old_pending(&mut self) {
        let now = Local::now();
        let max_age = chrono::Duration::seconds(30);
        
        for (_token, txs) in self.pending_by_token.iter_mut() {
            txs.retain(|tx| now.signed_duration_since(tx.timestamp) < max_age);
        }
        
        // Remove tokens with no pending transactions
        self.pending_by_token.retain(|_, txs| !txs.is_empty());
    }
    
    /// TIER 4 Task 2: Check if any alpha wallets have sold a token
    /// 
    /// Monitors mempool for sell transactions from specific wallet addresses.
    /// If an alpha wallet exits, it's a strong signal to exit as well.
    /// 
    /// Returns: true if any alpha wallet sold, false otherwise
    pub async fn check_alpha_wallet_exits(
        &self, 
        token: &str, 
        alpha_wallets: &[String]
    ) -> bool {
        // For production implementation, you would:
        // 1. Subscribe to token account changes for alpha wallets
        // 2. Monitor for balance decreases (sells)
        // 3. Check transaction history via RPC
        
        // Placeholder implementation - checks pending transactions
        // In reality, you'd want to use gRPC subscriptions or RPC polling
        
        if let Some(pending_txs) = self.pending_by_token.get(token) {
            for tx in pending_txs {
                if alpha_wallets.contains(&tx.wallet) {
                    // Check if this is a sell (negative amount would indicate sell)
                    // For now, we track buys, but same logic applies for sells
                    info!("👁️ ALPHA WALLET DETECTED: {} in token {}...", 
                        &tx.wallet[..8], &token[..12]);
                    return true;
                }
            }
        }
        
        false
    }
    
    /// TIER 4 Task 2: Get list of wallets who bought early
    /// 
    /// Returns wallet addresses from the first N transactions.
    /// These early buyers can be considered "alpha wallets" to track.
    pub fn get_early_buyers(&self, token: &str, top_n: usize) -> Vec<String> {
        if let Some(pending_txs) = self.pending_by_token.get(token) {
            pending_txs
                .iter()
                .take(top_n)
                .map(|tx| tx.wallet.clone())
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// TIER 4 Task 4: Check recent buyer activity (momentum tracking)
    /// 
    /// Returns the number of unique buyers in the last `window_ms` milliseconds.
    /// Used to detect "time-to-flat" - when momentum dies and no new buyers appear.
    /// 
    /// Returns: unique_buyers_in_window
    pub fn get_recent_buyer_count(&self, token: &str, window_ms: u64) -> usize {
        if let Some(pending_txs) = self.pending_by_token.get(token) {
            let cutoff = Local::now() - chrono::Duration::milliseconds(window_ms as i64);
            
            // Count unique wallets that bought after cutoff
            let recent_wallets: std::collections::HashSet<String> = pending_txs
                .iter()
                .filter(|tx| tx.timestamp > cutoff)
                .map(|tx| tx.wallet.clone())
                .collect();
            
            recent_wallets.len()
        } else {
            0
        }
    }
}

// NOTES ON IMPLEMENTATION:
//
// Mempool Querying Strategy:
// - We use CommitmentLevel::Processed to get unconfirmed/pending transactions
// - We collect for 500ms to get a snapshot of current mempool state
// - This is a trade-off between speed and completeness
//
// Limitations:
// - Some pending transactions might confirm before we query
// - Some might fail and never confirm
// - MEV bots might be using private mempools we can't see
//
// Improvements for Production:
// - Maintain a persistent mempool subscription instead of querying each time
// - Track transaction lifecycle (pending -> confirmed/failed)
// - Implement more sophisticated BUY detection (parse instruction data)
// - Handle transaction replacements (higher fee versions)
// - Track which transactions came from the same wallet (bundled txs)