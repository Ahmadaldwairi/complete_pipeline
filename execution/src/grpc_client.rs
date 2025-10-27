use crate::config::Config;
use log::{info, error, warn};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof,
    SubscribeRequest,
    SubscribeRequestFilterAccounts,
    SubscribeRequestFilterTransactions,
    SubscribeUpdateTransaction,
    CommitmentLevel,
};
use std::collections::{HashMap, HashSet};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tokio_stream::StreamExt;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub struct GrpcClient {
    endpoint: String,
    auth_token: Option<String>,
    pump_program_id: Pubkey,
}

#[derive(Debug, Clone)]
pub struct LaunchEvent {
    pub token_address: String,
    pub creator: String,
    pub initial_volume: f64,
    pub unique_buyers: u32,
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub detection_time_ms: f64,  // How fast we detected it
}

/// Transaction landing event (for latency tracking)
#[derive(Debug, Clone)]
pub struct TransactionLandedEvent {
    pub trace_id: String,
    pub signature: String,
    pub slot: u64,
    pub tx_index: u64,        // Position in block
    pub rank_in_slot: u32,    // How many pump txs before us
    pub landed_at: std::time::Instant,
}

/// Tracks priority fees from recent Pump.fun transactions for dynamic fee calculation
#[derive(Clone)]
pub struct PriorityFeeTracker {
    fees: Arc<RwLock<Vec<(Instant, u64)>>>,  // (timestamp, microlamports per CU)
    max_age: Duration,  // How long to keep entries (default 10s)
    max_entries: usize, // Max entries to prevent unbounded growth (default 100)
}

impl PriorityFeeTracker {
    pub fn new() -> Self {
        Self {
            fees: Arc::new(RwLock::new(Vec::new())),
            max_age: Duration::from_secs(10),
            max_entries: 100,
        }
    }
    
    /// Add a priority fee observation from a recent transaction
    pub fn add_fee(&self, fee_microlamports: u64) {
        let mut fees = self.fees.write().unwrap();
        fees.push((Instant::now(), fee_microlamports));
        
        // Prune old entries and limit size
        let cutoff = Instant::now() - self.max_age;
        fees.retain(|(timestamp, _)| *timestamp > cutoff);
        
        // Keep only most recent if over limit
        if fees.len() > self.max_entries {
            let excess = fees.len() - self.max_entries;
            fees.drain(0..excess);
        }
    }
    
    /// Calculate p95 (95th percentile) of recent priority fees
    /// Returns None if insufficient data (< 10 samples)
    pub fn get_p95(&self) -> Option<u64> {
        let fees = self.fees.read().unwrap();
        
        // Need at least 10 samples for meaningful p95
        if fees.len() < 10 {
            return None;
        }
        
        // Prune stale entries first
        let cutoff = Instant::now() - self.max_age;
        let mut recent_fees: Vec<u64> = fees.iter()
            .filter(|(timestamp, _)| *timestamp > cutoff)
            .map(|(_, fee)| *fee)
            .collect();
        
        if recent_fees.len() < 10 {
            return None;
        }
        
        // Sort to find p95
        recent_fees.sort_unstable();
        
        // Calculate 95th percentile index
        let p95_idx = (recent_fees.len() as f64 * 0.95).floor() as usize;
        let p95_idx = p95_idx.min(recent_fees.len() - 1);
        
        Some(recent_fees[p95_idx])
    }
    
    /// Get current sample count for diagnostics
    pub fn sample_count(&self) -> usize {
        let fees = self.fees.read().unwrap();
        let cutoff = Instant::now() - self.max_age;
        fees.iter().filter(|(timestamp, _)| *timestamp > cutoff).count()
    }
}

// Tracks volume over a time window after launch
struct VolumeTracker {
    token_address: String,
    launch_time: std::time::Instant,
    total_volume: f64,
    unique_wallets: HashSet<String>,
}

impl VolumeTracker {
    fn new(token_address: String) -> Self {
        VolumeTracker {
            token_address,
            launch_time: std::time::Instant::now(),
            total_volume: 0.0,
            unique_wallets: HashSet::new(),
        }
    }
    
    fn add_transaction(&mut self, wallet: String, sol_amount: f64) {
        self.unique_wallets.insert(wallet);
        self.total_volume += sol_amount;
    }
    
    fn elapsed_seconds(&self) -> u64 {
        self.launch_time.elapsed().as_secs()
    }
    
    fn get_stats(&self) -> (f64, u32) {
        (self.total_volume, self.unique_wallets.len() as u32)
    }
}

impl GrpcClient {
    pub async fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!("gRPC endpoint configured: {}", config.grpc_endpoint);
        
        // Pump.fun program ID
        let pump_program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")?;
        
        Ok(GrpcClient {
            endpoint: config.grpc_endpoint.clone(),
            auth_token: None,  // No auth for localhost
            pump_program_id,
        })
    }
    
    /// Monitor for new token launches
    /// This is the main detection loop - runs continuously
    pub async fn monitor_launches(&self) -> Result<LaunchEvent, Box<dyn std::error::Error + Send + Sync>> {
        // Connect to gRPC
        let mut client = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token::<String>(None)?
            .connect()
            .await?;
        
        info!("✅ Connected to Yellowstone gRPC");
        
        // Subscribe to Pump.fun program transactions
        let mut accounts: HashMap<String, SubscribeRequestFilterAccounts> = HashMap::new();
        accounts.insert(
            "pump_program".to_string(),
            SubscribeRequestFilterAccounts {
                account: vec![self.pump_program_id.to_string()],
                owner: vec![],
                filters: vec![],
                nonempty_txn_signature: None,
            },
        );
        
        let mut transactions: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
        transactions.insert(
            "pump_transactions".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![self.pump_program_id.to_string()],
                account_exclude: vec![],
                account_required: vec![],
            },
        );
        
        let request = SubscribeRequest {
            accounts,
            slots: HashMap::new(),
            transactions,
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };
        
        // Subscribe and get stream
        let mut stream = client.subscribe_once(request).await?;
        
        info!("🔍 Monitoring Pump.fun launches...");
        println!("⚡ Listening for CREATE instructions only...\n");
        
        // Counters for statistics
        let mut total_transactions = 0;
        let mut total_launches = 0;
        let start_time = std::time::Instant::now();
        
        // Wait for messages
        loop {
            match stream.next().await {
                Some(Ok(msg)) => {
                    // Record when we received the message
                    let receive_time = std::time::Instant::now();
                    
                    if let Some(update) = msg.update_oneof {
                        match update {
                            UpdateOneof::Transaction(tx_update) => {
                                total_transactions += 1;
                                
                                // Only log every 100 transactions to reduce noise
                                if total_transactions % 100 == 0 {
                                    let elapsed = start_time.elapsed().as_secs();
                                    let tps = total_transactions as f64 / elapsed.max(1) as f64;
                                    println!("📊 Processed {} transactions ({:.1} tx/s) | {} launches detected", 
                                        total_transactions, tps, total_launches);
                                }
                                
                                // Try to parse as a launch
                                if let Some(mut launch) = self.parse_transaction(&tx_update) {
                                    total_launches += 1;
                                    
                                    // Calculate detection latency
                                    let detection_latency_ms = receive_time.elapsed().as_micros() as f64 / 1000.0;
                                    launch.detection_time_ms = detection_latency_ms;
                                    
                                    // Color-code the latency
                                    let latency_indicator = if detection_latency_ms < 50.0 {
                                        format!("⚡ {:.3}ms (EXCELLENT)", detection_latency_ms)
                                    } else if detection_latency_ms < 100.0 {
                                        format!("✅ {:.3}ms (GOOD)", detection_latency_ms)
                                    } else if detection_latency_ms < 200.0 {
                                        format!("⚠️  {:.3}ms (OK)", detection_latency_ms)
                                    } else {
                                        format!("❌ {:.3}ms (SLOW)", detection_latency_ms)
                                    };
                                    
                                    println!("\n🚀 LAUNCH DETECTED!");
                                    println!("📊 Token: {}", launch.token_address);
                                    println!("👤 Creator: {}", launch.creator);
                                    println!("💰 Initial Volume: {:.2} SOL", launch.initial_volume);
                                    println!("👥 Unique Buyers: {}", launch.unique_buyers);
                                    println!("⚡ Detection Speed: {}", latency_indicator);
                                    println!();
                                    
                                    return Ok(launch);
                                }
                            }
                            UpdateOneof::TransactionStatus(_) => {
                                // Ignore transaction status updates
                            }
                            _ => {
                                // Ignore other update types
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    error!("❌ Stream error: {}", e);
                    return Err(e.into());
                }
                None => {
                    error!("⚠️  Stream ended unexpectedly");
                    return Err("Stream ended".into());
                }
            }
        }
    }
    
    /// Track volume and buyers for a specific token over a time window
    /// This implements the SLOW PATH - waits for confirmed transactions
    /// Can exit early if thresholds are met within first 2 seconds
    /// 
    /// Returns: (total_volume, unique_buyers)
    pub async fn track_token_volume(
        &mut self,
        token_address: &str,
        duration_seconds: u64,
    ) -> Result<(f64, u32), Box<dyn std::error::Error + Send + Sync>> {
        println!("⏳ Tracking volume for {} seconds...", duration_seconds);
        
        let mut tracker = VolumeTracker::new(token_address.to_string());
        
        // Connect to gRPC for tracking this specific token
        let mut client = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token::<String>(None)?
            .connect()
            .await?;
        
        // Subscribe to transactions involving this token
        let mut transactions: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
        transactions.insert(
            "token_transactions".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![token_address.to_string()],
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
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };
        
        let mut stream = client.subscribe_once(request).await?;
        
        // Track transactions for the specified duration
        loop {
            // Check if time window has elapsed
            if tracker.elapsed_seconds() >= duration_seconds {
                let (volume, buyers) = tracker.get_stats();
                println!("✅ Tracking complete: {:.2} SOL, {} buyers", volume, buyers);
                return Ok((volume, buyers));
            }
            
            // Use timeout to avoid blocking forever
            let timeout = tokio::time::timeout(
                tokio::time::Duration::from_secs(1),
                stream.next()
            ).await;
            
            match timeout {
                Ok(Some(Ok(msg))) => {
                    if let Some(UpdateOneof::Transaction(tx_update)) = msg.update_oneof {
                        // Parse BUY transactions and add to tracker
                        if let Some((wallet, sol_amount)) = self.parse_buy_transaction(&tx_update) {
                            tracker.add_transaction(wallet, sol_amount);
                            
                            let (vol, buyers) = tracker.get_stats();
                            println!("  📈 +{:.2} SOL | Total: {:.2} SOL, {} buyers", 
                                sol_amount, vol, buyers);
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    warn!("Stream error during tracking: {}", e);
                }
                Ok(None) => {
                    warn!("Stream ended during tracking");
                    break;
                }
                Err(_) => {
                    // Timeout - just continue loop to check elapsed time
                }
            }
        }
        
        // Return current stats if stream ended early
        Ok(tracker.get_stats())
    }
    
    /// Track volume with early exit if thresholds are met within 2 seconds
    /// Returns: (volume, buyers, elapsed_seconds)
    pub async fn track_token_volume_with_early_exit(
        &self,
        token_address: &str,
        duration_seconds: u64,
        min_volume: f64,
        min_buyers: u32,
    ) -> Result<(f64, u32, f64), Box<dyn std::error::Error + Send + Sync>> {
        println!("⏳ Tracking volume for up to {} seconds (early exit if thresholds met)...", duration_seconds);
        
        let mut tracker = VolumeTracker::new(token_address.to_string());
        
        // Connect to gRPC for tracking this specific token
        let mut client = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token::<String>(None)?
            .connect()
            .await?;
        
        // Subscribe to transactions involving this token
        let mut transactions: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
        transactions.insert(
            "token_transactions".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![token_address.to_string()],
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
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };
        
        let mut stream = client.subscribe_once(request).await?;
        
        // Track transactions for the specified duration
        loop {
            let elapsed = tracker.elapsed_seconds();
            let elapsed_f = elapsed as f64;
            
            // Check if time window has elapsed
            if elapsed >= duration_seconds {
                let (volume, buyers) = tracker.get_stats();
                println!("✅ Tracking complete: {:.2} SOL, {} buyers ({}s)", volume, buyers, elapsed_f);
                return Ok((volume, buyers, elapsed_f));
            }
            
            // Early exit: After 2 seconds, if thresholds are met, exit immediately
            if elapsed >= 2 {
                let (volume, buyers) = tracker.get_stats();
                if volume >= min_volume && buyers >= min_buyers {
                    println!("⚡ Early exit! Thresholds met at {:.1}s: {:.2} SOL, {} buyers", elapsed_f, volume, buyers);
                    return Ok((volume, buyers, elapsed_f));
                }
            }
            
            // Use timeout to avoid blocking forever
            let timeout = tokio::time::timeout(
                tokio::time::Duration::from_secs(1),
                stream.next()
            ).await;
            
            match timeout {
                Ok(Some(Ok(msg))) => {
                    if let Some(UpdateOneof::Transaction(tx_update)) = msg.update_oneof {
                        // Parse BUY transactions and add to tracker
                        if let Some((wallet, sol_amount)) = self.parse_buy_transaction(&tx_update) {
                            tracker.add_transaction(wallet, sol_amount);
                            
                            let (vol, buyers) = tracker.get_stats();
                            println!("  📈 +{:.2} SOL | Total: {:.2} SOL, {} buyers", 
                                sol_amount, vol, buyers);
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    warn!("Stream error during tracking: {}", e);
                }
                Ok(None) => {
                    warn!("Stream ended during tracking");
                    break;
                }
                Err(_) => {
                    // Timeout - just continue loop to check elapsed time
                }
            }
        }
        
        // Return current stats if stream ended early
        let (volume, buyers) = tracker.get_stats();
        Ok((volume, buyers, tracker.elapsed_seconds() as f64))
    }
    
    fn parse_transaction(&self, tx: &SubscribeUpdateTransaction) -> Option<LaunchEvent> {
        // Extract transaction data
        let transaction = tx.transaction.as_ref()?;
        let meta = transaction.meta.as_ref()?;
        
        // Get the actual transaction
        let tx_data = transaction.transaction.as_ref()?;
        let message = tx_data.message.as_ref()?;
        
        // Look for Pump.fun instructions
        for instruction in &message.instructions {
            let program_id_index = instruction.program_id_index as usize;
            
            // Check if this instruction is for Pump.fun program
            if program_id_index < message.account_keys.len() {
                let program_pubkey = &message.account_keys[program_id_index];
                
                // Check if it's the Pump.fun program
                if program_pubkey == self.pump_program_id.to_bytes().as_slice() {
                    // Parse instruction data
                    let data = &instruction.data;
                    
                    if data.is_empty() {
                        continue;
                    }
                    
                    // Pump.fun CREATE instruction discriminator
                    if data.len() >= 8 {
                        let discriminator = &data[0..8];
                        let create_discriminator = [24u8, 30, 200, 40, 5, 28, 7, 119];
                        
                        if discriminator == create_discriminator {
                            // Extract accounts
                            let accounts = &instruction.accounts;
                            
                            if accounts.len() >= 2 {
                                let token_mint_index = accounts[0] as usize;
                                let creator_index = accounts[1] as usize;
                                
                                if token_mint_index < message.account_keys.len() 
                                    && creator_index < message.account_keys.len() {
                                    
                                    let token_mint = &message.account_keys[token_mint_index];
                                    let creator = &message.account_keys[creator_index];
                                    
                                    // Convert to base58 strings
                                    let token_address = bs58::encode(token_mint).into_string();
                                    let creator_address = bs58::encode(creator).into_string();
                                    
                                    // Calculate initial metrics from CREATE transaction
                                    let (initial_volume, unique_buyers) = 
                                        self.calculate_initial_metrics(meta);
                                    
                                    return Some(LaunchEvent {
                                        token_address,
                                        creator: creator_address,
                                        initial_volume,
                                        unique_buyers,
                                        timestamp: chrono::Local::now(),
                                        detection_time_ms: 0.0,  // Set by caller
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        
        None
    }
    
    /// Parse a BUY transaction and extract wallet + SOL amount
    /// Returns: Some((wallet_address, sol_amount)) or None
    fn parse_buy_transaction(&self, tx: &SubscribeUpdateTransaction) -> Option<(String, f64)> {
        let transaction = tx.transaction.as_ref()?;
        let meta = transaction.meta.as_ref()?;
        let tx_data = transaction.transaction.as_ref()?;
        let message = tx_data.message.as_ref()?;
        
        // Parse pre/post balances to find SOL transfers
        let pre_balances = &meta.pre_balances;
        let post_balances = &meta.post_balances;
        
        for (i, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
            let balance_change = (*post as i64) - (*pre as i64);
            
            // If balance decreased (someone bought), extract info
            if balance_change < 0 && i < message.account_keys.len() {
                let wallet = &message.account_keys[i];
                let wallet_address = bs58::encode(wallet).into_string();
                let sol_amount = balance_change.abs() as f64 / 1_000_000_000.0;
                
                return Some((wallet_address, sol_amount));
            }
        }
        
        None
    }
    
    /// Extract priority fee (SetComputeUnitPrice) from transaction instructions
    /// Returns microlamports per CU if found
    fn extract_priority_fee(
        message: &yellowstone_grpc_proto::prelude::Message
    ) -> Option<u64> {
        // ComputeBudget program ID: ComputeBudget111111111111111111111111111111
        let compute_budget_program = "ComputeBudget111111111111111111111111111111";
        
        for ix in &message.instructions {
            // Check if this is a ComputeBudget instruction
            if (ix.program_id_index as usize) < message.account_keys.len() {
                let program_key = &message.account_keys[ix.program_id_index as usize];
                let program_str = bs58::encode(program_key).into_string();
                
                if program_str == compute_budget_program {
                    // SetComputeUnitPrice instruction discriminator is 3
                    // Format: [3, u64_le_bytes (8 bytes)]
                    if ix.data.len() == 9 && ix.data[0] == 3 {
                        // Extract u64 from bytes 1-8 (little-endian)
                        let price_bytes: [u8; 8] = ix.data[1..9].try_into().ok()?;
                        let price = u64::from_le_bytes(price_bytes);
                        return Some(price);
                    }
                }
            }
        }
        
        None
    }
    
    fn calculate_initial_metrics(
        &self,
        meta: &yellowstone_grpc_proto::prelude::TransactionStatusMeta
    ) -> (f64, u32) {
        let mut total_volume = 0.0;
        let mut unique_buyers = 0;
        
        // Parse pre and post balances
        let pre_balances = &meta.pre_balances;
        let post_balances = &meta.post_balances;
        
        for (_i, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
            let balance_change = (*post as i64) - (*pre as i64);
            
            if balance_change < 0 {
                let sol_amount = balance_change.abs() as f64 / 1_000_000_000.0;
                total_volume += sol_amount;
                unique_buyers += 1;
            }
        }
        
        (total_volume, unique_buyers)
    }
    
    /// Monitor for our own transactions landing on-chain
    /// Returns trace_id, slot, tx_index, rank_in_slot when we see our transaction
    /// Also tracks priority fees from all Pump.fun transactions for dynamic fee calculation
    pub async fn monitor_transaction_landing(
        &self,
        signature: String,
        fee_tracker: Option<PriorityFeeTracker>,  // Optional fee tracker for dynamic fees
    ) -> Result<TransactionLandedEvent, Box<dyn std::error::Error + Send + Sync>> {
        // Connect to gRPC
        let mut client = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token::<String>(None)?
            .connect()
            .await?;
        
        info!("🔍 Monitoring for transaction: {}...", &signature[..12]);
        
        // Subscribe to ALL Pump.fun transactions to calculate rank
        let mut transactions: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
        transactions.insert(
            "pump_transactions".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![self.pump_program_id.to_string()],
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
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };
        
        // Subscribe and get stream
        let mut stream = client.subscribe_once(request).await?;
        
        // Track transactions per slot to calculate rank
        let mut current_slot: u64 = 0;
        let mut txs_in_slot: u32 = 0;
        let mut our_tx_found = false;
        
        // Wait for our transaction with timeout
        let timeout = tokio::time::Duration::from_secs(60);
        let start = std::time::Instant::now();
        
        loop {
            if start.elapsed() > timeout {
                return Err("Transaction monitoring timeout (60s)".into());
            }
            
            match tokio::time::timeout(tokio::time::Duration::from_secs(1), stream.next()).await {
                Ok(Some(Ok(msg))) => {
                    if let Some(update) = msg.update_oneof {
                        if let UpdateOneof::Transaction(tx_update) = update {
                            if let Some(tx) = tx_update.transaction {
                                let tx_signature = bs58::encode(&tx.signature).into_string();
                                let tx_slot = tx_update.slot;
                                
                                // New slot? Reset counter
                                if tx_slot != current_slot {
                                    current_slot = tx_slot;
                                    txs_in_slot = 0;
                                }
                                
                                // Extract priority fee from ALL Pump.fun transactions for tracking
                                if let Some(ref tracker) = fee_tracker {
                                    if let Some(message) = tx.transaction.as_ref().and_then(|t| t.message.as_ref()) {
                                        if let Some(priority_fee) = Self::extract_priority_fee(message) {
                                            tracker.add_fee(priority_fee);
                                        }
                                    }
                                }
                                
                                // Check if this is our transaction
                                if tx_signature == signature {
                                    our_tx_found = true;
                                    let landed_at = std::time::Instant::now();
                                    
                                    // Extract trace_id from memo instruction if present
                                    let trace_id = self.extract_trace_id_from_tx(&tx)
                                        .unwrap_or_else(|| "unknown".to_string());
                                    
                                    info!("✅ Transaction landed! Slot: {}, Rank: #{}", tx_slot, txs_in_slot + 1);
                                    
                                    return Ok(TransactionLandedEvent {
                                        trace_id,
                                        signature: tx_signature,
                                        slot: tx_slot,
                                        tx_index: txs_in_slot as u64,
                                        rank_in_slot: txs_in_slot + 1,
                                        landed_at,
                                    });
                                }
                                
                                // Count this transaction for rank calculation
                                txs_in_slot += 1;
                            }
                        }
                    }
                },
                Ok(Some(Err(e))) => {
                    warn!("gRPC stream error: {}", e);
                },
                Ok(None) => {
                    return Err("gRPC stream closed unexpectedly".into());
                },
                Err(_) => {
                    // Timeout on this iteration, continue
                    continue;
                }
            }
        }
    }
    
    /// Extract trace_id from transaction's memo instruction
    fn extract_trace_id_from_tx(
        &self,
        tx: &yellowstone_grpc_proto::prelude::SubscribeUpdateTransactionInfo,
    ) -> Option<String> {
        // Parse transaction to find memo instruction in logs
        if let Some(ref meta) = tx.meta {
            // log_messages is Vec<String>, not Option
            for log in &meta.log_messages {
                if log.contains("Memo") && log.contains("-") {
                    // Extract UUID-like trace_id from log (format: "Program log: Memo (len X): trace_id")
                    if let Some(start_idx) = log.rfind(": ") {
                        let trace_id = log[start_idx + 2..].trim().to_string();
                        if trace_id.contains("-") && trace_id.len() > 30 {
                            return Some(trace_id);
                        }
                    }
                }
            }
        }
        None
    }
}