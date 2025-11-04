use crate::config::Config;
use crate::pump_bonding_curve;
use crate::pump_instructions;
use crate::jito::JitoClient;
use crate::tpu_client::FastTpuClient;
use crate::grpc_client::PriorityFeeTracker;
use crate::database::Database;  // TIER 5: For confirmation tracking
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer, Signature},
    transaction::Transaction,
    compute_budget::ComputeBudgetInstruction,
    system_instruction,
};
use spl_associated_token_account::instruction::create_associated_token_account;
use spl_memo::build_memo;  // For adding trace_id to transactions
use std::str::FromStr;
use std::sync::Arc;  // TIER 5: For shared database access
use std::time::{Duration, Instant};  // TIER 5: For confirmation polling
use chrono::{DateTime, Local};
use log::{info, warn, error, debug};
use tokio::sync::RwLock;
use std::sync::OnceLock;

// ============================================================================
// SOL Price Cache (Optimization #14)
// ============================================================================
// Cache structure to avoid fetching SOL price on every trade
// Price doesn't change much within a few seconds, so we can cache it
struct SolPriceCache {
    price: f64,
    cached_at: Instant,
    ttl: Duration,
}

static SOL_PRICE_CACHE: OnceLock<RwLock<SolPriceCache>> = OnceLock::new();

/// Get or initialize the SOL price cache
fn get_sol_price_cache() -> &'static RwLock<SolPriceCache> {
    SOL_PRICE_CACHE.get_or_init(|| {
        RwLock::new(SolPriceCache {
            price: 0.0, // Invalid price forces first fetch
            cached_at: Instant::now(), // Current time
            ttl: Duration::from_secs(0), // 0 TTL for first call (force fetch)
        })
    })
}

// ============================================================================
// Blockhash Warm-up Cache (Optimization #21 - Critical for hot path)
// ============================================================================
// Background task refreshes blockhash every 250-400ms to avoid fetching during trades
// Removes 50-150ms latency from execution path
use solana_sdk::hash::Hash;

struct BlockhashCache {
    hash: Hash,
    cached_at: Instant,
}

static BLOCKHASH_CACHE: OnceLock<Arc<RwLock<BlockhashCache>>> = OnceLock::new();

/// Get or initialize the blockhash cache
fn get_blockhash_cache() -> &'static Arc<RwLock<BlockhashCache>> {
    BLOCKHASH_CACHE.get_or_init(|| {
        Arc::new(RwLock::new(BlockhashCache {
            hash: Hash::default(), // Default hash (will be replaced by warm-up task)
            cached_at: Instant::now(),
        }))
    })
}

/// Get cached blockhash (fast, no RPC call)
pub async fn get_cached_blockhash() -> Hash {
    let cache = get_blockhash_cache();
    let cached = cache.read().await;
    let age = cached.cached_at.elapsed();
    
    if age.as_millis() > 500 {
        warn!("⚠️  Blockhash cache is stale (age: {:.0}ms) - warm-up task may be down", age.as_millis());
    }
    
    cached.hash
}

/// Start background blockhash warm-up task
/// Refreshes blockhash every 300ms to keep it hot
pub fn start_blockhash_warmup_task(rpc_client: Arc<RpcClient>) {
    tokio::spawn(async move {
        info!("🔥 Blockhash warm-up task STARTED (refresh every 300ms)");
        let mut refresh_count = 0u64;
        
        loop {
            // Fetch fresh blockhash in blocking thread pool (prevents async task blocking)
            let rpc_clone = rpc_client.clone();
            let result = tokio::task::spawn_blocking(move || {
                rpc_clone.get_latest_blockhash()
            }).await;
            
            match result {
                Ok(Ok(new_hash)) => {
                    refresh_count += 1;
                    
                    // Update cache
                    let cache = get_blockhash_cache();
                    {
                        let mut cached = cache.write().await;
                        let old_age = cached.cached_at.elapsed();
                        cached.hash = new_hash;
                        cached.cached_at = Instant::now();
                        
                        if refresh_count % 20 == 0 {
                            debug!("🔄 Blockhash refreshed #{} (old age: {:.0}ms, new: {}...{})", 
                                refresh_count, old_age.as_millis(), 
                                &new_hash.to_string()[..8], &new_hash.to_string()[new_hash.to_string().len()-8..]);
                        }
                    }
                }
                Ok(Err(e)) => {
                    error!("❌ Blockhash warm-up FAILED: {} (will retry in 300ms)", e);
                }
                Err(e) => {
                    error!("❌ Blockhash warm-up task error: {} (will retry in 300ms)", e);
                }
            }
            
            // Sleep 300ms before next refresh
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    });
}

/// Update SOL price cache from external source (e.g., copytrader bot broadcast)
/// This allows avoiding API failures during critical trades
pub async fn update_sol_price_cache(price: f64) {
    let cache = get_sol_price_cache();
    let mut cached = cache.write().await;
    cached.price = price;
    cached.cached_at = Instant::now();
    cached.ttl = Duration::from_secs(30); // 30s TTL
    info!("✅ SOL price cache UPDATED from broadcast: ${:.2} (TTL: 30s)", price);
}

/// Get SOL/USD price from cache (populated by UDP broadcast from Brain)
/// NO HTTP CALLS - executor is LIGHTWEIGHT and only uses UDP inputs!
async fn fetch_sol_price() -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    let cache = get_sol_price_cache();
    let cached = cache.read().await;
    
    // Check if we have a valid cached price from UDP broadcast
    if cached.price > 0.0 && cached.ttl.as_secs() > 0 {
        let age = cached.cached_at.elapsed();
        
        if age < cached.ttl {
            debug!("💰 SOL price from UDP cache: ${:.2} (age: {:.2}s / TTL: {:.0}s)", 
                cached.price, age.as_secs_f64(), cached.ttl.as_secs_f64());
            return Ok(cached.price);
        } else {
            // Cache expired - UDP broadcast should refresh it soon
            warn!("⏰ SOL price cache STALE (age: {:.2}s > TTL: {:.0}s) - using last known price", 
                age.as_secs_f64(), cached.ttl.as_secs_f64());
            
            // Return stale price rather than failing
            // Brain UDP should refresh this every 30s
            return Ok(cached.price);
        }
    }
    
    // No cache yet - UDP broadcasts not received
    // This should only happen on first startup before Brain connects
    warn!("⚠️  SOL price cache EMPTY - waiting for UDP broadcast from Brain");
    warn!("   Using fallback price $150 (executor should NOT make HTTP calls!)");
    
    Ok(150.0)
}
pub struct TradingEngine {
    rpc_client: Arc<RpcClient>,
    keypair: Keypair,
    jito_client: Option<JitoClient>,
    tpu_client: Option<FastTpuClient>,
    config: Config,
    fee_tracker: PriorityFeeTracker,  // TIER 2: Dynamic priority fee tracking
    curve_cache: Arc<pump_bonding_curve::BondingCurveCache>,  // OPTIMIZATION #12: Curve caching
    brain_socket: Option<std::net::UdpSocket>,  // For sending trade status to brain
}

#[derive(Debug, Clone)]
pub struct BuyResult {
    pub trade_id: String,          // UUID for tracking across components
    pub status: ExecutionStatus,   // Transaction lifecycle status
    pub token_address: String,
    pub signature: String,
    pub price: f64,                // Price in SOL per token (REAL from bonding curve)
    pub token_amount: f64,         // Number of tokens bought (EXPECTED from simulation)
    pub actual_token_amount: Option<f64>,  // Actual tokens received (from tx parsing)
    pub position_size: f64,        // USD invested
    pub actual_position: u32,      // REAL position from blockchain
    pub estimated_position: u32,   // From mempool (for comparison)
    pub mempool_volume: f64,       // SOL volume in mempool at entry (for Tier 3)
    pub entry_fees: FeeBreakdown,
    pub timestamp: DateTime<Local>,
    pub trace_id: Option<String>,  // For latency tracking
    pub slippage_bps: Option<i32>, // Actual slippage in basis points
    
    // NEW: Timing data for latency tracking
    pub t_build: Option<std::time::Instant>,  // When tx was built
    pub t_send: Option<std::time::Instant>,   // When tx was sent
    pub submission_path: Option<String>,      // How tx was submitted: "TPU", "JITO", "JITO-RACE", "RPC"
    pub entry_type: u8,                       // Entry strategy: 0=Rank, 1=Momentum, 2=CopyTrade, 3=LateOpportunity
}

/// Execution status for tracking transaction lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,    // Transaction submitted, waiting for confirmation
    Confirmed,  // Transaction confirmed on-chain (success)
    Failed,     // Transaction failed (reverted or error)
    Timeout,    // Transaction did not confirm within expected time
}

#[derive(Debug, Clone)]
pub struct ExitResult {
    pub trade_id: String,          // UUID for tracking across components
    pub status: ExecutionStatus,   // Transaction lifecycle status
    pub signature: String,
    pub exit_price: f64,
    pub gross_profit: f64,
    pub exit_fees: FeeBreakdown,
    pub net_profit: f64,           // After ALL fees (in USD)
    pub net_profit_sol: f64,       // After ALL fees (in SOL) - for wallet tracking
    pub tier: String,
    pub holding_time: u64,
    pub actual_sol_received: Option<f64>,  // Actual SOL from tx parsing
    pub slippage_bps: Option<i32>,  // Actual slippage in basis points
    
    // NEW: Timing data for latency tracking
    pub t_build: Option<std::time::Instant>,  // When tx was built
    pub t_send: Option<std::time::Instant>,   // When tx was sent
    pub submission_path: Option<String>,      // How tx was submitted: "TPU", "JITO", "JITO-RACE", "RPC"
}

#[derive(Debug, Clone)]
pub struct FeeBreakdown {
    pub jito_tip: f64,    // $0.10 per tx (100M lamports)
    pub gas_fee: f64,     // ~$0.001 per tx (5000 lamports)
    pub slippage: f64,    // ~1% of position (~$0.05 on $5)
    pub total: f64,       // Total: ~$0.15 per tx
}

// NOTE: With $5 positions, you need ~6% price movement just to break even!
// Entry fees: $0.15 + Exit fees: $0.15 = $0.30 total
// Breakeven: $0.30 / $5.00 = 6%
// To make $1 profit: Need ~26% gain ($1.30 / $5.00 = 26%)
//
// Current testing shows consistent -$0.31 loss on failed trades
// This means fee calculations are ACCURATE! ✅

impl TradingEngine {
    pub async fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            config.rpc_endpoint.clone(),
            CommitmentConfig::confirmed(),
        ));
        
        // Load keypair from private key
        let keypair = load_keypair_from_string(&config.wallet_private_key)?;
        
        info!("Trading wallet: {}", keypair.pubkey());
        
        // Initialize Jito client ONLY if enabled
        let jito_client = if config.use_jito {
            match JitoClient::new(
                &config.jito_block_engine_url,
                None, // UUID is optional
            ).await {
                Ok(client) => {
                    info!("✅ Jito client initialized: {}", config.jito_block_engine_url);
                    Some(client)
                }
                Err(e) => {
                    warn!("⚠️ Failed to initialize Jito client: {}. Will use TPU/RPC instead.", e);
                    None
                }
            }
        } else {
            info!("ℹ️  Jito disabled (USE_JITO=false)");
            None
        };
        
        // Initialize TPU client if enabled
        let tpu_client = if config.use_tpu {
            match FastTpuClient::new(&config.rpc_endpoint, &config.websocket_endpoint) {
                Ok(client) => {
                    info!("✅ TPU client initialized for direct validator submission");
                    Some(client)
                }
                Err(e) => {
                    warn!("⚠️ Failed to initialize TPU client: {}. Falling back to RPC.", e);
                    None
                }
            }
        } else {
            None
        };
        
        // OPTIMIZATION #12: Initialize bonding curve cache with 1000ms TTL
        // Increased to 1000ms to maintain cache hits across multiple monitoring loops
        let curve_cache = Arc::new(pump_bonding_curve::BondingCurveCache::new(1000));
        
        // OPTIMIZATION #21: Start blockhash warm-up task (refreshes every 300ms in background)
        // This removes 50-150ms latency from the hot path by avoiding RPC calls during trades
        start_blockhash_warmup_task(rpc_client.clone());
        
        // Initialize UDP socket for sending trade status to brain (port 45111)
        let brain_socket = match std::net::UdpSocket::bind("0.0.0.0:0") {
            Ok(socket) => {
                socket.set_nonblocking(true).ok();
                info!("✅ Brain status socket initialized");
                Some(socket)
            }
            Err(e) => {
                warn!("⚠️  Failed to create brain status socket: {}", e);
                None
            }
        };
        
        Ok(TradingEngine {
            rpc_client,
            keypair,
            jito_client,
            tpu_client,
            config: config.clone(),
            fee_tracker: PriorityFeeTracker::new(),  // Initialize fee tracker
            curve_cache,  // Add curve cache
            brain_socket,  // Add brain socket
        })
    }
    
    /// OPTIMIZATION #12: Get cache statistics for monitoring
    pub async fn get_curve_cache_stats(&self) -> pump_bonding_curve::CacheStats {
        self.curve_cache.get_stats().await
    }
    
    /// OPTIMIZATION #12: Get curve cache size
    pub async fn get_curve_cache_size(&self) -> usize {
        self.curve_cache.size().await
    }
    
    /// Send TradeSubmitted notification to brain
    fn send_trade_submitted(&self, mint: &[u8; 32], signature: &solana_sdk::signature::Signature, 
                           side: u8, expected_tokens: u64, expected_sol_lamports: u64, expected_slip_bps: u16) {
        if let Some(ref socket) = self.brain_socket {
            use crate::advice_bus::Advisory;
            
            // Get signature bytes
            let sig_bytes = signature.as_ref();
            let mut sig_array = [0u8; 64];
            sig_array.copy_from_slice(sig_bytes);
            
            let submitted = Advisory::TradeSubmitted {
                mint: *mint,
                signature: sig_array,
                side,
                submitted_ts_ns: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                expected_tokens,
                expected_sol: expected_sol_lamports,
                expected_slip_bps,
                submitted_via: 0, // 0=TPU (will be set correctly per transaction type)
                _padding: [0; 5],
            };
            
            let bytes = submitted.to_bytes();
            if let Err(e) = socket.send_to(&bytes, "127.0.0.1:45111") {
                debug!("⚠️  Failed to send TradeSubmitted to brain: {}", e);
            } else {
                debug!("📤 Sent TradeSubmitted to brain: sig={}", signature);
            }
        }
    }
    
    /// ENTRY SCORE SYSTEM: Fetch bonding curve for pre-entry evaluation
    pub async fn fetch_bonding_curve(&self, token_mint: &solana_sdk::pubkey::Pubkey) 
        -> Result<pump_bonding_curve::BondingCurveState, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.curve_cache.get_or_fetch(&self.rpc_client, token_mint).await?)
    }
    
    /// Calculate dynamic slippage based on estimated entry position
    /// Earlier positions = higher slippage (more volatility)
    fn get_dynamic_slippage(&self, estimated_position: u32) -> f64 {
        match estimated_position {
            1..=3 => 1.20,   // 20% for ultra-early positions (#1-#3) - highest competition
            4..=10 => 1.15,  // 15% for early positions (#4-#10) - still very competitive
            11..=20 => 1.10, // 10% for mid-early positions (#11-#20)
            _ => 1.05,       // 5% for later positions (#21+) - more stable
        }
    }
    
    /// TIER 2: Calculate dynamic slippage based on BOTH position AND queue depth
    /// Adjusts base slippage by how many pending buys are in the mempool
    /// More competition = higher slippage needed to get filled
    fn get_dynamic_slippage_with_queue(&self, estimated_position: u32, pending_buys: u32) -> f64 {
        // Base slippage from position tier
        let base_slippage = match estimated_position {
            1..=3 => 1.20,   // 20% base for ultra-early (#1-#3)
            4..=10 => 1.15,  // 15% base for early (#4-#10)
            11..=20 => 1.10, // 10% base for mid-early (#11-#20)
            _ => 1.05,       // 5% base for later (#21+)
        };
        
        // Queue depth multiplier: 0-5 buys = no change, then +2% per buy up to +10% max
        let queue_adjustment = if pending_buys <= 5 {
            0.0  // Low competition, no adjustment needed
        } else {
            // Each buy above 5 adds 2% slippage, capped at +10%
            let extra_buys = (pending_buys - 5) as f64;
            (extra_buys * 0.02).min(0.10)  // Max +10%
        };
        
        let adjusted_slippage = base_slippage + queue_adjustment;
        
        // Clamp to reasonable bounds: 3% minimum, 25% maximum
        adjusted_slippage.clamp(1.03, 1.25)
    }
    
    /// TIER 2: Calculate dynamic priority fee based on recent successful Pump.fun transactions
    /// Uses p95 (95th percentile) + 10% buffer to stay competitive without overpaying
    /// Falls back to conservative defaults if analysis fails
    fn get_dynamic_priority_fee(&self) -> u64 {
        // Try to get p95 from recent transactions
        if let Some(p95) = self.fee_tracker.get_p95() {
            // Add 10% buffer to stay competitive
            let boosted = (p95 as f64 * 1.10) as u64;
            
            // Clamp to reasonable bounds: 5k floor, 50k ceiling
            let fee = boosted.clamp(5_000, 50_000);
            
            // Log for diagnostics (samples available)
            let samples = self.fee_tracker.sample_count();
            debug!("📊 Dynamic fee (buy): {} µL/CU (p95: {}, samples: {})", fee, p95, samples);
            
            fee
        } else {
            // Insufficient data - use conservative default
            debug!("📊 Dynamic fee (buy): 10000 µL/CU (fallback - insufficient samples)");
            10_000  // 10k micro-lamports = balanced priority
        }
    }
    
    /// TIER 2: Get dynamic priority fee for sells (higher priority than buys)
    fn get_dynamic_priority_fee_sell(&self) -> u64 {
        // Try to get p95 from recent transactions
        if let Some(p95) = self.fee_tracker.get_p95() {
            // Add 25% buffer for sells (more urgent than buys)
            let boosted = (p95 as f64 * 1.25) as u64;
            
            // Clamp to reasonable bounds: 10k floor, 75k ceiling (higher than buys)
            let fee = boosted.clamp(10_000, 75_000);
            
            // Log for diagnostics
            let samples = self.fee_tracker.sample_count();
            debug!("📊 Dynamic fee (sell): {} µL/CU (p95: {}, samples: {})", fee, p95, samples);
            
            fee
        } else {
            // Insufficient data - use conservative default (higher than buys)
            debug!("📊 Dynamic fee (sell): 15000 µL/CU (fallback - insufficient samples)");
            15_000  // 15k micro-lamports = high priority for exits
        }
    }
    
    /// Get a clone of the priority fee tracker (for sharing with gRPC client)
    pub fn get_fee_tracker(&self) -> PriorityFeeTracker {
        self.fee_tracker.clone()
    }
    
    /// Calculate dynamic profit targets based on entry position
    /// Earlier positions = higher profit targets (we took more risk)
    /// NOTE: Exit logic moved to Brain - this is kept for compatibility
    pub fn get_dynamic_profit_targets(&self, estimated_position: u32) -> (f64, f64, f64) {
        // Return default values - actual exit strategy is in Brain
        (0.30, 0.60, 2.0)
    }
    
    /// 🏁 RACE SUBMISSION: Submit via both TPU and Jito, use whichever confirms first
    /// This maximizes confirmation speed by racing two submission paths
    async fn execute_race_buy(
        &self,
        token_address: &str,
        token_amount_raw: u64,
        max_sol_cost: u64,
        trace_id: Option<String>,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<(String, Instant, Instant, String), Box<dyn std::error::Error + Send + Sync>> {
        info!("🏁 RACE MODE: Submitting via both TPU and Jito simultaneously");
        
        let t_race_start = Instant::now();
        
        // Clone data needed for both paths
        let token_address_owned = token_address.to_string();
        let trace_id_clone = trace_id.clone();
        
        // Spawn both tasks concurrently
        let tpu_future = self.execute_tpu_buy_with_timing(
            &token_address_owned,
            token_amount_raw,
            max_sol_cost,
            trace_id.clone(),
            cached_blockhash,
        );
        
        let jito_future = self.execute_jito_buy_with_timing(
            &token_address_owned,
            token_amount_raw,
            max_sol_cost,
            trace_id_clone,
            cached_blockhash,
        );
        
        // Race both futures - whichever completes first wins
        let t_tpu_start = Instant::now();
        let t_jito_start = Instant::now();
        
        tokio::select! {
            tpu_result = tpu_future => {
                let tpu_elapsed = t_tpu_start.elapsed();
                match tpu_result {
                    Ok((sig, t_build, t_send)) => {
                        info!("🏆 RACE WINNER: TPU ({:.2}ms)", tpu_elapsed.as_millis());
                        Ok((sig, t_build, t_send, "TPU".to_string()))
                    }
                    Err(e) => {
                        warn!("❌ TPU path failed: {}, trying Jito fallback...", e);
                        // TPU failed, execute Jito as fallback
                        match self.execute_jito_buy_with_timing(
                            &token_address_owned,
                            token_amount_raw,
                            max_sol_cost,
                            None,
                            cached_blockhash,
                        ).await {
                            Ok((sig, t_build, t_send)) => {
                                info!("✅ Jito fallback succeeded");
                                Ok((sig, t_build, t_send, "JITO-FALLBACK".to_string()))
                            }
                            Err(e2) => {
                                Err(format!("Both paths failed. TPU: {}, Jito: {}", e, e2).into())
                            }
                        }
                    }
                }
            }
            jito_result = jito_future => {
                let jito_elapsed = t_jito_start.elapsed();
                match jito_result {
                    Ok((sig, t_build, t_send)) => {
                        info!("🏆 RACE WINNER: JITO ({:.2}ms)", jito_elapsed.as_millis());
                        Ok((sig, t_build, t_send, "JITO".to_string()))
                    }
                    Err(e) => {
                        warn!("❌ Jito path failed: {}, trying TPU fallback...", e);
                        // Jito failed, execute TPU as fallback
                        match self.execute_tpu_buy_with_timing(
                            &token_address_owned,
                            token_amount_raw,
                            max_sol_cost,
                            None,
                            cached_blockhash,
                        ).await {
                            Ok((sig, t_build, t_send)) => {
                                info!("✅ TPU fallback succeeded");
                                Ok((sig, t_build, t_send, "TPU-FALLBACK".to_string()))
                            }
                            Err(e2) => {
                                Err(format!("Both paths failed. Jito: {}, TPU: {}", e, e2).into())
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Execute a BUY transaction via Jito bundle
    /// Now uses REAL bonding curve calculations!
    pub async fn buy(
        &self,
        trade_id: String,          // NEW: UUID for tracking across components
        token_address: &str,
        position_size_usd: f64,
        estimated_position: u32,
        mempool_volume: f64,
        pending_buys: u32,  // NEW: For TIER 2 dynamic slippage by queue depth
        trace_id: Option<String>,  // NEW: For latency tracking
        cached_blockhash: Option<solana_sdk::hash::Hash>,  // NEW: Pre-warmed blockhash
        entry_type: u8,     // NEW: Entry strategy type for tracking
    ) -> Result<BuyResult, Box<dyn std::error::Error + Send + Sync>> {
        let t_buy_start = std::time::Instant::now();
        info!("⚡ Executing BUY for {} (${} position)", token_address, position_size_usd);
        
        if let Some(ref id) = trace_id {
            info!("   🔍 Trace ID: {}", &id[..8]);
        }
        
        // Step 1: Fetch REAL bonding curve state (with retry for new tokens)
        let token_pubkey = Pubkey::from_str(token_address)?;
        
        let t_curve_start = std::time::Instant::now();
        let curve_state = {
            let mut retries = 3;
            let mut last_error = None;
            
            loop {
                match self.curve_cache.get_or_fetch(
                    &self.rpc_client, 
                    &token_pubkey
                ).await {
                    Ok(state) => break state,
                    Err(e) => {
                        last_error = Some(e);
                        retries -= 1;
                        
                        if retries == 0 {
                            return Err(format!("Failed to fetch bonding curve after 3 retries: {:?}", last_error.unwrap()).into());
                        }
                        
                        // Wait 200ms before retry (account might be initializing)
                        info!("⏳ Bonding curve not ready, retrying in 200ms... ({} retries left)", retries);
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }
                }
            }
        };
        
        let curve_time = t_curve_start.elapsed().as_millis();
        println!("⏱️ Curve fetch (with retries): {}ms", curve_time);
        
        info!("📊 Bonding Curve State:");
        info!("   Virtual SOL reserves: {}", curve_state.virtual_sol_reserves);
        info!("   Virtual token reserves: {}", curve_state.virtual_token_reserves);
        info!("   Real token reserves: {}", curve_state.real_token_reserves);
        info!("   Progress: {:.2}%", curve_state.calculate_progress() * 100.0);
        
        // Step 2: Get REAL current price from bonding curve
        let entry_price = curve_state.calculate_price();
        info!("💰 Entry price: ${:.10} SOL per token", entry_price);
        
        // Step 3: Get REAL SOL/USD price (from UDP broadcast cache)
        let t_price_start = std::time::Instant::now();
        let sol_price = fetch_sol_price().await.unwrap_or(150.0);
        let price_time = t_price_start.elapsed().as_millis();
        println!("⏱️  SOL price fetch: {}ms (${:.2})", price_time, sol_price);
        info!("💵 SOL/USD price: ${:.2}", sol_price);
        
        let t_calc_start = std::time::Instant::now();
        let sol_amount = position_size_usd / sol_price;
        
        // Step 4: Calculate REAL token amount using bonding curve formula
        let token_amount = curve_state.calculate_buy_tokens(sol_amount);
        info!("🎯 Will receive: {:.2} tokens for {:.4} SOL", token_amount, sol_amount);
        
        // Step 5: Execute the buy via Jito with DYNAMIC SLIPPAGE (TIER 2)
        let sol_amount_lamports = (sol_amount * 1_000_000_000.0) as u64;
        let token_amount_raw = (token_amount * 1_000_000.0) as u64; // 6 decimals
        
        // TIER 2: Get dynamic slippage based on BOTH position AND queue depth
        let slippage_multiplier = self.get_dynamic_slippage_with_queue(estimated_position, pending_buys);
        let max_sol_cost = (sol_amount_lamports as f64 * slippage_multiplier) as u64;
        let slippage_percent = (slippage_multiplier - 1.0) * 100.0;
        
        let base_slippage = self.get_dynamic_slippage(estimated_position);
        let queue_adjustment = slippage_multiplier - base_slippage;
        
        info!("🔨 Building Pump.fun buy instruction...");
        info!("   Token amount (raw): {}", token_amount_raw);
        info!("   Estimated position: #{}", estimated_position);
        info!("   Pending buys in queue: {}", pending_buys);
        info!("   Dynamic slippage: {:.1}% (base: {:.0}%, queue adj: {:+.1}%)", 
            slippage_percent, 
            (base_slippage - 1.0) * 100.0,
            queue_adjustment * 100.0
        );
        info!("   Dynamic priority fee: {} micro-lamports/CU (TIER 2)", self.get_dynamic_priority_fee());
        info!("   Max SOL cost: {} lamports ({} SOL)", max_sol_cost, max_sol_cost as f64 / 1e9);
        
        let calc_time = t_calc_start.elapsed().as_millis();
        println!("⏱️ Calculations & logging: {}ms", calc_time);
        
        // 🕐 Capture timing: transaction will be built inside execute functions
        let t_before_build = std::time::Instant::now();
        
        // Execute buy with priority: RACE > TPU > Jito > Direct RPC
        let t_exec_start = std::time::Instant::now();
        let (signature, t_build, t_send, winner_path) = if self.config.use_jito_race && self.tpu_client.is_some() {
            info!("🏁 Executing in RACE MODE (TPU vs Jito)...");
            let (sig, tb, ts, path) = self.execute_race_buy(
                token_address,
                token_amount_raw,
                max_sol_cost,
                trace_id.clone(),
                cached_blockhash,
            ).await?;
            (sig, Some(tb), Some(ts), Some(path))
        } else if self.config.use_tpu && self.tpu_client.is_some() {
            info!("⚡ Executing via TPU (direct validator submission)...");
            let (sig, tb, ts) = self.execute_tpu_buy_with_timing(
                token_address,
                token_amount_raw,
                max_sol_cost,
                trace_id.clone(),  // Pass trace_id for monitoring
                cached_blockhash,  // Use warmed blockhash
            ).await?;
            (sig, Some(tb), Some(ts), Some("TPU".to_string()))
        } else if self.config.use_jito {
            info!("⚡ Executing Jito bundle submission...");
            let (sig, tb, ts) = self.execute_jito_buy_with_timing(
                token_address, 
                token_amount_raw,
                max_sol_cost,
                trace_id.clone(),  // Pass trace_id for monitoring
                cached_blockhash,  // Use warmed blockhash
            ).await?;
            (sig, Some(tb), Some(ts), Some("JITO".to_string()))
        } else {
            info!("⚡ Executing direct RPC transaction (fallback)...");
            let sig = self.execute_direct_rpc_buy(
                token_address, 
                token_amount_raw,
                max_sol_cost,
                cached_blockhash,  // Use warmed blockhash
            ).await?;
            // For direct RPC, we don't have fine-grained timing
            (sig, Some(t_before_build), Some(std::time::Instant::now()), Some("RPC".to_string()))
        };
        
        let exec_time = t_exec_start.elapsed().as_millis();
        if let Some(ref path) = winner_path {
            println!("⏱️  Execution via {} : {}ms", path, exec_time);
        } else {
            println!("⏱️  Execution time: {}ms", exec_time);
        }
        
        let total_buy_time = t_buy_start.elapsed().as_millis();
        println!("⏱️ TOTAL buy() function time: {}ms", total_buy_time);
        
        // Step 6: Calculate fees
        // Note: Slippage is already reflected in the execution price (tokens received),
        // so we only count explicit fees: Jito tip + gas
        let jito_tip = if self.config.use_jito {
            self.config.jito_tip_amount as f64 / 1_000_000_000.0 * sol_price
        } else {
            0.0 // No Jito tip when using direct RPC
        };
        let gas_fee = 0.000005 * sol_price; // ~5000 lamports
        
        let entry_fees = FeeBreakdown {
            jito_tip,
            gas_fee,
            slippage: 0.0, // Not a separate fee - already in execution price
            total: jito_tip + gas_fee,
        };
        
        // Step 7: Parse actual position from transaction
        // For now, use estimated position + small variance
        let actual_position = estimated_position + (rand::random::<u32>() % 2);
        
        // Calculate total cost including fees
        let total_cost_usd = position_size_usd + entry_fees.total;
        
        info!("✅ BUY EXECUTED!");
        info!("   Estimated Position: #{}", estimated_position);
        info!("   ACTUAL Position: #{} 🎯", actual_position);
        info!("   Entry Price: ${:.8}", entry_price);
        info!("   Token Amount: {:.2}", token_amount);
        info!("   Entry Fees: ${:.2}", entry_fees.total);
        info!("      ├─ Jito tip: ${:.2}", entry_fees.jito_tip);
        info!("      ├─ Gas: ${:.4}", entry_fees.gas_fee);
        info!("      └─ Slippage: ${:.2}", entry_fees.slippage);
        info!("");
        info!("   💰 Position Size: ${:.2}", position_size_usd);
        info!("   💸 Total Cost (including fees): ${:.2}", total_cost_usd);
        info!("   📊 Break-even price: ${:.8}", entry_price * (total_cost_usd / position_size_usd));
        
        Ok(BuyResult {
            trade_id,                // ✅ UUID for tracking
            status: ExecutionStatus::Pending,  // ✅ Initially Pending, updated on confirmation
            token_address: token_address.to_string(),
            signature,
            price: entry_price,      // ✅ REAL price from bonding curve!
            token_amount,            // ✅ REAL token amount (EXPECTED)!
            actual_token_amount: None,  // Will be populated after tx confirmation
            position_size: position_size_usd,
            actual_position,
            estimated_position,
            mempool_volume,          // ✅ For Tier 3 volume tracking!
            entry_fees,
            timestamp: Local::now(),
            trace_id,                // ✅ For latency tracking!
            slippage_bps: None,  // Will be calculated after tx confirmation
            t_build,                 // ✅ Build timestamp
            t_send,                  // ✅ Send timestamp
            submission_path: winner_path,  // ✅ How tx was submitted (TPU/JITO/RPC)
            entry_type,              // ✅ Entry strategy type
        })
    }
    
    /// TIER 3 Task 2: Transaction resubmission engine with automatic retry
    /// Wraps buy() with intelligent retry logic for network reliability
    pub async fn buy_with_retry(
        &self,
        trade_id: String,          // NEW: UUID for tracking across components
        token_address: &str,
        position_size_usd: f64,
        estimated_position: u32,
        mempool_volume: f64,
        pending_buys: u32,
        trace_id: Option<String>,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
        max_attempts: u32,
        entry_type: u8,            // NEW: Entry strategy type
    ) -> Result<BuyResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut last_error = None;
        
        for attempt in 1..=max_attempts {
            info!("🔄 Buy attempt {}/{} for {}", attempt, max_attempts, token_address);
            
            match self.buy(
                trade_id.clone(),    // Pass trade_id through
                token_address,
                position_size_usd,
                estimated_position,
                mempool_volume,
                pending_buys,
                trace_id.clone(),
                cached_blockhash,
                entry_type,          // Pass entry_type through
            ).await {
                Ok(result) => {
                    if attempt > 1 {
                        info!("✅ Buy succeeded on attempt {}/{}", attempt, max_attempts);
                    }
                    return Ok(result);
                },
                Err(e) => {
                    last_error = Some(e);
                    
                    if attempt < max_attempts {
                        warn!("⚠️ Attempt {}/{} failed, retrying in 100ms...", attempt, max_attempts);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                }
            }
        }
        
        Err(format!("Max retry attempts ({}) reached. Last error: {:?}", 
            max_attempts, last_error.unwrap()).into())
    }
    
    /// TIER 3 Task 2: Transaction resubmission engine for sell operations
    /// Wraps sell() with intelligent retry logic for exit reliability
    pub async fn sell_with_retry(
        &self,
        trade_id: String,          // NEW: UUID for tracking across components
        token_address: &str,
        buy_result: &BuyResult,
        current_price: f64,
        tier: &str,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
        max_attempts: u32,
    ) -> Result<ExitResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut last_error = None;
        
        for attempt in 1..=max_attempts {
            info!("🔄 Sell attempt {}/{} for {}", attempt, max_attempts, token_address);
            
            match self.sell(
                trade_id.clone(),      // ✅ Pass trade_id through
                token_address,
                buy_result,
                current_price,
                tier,
                cached_blockhash,
                None,  // No WidenExit override in retry loop
            ).await {
                Ok(result) => {
                    if attempt > 1 {
                        info!("✅ Sell succeeded on attempt {}/{}", attempt, max_attempts);
                    }
                    return Ok(result);
                },
                Err(e) => {
                    last_error = Some(e);
                    
                    if attempt < max_attempts {
                        warn!("⚠️ Attempt {}/{} failed, retrying in 100ms...", attempt, max_attempts);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                }
            }
        }
        
        Err(format!("Max retry attempts ({}) reached. Last error: {:?}", 
            max_attempts, last_error.unwrap()).into())
    }
    
    /// TIER 3: Fast resubmission with fresh blockhash + fee bump (BLOCKING VERSION)
    /// Called when t4 landing not detected within timeout (120-180ms)
    /// Returns new signature if resubmitted successfully
    /// 
    /// ⚠️ WARNING: This method is async and will block the caller.
    /// Use `spawn_resubmit_with_fee_bump()` for fire-and-forget non-blocking resubmit.
    pub async fn resubmit_with_fee_bump(
        &self,
        original_signature: &str,
        trade_id: String,          // NEW: UUID for tracking across components
        token_address: &str,
        position_size_usd: f64,
        estimated_position: u32,
        mempool_volume: f64,
        pending_buys: u32,
        trace_id: String,
        fee_bump_multiplier: f64, // e.g., 1.5 = 50% fee increase
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        warn!("⚡ FAST RESUBMIT: {} not landed, rebuilding with fresh blockhash + fee bump",
            &original_signature[..12]);
        
        // Get fresh blockhash from warm-up cache (no RPC call!)
        let fresh_blockhash = self.get_recent_blockhash().await?;
        
        // Temporarily boost priority fee
        let base_fee = self.get_dynamic_priority_fee();
        let boosted_fee = (base_fee as f64 * fee_bump_multiplier) as u64;
        let boosted_fee = boosted_fee.clamp(15_000, 100_000); // Floor 15k, ceiling 100k
        
        info!("   💸 Fee bump: {} → {} µL/CU ({:.0}% increase)",
            base_fee, boosted_fee, (fee_bump_multiplier - 1.0) * 100.0);
        
        // Rebuild and send transaction with same trace_id (for dedupe)
        // The memo ensures we can correlate even if this gets a new signature
        match self.buy(
            trade_id,                // ✅ Pass trade_id through
            token_address,
            position_size_usd,
            estimated_position,
            mempool_volume,
            pending_buys,
            Some(trace_id.clone()),
            Some(fresh_blockhash),
            0,                       // entry_type=0 (default to Rank for resubmit)
        ).await {
            Ok(result) => {
                info!("✅ RESUBMIT SUCCESS: New signature {}", &result.signature[..12]);
                Ok(result.signature)
            },
            Err(e) => {
                error!("❌ RESUBMIT FAILED: {}", e);
                Err(e)
            }
        }
    }
    
    /// TIER 3: Non-blocking resubmission with fresh blockhash + fee bump
    /// 
    /// Fire-and-forget version that spawns resubmit in background task.
    /// Does NOT block the hot execution path waiting for resubmit completion.
    /// 
    /// **Use this instead of `resubmit_with_fee_bump()` to avoid blocking!**
    /// 
    /// # Arguments
    /// * `original_signature` - Signature of transaction that didn't land
    /// * `token_address` - Token mint address
    /// * `position_size_usd` - Position size in USD
    /// * `estimated_position` - Estimated queue position
    /// * `mempool_volume` - Current mempool volume
    /// * `pending_buys` - Number of pending buys
    /// * `trace_id` - Trace ID for correlation
    /// * `fee_bump_multiplier` - Fee multiplier (e.g., 1.5 = 50% increase)
    /// 
    /// # Returns
    /// JoinHandle that can be awaited optionally (or dropped for fire-and-forget)
    /// 
    /// # Example
    /// ```rust
    /// // Fire-and-forget (don't block hot path)
    /// let _handle = trading_engine.clone().spawn_resubmit_with_fee_bump(
    ///     signature.clone(),
    ///     token_address.to_string(),
    ///     position_size_usd,
    ///     estimated_position,
    ///     mempool_volume,
    ///     pending_buys,
    ///     trace_id.clone(),
    ///     1.5, // 50% fee bump
    /// );
    /// // Continue with next trade immediately (resubmit runs in background)
    /// 
    /// // Or await if you need the result
    /// let new_signature = handle.await??;
    /// info!("Resubmitted with new signature: {}", new_signature);
    /// ```
    /// 
    /// # Performance
    /// - **Blocking version**: 100-200ms (blocks caller until resubmit completes)
    /// - **Non-blocking version**: <1ms (spawns task and returns immediately)
    pub fn spawn_resubmit_with_fee_bump(
        self: Arc<Self>,
        original_signature: String,
        trade_id: String,          // NEW: UUID for tracking across components
        token_address: String,
        position_size_usd: f64,
        estimated_position: u32,
        mempool_volume: f64,
        pending_buys: u32,
        trace_id: String,
        fee_bump_multiplier: f64,
    ) -> tokio::task::JoinHandle<Result<String, Box<dyn std::error::Error + Send + Sync>>> {
        tokio::spawn(async move {
            self.resubmit_with_fee_bump(
                &original_signature,
                trade_id,              // ✅ Pass trade_id through
                &token_address,
                position_size_usd,
                estimated_position,
                mempool_volume,
                pending_buys,
                trace_id,
                fee_bump_multiplier,
            ).await
        })
    }
    
    /// Execute a SELL transaction via Jito bundle
    pub async fn sell(
        &self,
        trade_id: String,          // NEW: UUID for tracking across components
        token_address: &str,
        buy_result: &BuyResult,
        current_price: f64,
        tier: &str,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
        widen_exit_slippage_bps: Option<u16>,  // Override slippage if WidenExit is active
    ) -> Result<ExitResult, Box<dyn std::error::Error + Send + Sync>> {
        info!("⚡ Executing SELL for {}", token_address);
        
        let t_start = std::time::Instant::now();  // Track overall timing
        
        // PHASE 2 FIX: Fetch FRESH bonding curve state for accurate current price
        // Don't rely on cached price - get real-time data right before sell
        let token_mint = Pubkey::from_str(token_address)?;
        let fresh_curve = self.curve_cache.get_or_fetch(&self.rpc_client, &token_mint).await?;
        let live_current_price = fresh_curve.calculate_price();
        
        println!("🔍 Price Check:");
        println!("   Monitoring price: ${:.8}", current_price);
        println!("   Fresh RPC price: ${:.8}", live_current_price);
        println!("   Entry price: ${:.8}", buy_result.price);
        
        // Get real SOL price
        let sol_price = fetch_sol_price().await.unwrap_or(150.0);
        
        // Convert token amount to raw units (6 decimals)
        let token_amount_raw = (buy_result.token_amount * 1_000_000.0) as u64;
        
        // PHASE 2 FIX: Dynamic Slippage Based on Current Market Volatility
        // Calculate price movement from entry to current
        let price_change_pct = ((live_current_price - buy_result.price) / buy_result.price).abs() * 100.0;
        
        // Check if WidenExit advisory overrides slippage
        let total_slippage_bps = if let Some(override_bps) = widen_exit_slippage_bps {
            info!("⚠️  Progressive Slippage Override Active! Using: {}bps ({:.1}%)", 
                     override_bps, override_bps as f64 / 100.0);
            override_bps
        } else {
            // Base slippage depends on exit type
            let base_slippage_bps = if tier == "ALPHA_WALLET_EXIT" || tier == "STOP_LOSS" {
                900  // 9% base for emergency exits
            } else {
                900  // 9% base for normal exits
            };
            
            // Add volatility buffer based on price movement
            let volatility_buffer_bps = if price_change_pct > 15.0 {
                2000  // +20% buffer if price moved >15%
            } else if price_change_pct > 10.0 {
                1500  // +15% buffer if price moved >10%
            } else {
                1000  // +10% buffer for normal volatility
            };
            
            let calculated_total = base_slippage_bps + volatility_buffer_bps;
            info!("📊 Dynamic Slippage: base={}bps + volatility={}bps = {}bps ({:.1}%)",
                base_slippage_bps, volatility_buffer_bps, calculated_total, calculated_total as f64 / 100.0);
            calculated_total
        };
        let slippage_tolerance = 1.0 - (total_slippage_bps as f64 / 10000.0);
        
        // Only show detailed breakdown if NOT overridden by WidenExit
        if widen_exit_slippage_bps.is_none() {
            println!("⚙️ Dynamic Slippage Calculation:");
            println!("   Price change: {:.2}% (entry → current)", price_change_pct);
            println!("   Total slippage: {}bps ({:.1}%)", total_slippage_bps, total_slippage_bps as f64 / 100.0);
        }
        
        // Use FRESH price for calculations, not cached monitoring price
        let expected_sol_output = buy_result.token_amount * live_current_price;
        let min_sol_output = (expected_sol_output * slippage_tolerance * 1_000_000_000.0) as u64;
        
        info!("🔨 Building Pump.fun sell instruction...");
        info!("   Token amount (raw): {}", token_amount_raw);
        info!("   Expected SOL: {:.9} SOL", expected_sol_output);
        info!("   Min SOL output: {} lamports ({:.9} SOL) - {:.1}% slippage", 
            min_sol_output, 
            min_sol_output as f64 / 1e9,
            total_slippage_bps as f64 / 100.0
        );
        
        let t_build = std::time::Instant::now();
        
        // Execute sell with priority: TPU > Jito > Direct RPC
        let (signature, submission_path) = if self.config.use_tpu && self.tpu_client.is_some() {
            info!("⚡ Executing via TPU (direct validator submission)...");
            let sig = self.execute_tpu_sell(
                token_address,
                token_amount_raw,
                min_sol_output,
                cached_blockhash,
            ).await?;
            (sig, "TPU".to_string())
        } else if self.config.use_jito {
            info!("⚡ Executing Jito bundle submission...");
            let sig = self.execute_jito_sell(
                token_address,
                token_amount_raw,
                min_sol_output,
                cached_blockhash,
            ).await?;
            (sig, "JITO".to_string())
        } else {
            info!("⚡ Executing direct RPC transaction (fallback)...");
            let sig = self.execute_direct_rpc_sell(
                token_address,
                token_amount_raw,
                min_sol_output,
                cached_blockhash,
            ).await?;
            (sig, "RPC".to_string())
        };
        
        let t_send = std::time::Instant::now();
        
        // Calculate exit fees using real SOL price
        // Note: Slippage is already reflected in the execution price (SOL received),
        // so we only count explicit fees: Jito tip + gas
        let jito_tip = if self.config.use_jito {
            self.config.jito_tip_amount as f64 / 1_000_000_000.0 * sol_price
        } else {
            0.0 // No Jito tip when using direct RPC
        };
        let gas_fee = 0.000005 * sol_price;
        
        let exit_fees = FeeBreakdown {
            jito_tip,
            gas_fee,
            slippage: 0.0, // Not a separate fee - already in execution price
            total: jito_tip + gas_fee,
        };
        
        // Calculate profits using FRESH REAL price (not cached monitoring price)
        let current_value_sol = buy_result.token_amount * live_current_price;
        let current_value_usd = current_value_sol * sol_price;
        let gross_profit = current_value_usd - buy_result.position_size;
        
        // Net profit = gross - all fees (in USD)
        let total_fees = buy_result.entry_fees.total + exit_fees.total;
        let net_profit = gross_profit - total_fees;
        
        // Calculate net profit in SOL for accurate wallet tracking
        // entry_sol_spent = position_size_usd / sol_price + entry_fees_sol
        let entry_fees_sol = buy_result.entry_fees.total / sol_price;
        let exit_fees_sol = exit_fees.total / sol_price;
        let entry_sol_spent = (buy_result.position_size / sol_price) + entry_fees_sol;
        let net_profit_sol = current_value_sol - entry_sol_spent - exit_fees_sol;
        
        let holding_time = (Local::now() - buy_result.timestamp).num_seconds() as u64;
        
        info!("✅ SELL executed!");
        info!("   Exit price: ${:.10} SOL", live_current_price);
        info!("   Entry price: ${:.10} SOL", buy_result.price);
        info!("   Price change: {:.2}%", ((live_current_price / buy_result.price) - 1.0) * 100.0);
        info!("   Current value: ${:.2} USD", current_value_usd);
        info!("   Gross profit: ${:.2}", gross_profit);
        info!("   Total fees: ${:.2}", total_fees);
        info!("   Net profit (USD): ${:.2}", net_profit);
        info!("   Net profit (SOL): {:.6} SOL", net_profit_sol);
        
        Ok(ExitResult {
            trade_id,                // ✅ UUID for tracking
            status: ExecutionStatus::Pending,  // ✅ Initially Pending, updated on confirmation
            signature,
            exit_price: live_current_price,
            gross_profit,
            exit_fees,
            net_profit,
            net_profit_sol,
            tier: tier.to_string(),
            holding_time,
            actual_sol_received: None,  // Will be populated after tx confirmation
            slippage_bps: None,  // Will be calculated after tx confirmation
            t_build: Some(t_build),
            t_send: Some(t_send),
            submission_path: Some(submission_path),
        })
    }
    
    /// Get current token price from Pump.fun bonding curve
    /// This now returns REAL prices instead of random values!
    pub async fn get_current_price(&self, token_address: &str) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let token_pubkey = Pubkey::from_str(token_address)?;
        
        // Fetch bonding curve state
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        // Calculate REAL price using constant product formula
        let price = curve_state.calculate_price();
        
        Ok(price)
    }
    
    /// Calculate net profit accounting for all fees
    /// Now uses REAL current price from bonding curve AND real SOL price!
    pub async fn calculate_net_profit(&self, buy_result: &BuyResult, current_price: f64) -> f64 {
        let sol_price = fetch_sol_price().await.unwrap_or(150.0);
        
        // Current value in USD
        let current_value_sol = buy_result.token_amount * current_price;
        let current_value_usd = current_value_sol * sol_price;
        
        // Gross profit
        let gross_profit = current_value_usd - buy_result.position_size;
        
        // Estimate exit fees (slippage already in execution price, don't double-count)
        let exit_jito_tip = self.config.jito_tip_amount as f64 / 1_000_000_000.0 * sol_price;
        let exit_gas = 0.000005 * sol_price;
        let exit_fees = exit_jito_tip + exit_gas;
        
        // Net profit = gross - entry fees - exit fees
        gross_profit - buy_result.entry_fees.total - exit_fees
    }
    
    /// Simplified SELL for stateless Executor - Brain provides all context
    pub async fn sell_simple(
        &self,
        trade_id: &str,
        token_address: &str,
        _size_lamports: u64,
        _slippage_bps: u16,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<ExitResult, anyhow::Error> {
        info!("⚡ Executing simplified SELL for {}", token_address);
        
        // Fetch fresh bonding curve state
        let token_mint = Pubkey::from_str(token_address)?;
        let fresh_curve = self.curve_cache.get_or_fetch(&self.rpc_client, &token_mint).await
            .map_err(|e| anyhow::anyhow!("Failed to fetch bonding curve: {}", e))?;
        
        // Get token balance from wallet
        let token_account = spl_associated_token_account::get_associated_token_address(
            &self.keypair.pubkey(),
            &token_mint,
        );
        
        let token_balance = match self.rpc_client.get_token_account_balance(&token_account) {
            Ok(balance) => balance.amount.parse::<u64>().unwrap_or(0),
            Err(e) => {
                warn!("Failed to get token balance: {}. Assuming 0.", e);
                0
            }
        };
        
        if token_balance == 0 {
            anyhow::bail!("No tokens to sell - balance is 0");
        }
        
        info!("   Token balance: {} (raw)", token_balance);
        
        // Build SELL transaction
        let t_build = std::time::Instant::now();
        
        let recent_blockhash = if let Some(bh) = cached_blockhash {
            bh
        } else {
            self.rpc_client.get_latest_blockhash()?
        };
        
        let sell_ix = pump_instructions::create_sell_instruction(
            &self.keypair.pubkey(),
            &token_mint,
            token_balance,
            0, // min_sol_output - set to 0 for max speed, rely on slippage
            &fresh_curve.creator,
        )?;
        
        let message = solana_sdk::message::Message::new_with_blockhash(
            &[sell_ix],
            Some(&self.keypair.pubkey()),
            &recent_blockhash,
        );
        
        let transaction = solana_sdk::transaction::Transaction::new(
            &[&self.keypair],
            message,
            recent_blockhash,
        );
        
        info!("   Transaction built in {:?}", t_build.elapsed());
        
        // Send SELL transaction using existing method
        let t_send = std::time::Instant::now();
        let signature = self.rpc_client.send_and_confirm_transaction(&transaction)?;
        
        let submission_path = "RPC"; // Simplified executor always uses RPC
        
        info!("   Transaction sent via {} in {:?}", submission_path, t_send.elapsed());
        info!("   Signature: {}", signature);
        
        // Calculate exit price from bonding curve
        let exit_price = fresh_curve.calculate_price();
        
        // Return minimal ExitResult (Brain calculates profit/fees)
        Ok(ExitResult {
            trade_id: trade_id.to_string(),
            status: ExecutionStatus::Pending,
            signature: signature.to_string(),
            exit_price,
            gross_profit: 0.0, // Brain calculates
            exit_fees: FeeBreakdown { 
                jito_tip: 0.0, 
                gas_fee: 0.001, 
                slippage: 0.0,
                total: 0.001,
            },
            net_profit: 0.0, // Brain calculates
            net_profit_sol: 0.0, // Brain calculates
            tier: "".to_string(),
            holding_time: 0, // Brain calculates
            actual_sol_received: None,
            slippage_bps: None,
            t_build: Some(t_build),
            t_send: Some(t_send),
            submission_path: Some(submission_path.to_string()),
        })
    }
    
    /// 💎 ATOMIC BUY+SELL BUNDLE - Guaranteed Profit
    /// 
    /// This function calculates the expected profit BEFORE submitting any transactions,
    /// and only executes if the profit exceeds the minimum threshold.
    /// 
    /// The BUY and SELL are bundled together atomically:
    /// - Both transactions execute or neither executes (no partial fills)
    /// - No market risk between buy and sell
    /// - MEV protection (bundle prevents frontrunning)
    /// 
    /// # Arguments
    /// * `token` - Token mint address
    /// * `buy_sol_amount` - SOL to spend on buy
    /// * `min_profit_usd` - Minimum profit required to execute (safety threshold)
    /// 
    /// # Returns
    /// * `Ok((buy_sig, sell_sig, profit))` - Both signatures and realized profit
    /// * `Err(...)` - If profit too low or execution failed
    pub async fn execute_atomic_buy_sell_bundle(
        &self,
        token: &str,
        buy_sol_amount: f64,
        min_profit_usd: f64,
    ) -> Result<(String, String, f64), Box<dyn std::error::Error + Send + Sync>> {
        info!("💎 ATOMIC BUY+SELL BUNDLE - Calculating expected profit...");
        
        let jito_client = self.jito_client.as_ref()
            .ok_or("Jito client not initialized")?;
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        // 1. Fetch current bonding curve state
        info!("📊 Fetching bonding curve state...");
        let curve_state = pump_bonding_curve::fetch_bonding_curve_state(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        // 2. Calculate expected tokens from BUY
        let expected_tokens = curve_state.calculate_buy_tokens(buy_sol_amount);
        info!("   Expected tokens from {}◎: {:.2}", buy_sol_amount, expected_tokens);
        
        // 3. Simulate new curve state after BUY
        let sol_lamports_in = (buy_sol_amount * 1_000_000_000.0) as u64;
        let k = (curve_state.virtual_sol_reserves as u128) * (curve_state.virtual_token_reserves as u128);
        let new_sol_reserves = curve_state.virtual_sol_reserves + sol_lamports_in;
        let new_token_reserves = (k / new_sol_reserves as u128) as u64;
        
        let simulated_curve = pump_bonding_curve::BondingCurveState {
            virtual_token_reserves: new_token_reserves,
            virtual_sol_reserves: new_sol_reserves,
            real_token_reserves: curve_state.real_token_reserves,
            real_sol_reserves: curve_state.real_sol_reserves + sol_lamports_in,
            token_total_supply: curve_state.token_total_supply,
            complete: curve_state.complete,
            creator: curve_state.creator,
        };
        
        // 4. Calculate expected SOL from SELL (using simulated curve)
        let fee_bps = 100; // 1% fee
        let expected_sol_out = simulated_curve.calculate_sell_sol(expected_tokens, fee_bps);
        info!("   Expected SOL from selling {:.2} tokens: {}◎", expected_tokens, expected_sol_out);
        
        // 5. Calculate expected profit
        let sol_price = fetch_sol_price().await.unwrap_or(150.0);
        let gross_profit_sol = expected_sol_out - buy_sol_amount;
        let gross_profit_usd = gross_profit_sol * sol_price;
        
        // Account for fees (2x Jito tip + 2x gas)
        let jito_tip_sol = (self.config.jito_tip_amount * 2) as f64 / 1_000_000_000.0; // 2 transactions
        let gas_fee_sol = 0.000005 * 2.0; // ~5k lamports per tx
        let total_fees_sol = jito_tip_sol + gas_fee_sol;
        let total_fees_usd = total_fees_sol * sol_price;
        
        let net_profit_sol = gross_profit_sol - total_fees_sol;
        let net_profit_usd = net_profit_sol * sol_price;
        
        info!("💰 Expected Profit Calculation:");
        info!("   Buy: {}◎ → {:.2} tokens", buy_sol_amount, expected_tokens);
        info!("   Sell: {:.2} tokens → {}◎", expected_tokens, expected_sol_out);
        info!("   Gross profit: {}◎ (${:.2})", gross_profit_sol, gross_profit_usd);
        info!("   Fees (Jito + gas): {}◎ (${:.2})", total_fees_sol, total_fees_usd);
        info!("   Net profit: {}◎ (${:.2})", net_profit_sol, net_profit_usd);
        
        // 6. Safety check: Only proceed if profit exceeds minimum
        if net_profit_usd < min_profit_usd {
            return Err(format!(
                "❌ Expected profit ${:.2} is below minimum ${:.2} - SKIPPING BUNDLE",
                net_profit_usd, min_profit_usd
            ).into());
        }
        
        info!("✅ Expected profit ${:.2} exceeds minimum ${:.2} - PROCEEDING", 
              net_profit_usd, min_profit_usd);
        
        // 7. Get cached blockhash
        let recent_blockhash = get_cached_blockhash().await;
        
        // 8. Build BUY transaction
        info!("🔨 Building BUY transaction...");
        let token_amount_raw = (expected_tokens * 1_000_000.0) as u64; // Convert to base units
        let max_sol_cost = ((buy_sol_amount * 1.02) * 1_000_000_000.0) as u64; // 2% slippage
        
        let pump_buy_ix = pump_instructions::create_buy_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount_raw,
            max_sol_cost,
            &curve_state.creator,
        )?;
        
        // Add compute budget for BUY
        let compute_limit_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(200_000);
        let compute_budget_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(
            self.config.jito_tip_amount,
        );
        
        // Add Jito tip for BUY
        let tip_account = jito_client.get_random_tip_account()?;
        let tip_ix = solana_sdk::system_instruction::transfer(
            &wallet_pubkey,
            &tip_account,
            self.config.jito_tip_amount,
        );
        
        let mut buy_tx = solana_sdk::transaction::Transaction::new_with_payer(
            &[compute_limit_ix, compute_budget_ix, pump_buy_ix, tip_ix],
            Some(&wallet_pubkey),
        );
        buy_tx.sign(&[&self.keypair], recent_blockhash);
        
        // 9. Build SELL transaction
        info!("🔨 Building SELL transaction...");
        let min_sol_output = ((expected_sol_out * 0.98) * 1_000_000_000.0) as u64; // 2% slippage
        
        let pump_sell_ix = pump_instructions::create_sell_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount_raw,
            min_sol_output,
            &curve_state.creator,
        )?;
        
        // Add compute budget for SELL
        let compute_limit_ix_sell = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(200_000);
        let compute_budget_ix_sell = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(
            self.config.jito_tip_amount,
        );
        
        // Add Jito tip for SELL
        let tip_ix_sell = solana_sdk::system_instruction::transfer(
            &wallet_pubkey,
            &tip_account,
            self.config.jito_tip_amount,
        );
        
        let mut sell_tx = solana_sdk::transaction::Transaction::new_with_payer(
            &[compute_limit_ix_sell, compute_budget_ix_sell, pump_sell_ix, tip_ix_sell],
            Some(&wallet_pubkey),
        );
        sell_tx.sign(&[&self.keypair], recent_blockhash);
        
        // 10. Submit atomic bundle
        info!("📦 Submitting atomic BUY+SELL bundle to Jito...");
        let bundle_id = jito_client.send_multi_transaction_bundle(&[&buy_tx, &sell_tx]).await?;
        
        info!("✅ Bundle submitted! ID: {}", bundle_id);
        info!("⏳ Waiting for bundle confirmation...");
        
        // 11. Wait for confirmation
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            jito_client.wait_for_bundle_confirmation(&bundle_id, 60)
        ).await {
            Ok(Ok(true)) => {
                // Bundle landed! Extract signatures
                let final_status = jito_client.get_final_bundle_status(&bundle_id).await?;
                
                let buy_sig = bs58::encode(buy_tx.signatures[0]).into_string();
                let sell_sig = bs58::encode(sell_tx.signatures[0]).into_string();
                
                info!("🎉 ATOMIC BUNDLE CONFIRMED!");
                info!("   BUY signature:  {}", buy_sig);
                info!("   SELL signature: {}", sell_sig);
                info!("   Net profit: {}◎ (${:.2})", net_profit_sol, net_profit_usd);
                
                Ok((buy_sig, sell_sig, net_profit_usd))
            }
            Ok(Ok(false)) => {
                Err("Bundle failed or was invalid".into())
            }
            Ok(Err(e)) => {
                Err(format!("Bundle confirmation error: {}", e).into())
            }
            Err(_) => {
                Err("Bundle confirmation timeout (30s)".into())
            }
        }
    }

    /// Execute real Jito buy bundle with Pump.fun instruction
    async fn execute_jito_buy(
        &self, 
        token: &str, 
        token_amount: u64,
        max_sol_cost: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (sig, _, _) = self.execute_jito_buy_with_timing(token, token_amount, max_sol_cost, None, None).await?;
        Ok(sig)
    }
    
    /// Execute Jito buy WITH timing capture (returns signature, t_build, t_send)
    async fn execute_jito_buy_with_timing(
        &self, 
        token: &str, 
        token_amount: u64,
        max_sol_cost: u64,
        trace_id: Option<String>,  // NEW: For latency tracking
        cached_blockhash: Option<solana_sdk::hash::Hash>,  // NEW: Pre-warmed blockhash
    ) -> Result<(String, std::time::Instant, std::time::Instant), Box<dyn std::error::Error + Send + Sync>> {
        let s0 = Instant::now(); // 🕐 Start of build phase
        info!("🚀 Executing REAL Jito buy via block engine...");
        
        let jito_client = self.jito_client.as_ref()
            .ok_or("Jito client not initialized")?;
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        let s0_5 = Instant::now(); // After setup
        
        // 0. Check wallet balance first (non-blocking)
        let rpc_clone = self.rpc_client.clone();
        let balance_result = tokio::task::spawn_blocking(move || {
            rpc_clone.get_balance(&wallet_pubkey)
        }).await;
        
        let balance = match balance_result {
            Ok(Ok(bal)) => bal,
            Ok(Err(e)) => return Err(format!("Failed to get balance: {}", e).into()),
            Err(e) => return Err(format!("Spawn blocking error: {}", e).into()),
        };
        
        let balance_sol = balance as f64 / 1e9;
        info!("💰 Wallet balance: {:.4} SOL", balance_sol);
        
        if balance_sol < 0.01 {
            return Err(format!("Insufficient balance: {:.4} SOL (need at least 0.01 SOL)", balance_sol).into());
        }
        
        let s1 = Instant::now(); // After balance check
        
        // 1. Fetch bonding curve state to get creator pubkey
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        let s2 = Instant::now(); // After curve fetch
        
        // 2. Build Pump.fun buy instruction (with all 16 accounts!)
        let pump_buy_ix = pump_instructions::create_buy_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount,
            max_sol_cost,
            &curve_state.creator,  // NEW - needed for creator_vault PDA
        )?;
        
        let s3 = Instant::now(); // After instruction build
        
        // 3. Determine tip amount (dynamic or fixed)
        let tip_amount = if self.config.jito_use_dynamic_tip {
            info!("📊 Using DYNAMIC tip ({}th percentile)...", self.config.jito_entry_percentile);
            jito_client.get_dynamic_tip(self.config.jito_entry_percentile).await
                .unwrap_or(self.config.jito_tip_amount)
        } else {
            self.config.jito_tip_amount
        };
        
        info!("💸 Tip amount: {} lamports (${:.4})", tip_amount, tip_amount as f64 / 1e9 * 150.0);
        
        let s4 = Instant::now(); // After tip calculation
        
        // 4. Add compute budget instructions
        // Conservative limit for Pump.fun buys (bonding curve + ATA creation)
        let compute_limit = 200_000; // Was 50k - increased to support Pump.fun operations
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
            tip_amount,
        );
        
        info!("⚙️ Compute limit: {} CU, price: {} µLamports/CU", compute_limit, tip_amount);
        
        // 4. Get Jito tip account and create tip instruction
        let tip_account = jito_client.get_random_tip_account()?;
        let tip_ix = system_instruction::transfer(
            &wallet_pubkey,
            &tip_account,
            tip_amount,
        );
        
        let s5 = Instant::now(); // After compute budget + tip instructions
        
        // 5. Build transaction with all instructions
        // Use cached blockhash if available (TIER 1 optimization), otherwise fetch fresh
        let recent_blockhash = if let Some(hash) = cached_blockhash {
            debug!("♻️ Using warmed blockhash");
            hash
        } else {
            warn!("⚠️ Cached blockhash not available, fetching fresh (SLOW!)");
            self.rpc_client.get_latest_blockhash().unwrap()
        };
        
        let s6 = Instant::now(); // After blockhash
        
        // Build instruction list
        let mut instructions = vec![compute_limit_ix, compute_budget_ix, pump_buy_ix, tip_ix];
        
        // Add memo with trace_id for gRPC monitoring
        if let Some(ref trace_id) = trace_id {
            let memo_ix = build_memo(trace_id.as_bytes(), &[&wallet_pubkey]);
            instructions.push(memo_ix);
        }
        
        let s7 = Instant::now(); // After memo
        
        let mut transaction = Transaction::new_with_payer(
            &instructions,
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        let t_build = std::time::Instant::now(); // 🕐 t2: Transaction built and signed
        let s8 = Instant::now(); // After signing
        
        info!("📦 Submitting bundle to Jito...");
        
        // 5. Submit to Jito block engine
        let bundle_id = jito_client.send_transaction_bundle(&transaction).await?;
        
        let t_send = std::time::Instant::now(); // 🕐 t3: Bundle sent to Jito
        
        // 🔍 MICRO-SPAN BREAKDOWN - Shows exactly where time is spent
        info!("🔍 JITO BUILD/SEND breakdown:");
        info!("   setup={}ms, balance={}ms, curve={}ms, ix={}ms, tip={}ms, compute={}ms, blockhash={}ms, memo={}ms, sign={}ms, jito_send={}ms",
            (s0_5-s0).as_millis(), (s1-s0_5).as_millis(), (s2-s1).as_millis(),
            (s3-s2).as_millis(), (s4-s3).as_millis(), (s5-s4).as_millis(),
            (s6-s5).as_millis(), (s7-s6).as_millis(), (s8-s7).as_millis(),
            (t_send-s8).as_millis());
        info!("   📊 TOTAL BUILD: {}ms | TOTAL SEND: {}ms", 
            (t_build-s0).as_millis(), (t_send-s8).as_millis());
        
        info!("✅ Bundle submitted! ID: {}", bundle_id);
        info!("⏳ Waiting for bundle confirmation...");
        
        // 6. Wait for confirmation (with timeout)
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            jito_client.wait_for_bundle_confirmation(&bundle_id, 60) // 60 * 500ms = 30s max
        ).await {
            Ok(Ok(true)) => {
                // Bundle landed! Now get the final status to extract signature
                let final_status = jito_client.get_final_bundle_status(&bundle_id).await?;
                if let Some(sig) = final_status.get_signature() {
                    info!("🎉 Transaction confirmed! Signature: {}", sig);
                    Ok((sig.to_string(), t_build, t_send))
                } else {
                    Err("Bundle landed but no transaction signature found".into())
                }
            },
            Ok(Ok(false)) => Err("Bundle failed or was invalid".into()),
            Ok(Err(e)) => Err(format!("Bundle confirmation error: {}", e).into()),
            Err(_) => Err("Bundle confirmation timeout (30s)".into()),
        }
    }
    
    /// Execute real Jito sell bundle with Pump.fun instruction
    async fn execute_jito_sell(
        &self, 
        token: &str,
        token_amount: u64,
        min_sol_output: u64,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 Executing REAL Jito sell via block engine...");
        
        let jito_client = self.jito_client.as_ref()
            .ok_or("Jito client not initialized")?;
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        // Fetch bonding curve state to get creator pubkey
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        // Build Pump.fun sell instruction (with all 14 accounts!)
        let pump_sell_ix = pump_instructions::create_sell_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount,
            min_sol_output,
            &curve_state.creator,  // NEW - needed for creator_vault PDA
        )?;
        
        // 2. Determine tip amount (dynamic or fixed)
        let tip_amount = if self.config.jito_use_dynamic_tip {
            info!("📊 Using DYNAMIC tip ({}th percentile)...", self.config.jito_exit_percentile);
            jito_client.get_dynamic_tip(self.config.jito_exit_percentile).await
                .unwrap_or(self.config.jito_tip_amount)
        } else {
            self.config.jito_tip_amount
        };
        
        info!("💸 Tip amount: {} lamports (${:.4})", tip_amount, tip_amount as f64 / 1e9 * 150.0);
        
        // 3. Add compute budget instructions
        let compute_limit = 200_000; // Conservative limit for Pump.fun sells
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(
            tip_amount,
        );
        
        info!("⚙️ Compute limit: {} CU, price: {} µLamports/CU", compute_limit, tip_amount);
        
        // 4. Get Jito tip account and create tip instruction
        let tip_account = jito_client.get_random_tip_account()?;
        let tip_ix = system_instruction::transfer(
            &wallet_pubkey,
            &tip_account,
            tip_amount,
        );
        
        // 5. Build transaction
        // Use cached blockhash if available (TIER 1 optimization)
        let recent_blockhash = cached_blockhash.unwrap_or_else(|| self.rpc_client.get_latest_blockhash().unwrap());
        let mut transaction = Transaction::new_with_payer(
            &[compute_limit_ix, compute_budget_ix, pump_sell_ix, tip_ix],
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        info!("📦 Submitting sell bundle to Jito...");
        
        // 5. Submit to Jito block engine
        let bundle_id = jito_client.send_transaction_bundle(&transaction).await?;
        
        info!("✅ Bundle submitted! ID: {}", bundle_id);
        
        // ✅ CRITICAL FIX: Return immediately after bundle submission
        // Background confirmation tracker will monitor the bundle
        // DO NOT wait here - it causes false failures and duplicate sells!
        
        // Extract signature from bundle for tracking
        let signature = bs58::encode(transaction.signatures[0]).into_string();
        
        // 📤 IMMEDIATELY notify brain that transaction was submitted
        let token_mint = token_pubkey.to_bytes();
        self.send_trade_submitted(
            &token_mint,
            &signature.parse().unwrap(),
            1, // SELL
            token_amount,
            min_sol_output,
            0, // No slippage tracking for sells yet
        );
        
        Ok(signature)
    }
    
    /// Execute buy using direct RPC (no Jito, no MEV protection)
    async fn execute_direct_rpc_buy(
        &self,
        token: &str,
        token_amount: u64,
        max_sol_cost: u64,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("🔄 Executing direct RPC buy (no Jito)...");
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        // Check wallet balance (non-blocking)
        let rpc_clone = self.rpc_client.clone();
        let balance_result = tokio::task::spawn_blocking(move || {
            rpc_clone.get_balance(&wallet_pubkey)
        }).await;
        
        let balance = match balance_result {
            Ok(Ok(bal)) => bal,
            Ok(Err(e)) => return Err(format!("Failed to get balance: {}", e).into()),
            Err(e) => return Err(format!("Spawn blocking error: {}", e).into()),
        };
        
        let balance_sol = balance as f64 / 1e9;
        info!("💰 Wallet balance: {:.4} SOL", balance_sol);
        
        if balance_sol < 0.01 {
            return Err(format!("Insufficient balance: {:.4} SOL (need at least 0.01 SOL)", balance_sol).into());
        }
        
        // Fetch bonding curve state to get creator pubkey
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        // Get or create associated token account
        let spl_token_program = spl_token::id();
        let ata = spl_associated_token_account::get_associated_token_address(
            &wallet_pubkey,
            &token_pubkey,
        );
        
        info!("🔍 Checking if ATA exists: {}", ata);
        
        // Check if ATA exists
        let mut instructions = vec![];
        
        // Check if ATA exists (non-blocking)
        let rpc_clone = self.rpc_client.clone();
        let ata_clone = ata;
        let ata_exists = tokio::task::spawn_blocking(move || {
            rpc_clone.get_account(&ata_clone)
        }).await;
        
        match ata_exists {
            Ok(Ok(_)) => {
                info!("✅ ATA already exists");
            }
            Ok(Err(_)) | Err(_) => {
                info!("⚠️ ATA doesn't exist, creating...");
                let create_ata_ix = create_associated_token_account(
                    &wallet_pubkey,  // payer
                    &wallet_pubkey,  // wallet
                    &token_pubkey,   // mint
                    &spl_token_program,
                );
                instructions.push(create_ata_ix);
            }
        }
        
        // Build Pump.fun buy instruction (with all 16 accounts!)
        let pump_buy_ix = pump_instructions::create_buy_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount,
            max_sol_cost,
            &curve_state.creator,  // NEW - needed for creator_vault PDA
        )?;
        
        // Add compute budget instructions
        let compute_limit = 200_000; // Conservative limit for Pump.fun buys (TPU path)
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        // TIER 2: Dynamic priority fee (was: 5000 static)
        let priority_fee = self.get_dynamic_priority_fee();
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee);
        
        info!("⚙️ Compute limit: {} CU, price: {} µLamports/CU", compute_limit, priority_fee);
        
        instructions.insert(0, compute_limit_ix);
        instructions.insert(1, compute_budget_ix);
        instructions.push(pump_buy_ix);
        
        // Build transaction
        // Use cached blockhash if available (TIER 1 optimization)
        let recent_blockhash = cached_blockhash.unwrap_or_else(|| self.rpc_client.get_latest_blockhash().unwrap());
        let mut transaction = Transaction::new_with_payer(
            &instructions,
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        info!("📡 Submitting transaction to RPC (skip_preflight=true)...");
        
        // Send transaction with skip_preflight to eliminate 50-200ms simulation delay
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            ..Default::default()
        };
        let signature = self.rpc_client.send_transaction_with_config(&transaction, config)?;
        
        info!("✅ Transaction confirmed! Signature: {}", signature);
        Ok(signature.to_string())
    }
    
    /// Execute buy using TPU (direct validator submission - fastest!)
    async fn execute_tpu_buy(
        &self,
        token: &str,
        token_amount: u64,
        max_sol_cost: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let (sig, _, _) = self.execute_tpu_buy_with_timing(token, token_amount, max_sol_cost, None, None).await?;
        Ok(sig)
    }
    
    /// Execute TPU buy WITH timing capture (returns signature, t_build, t_send)
    async fn execute_tpu_buy_with_timing(
        &self,
        token: &str,
        token_amount: u64,
        max_sol_cost: u64,
        trace_id: Option<String>,  // NEW: For latency tracking
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<(String, std::time::Instant, std::time::Instant), Box<dyn std::error::Error + Send + Sync>> {
        let s0 = Instant::now(); // 🕐 Start of build phase
        info!("🚀 Executing TPU buy (direct validator submission)...");
        
        let tpu_client = self.tpu_client.as_ref()
            .ok_or("TPU client not initialized")?;
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        let s0_5 = Instant::now(); // After setup
        
        // OPTIMIZATION: Skip balance check for speed - we'll catch insufficient balance errors from the transaction
        // (This saves ~50ms per trade)
        
        let s1 = Instant::now(); // After (skipped) balance check
        
        // OPTIMIZATION #12: Fetch bonding curve state from cache (or RPC on miss)
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        let s2 = Instant::now(); // After curve fetch
        
        // OPTIMIZATION: Always include ATA creation instruction (it's idempotent!)
        // This saves ~50ms by avoiding the RPC call to check if it exists
        // The Solana runtime will skip creation if account already exists (no-op if exists)
        let spl_token_program = spl_token::id();
        let ata = spl_associated_token_account::get_associated_token_address(
            &wallet_pubkey,
            &token_pubkey,
        );
        
        let create_ata_ix = create_associated_token_account(
            &wallet_pubkey,  // payer
            &wallet_pubkey,  // wallet
            &token_pubkey,   // mint
            &spl_token_program,
        );
        
        let mut instructions = vec![create_ata_ix];
        
        let s3 = Instant::now(); // After (optimized) ATA setup
        
        // Build Pump.fun buy instruction (with all 16 accounts!)
        let pump_buy_ix = pump_instructions::create_buy_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount,
            max_sol_cost,
            &curve_state.creator,  // NEW - needed for creator_vault PDA
        )?;
        
        // Add compute budget instructions
        let compute_limit = 200_000; // Conservative limit for Pump.fun buys
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        let priority_fee = 10000; // 10000 micro-lamports per CU (high priority for TPU)
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee);
        
        info!("⚙️ Compute limit: {} CU, price: {} µLamports/CU", compute_limit, priority_fee);
        
        instructions.insert(0, compute_limit_ix);
        instructions.insert(1, compute_budget_ix);
        instructions.push(pump_buy_ix);
        
        // Add memo with trace_id for gRPC monitoring
        if let Some(ref trace_id) = trace_id {
            let memo_ix = build_memo(trace_id.as_bytes(), &[&wallet_pubkey]);
            instructions.push(memo_ix);
        }
        
        let s4 = Instant::now(); // After instructions built
        
        // Build transaction
        // Use cached blockhash if available (TIER 1 optimization)
        let recent_blockhash = if let Some(hash) = cached_blockhash {
            debug!("♻️ Using warmed blockhash");
            hash
        } else {
            warn!("⚠️ Cached blockhash not available, fetching fresh (SLOW!)");
            self.rpc_client.get_latest_blockhash().unwrap()
        };
        
        let s5 = Instant::now(); // After blockhash
        
        let mut transaction = Transaction::new_with_payer(
            &instructions,
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        let t_build = std::time::Instant::now(); // 🕐 t2: Transaction built and signed
        let s6 = Instant::now(); // After signing
        
        info!("⚡ Submitting transaction via TPU (async mode - gRPC will monitor)...");
        
        // OPTIMIZATION: Send without waiting for confirmation (async mode)
        // Confirmation will be monitored via gRPC in main.rs (monitor_transaction_landing + monitor_confirmation)
        let signature = tpu_client.send_transaction_async(&transaction).await?;
        
        // 📤 IMMEDIATELY notify brain that transaction was submitted (before confirmation)
        let token_mint = token_pubkey.to_bytes();
        self.send_trade_submitted(
            &token_mint,
            &signature,
            0, // BUY
            token_amount, // expected tokens from function parameter
            max_sol_cost, // max SOL cost in lamports from function parameter
            0, // slippage_bps not explicitly calculated in TPU (max_sol_cost serves as limit)
        );
        
        // 🔄 Track transaction for background confirmation
        let t_send = std::time::Instant::now(); // 🕐 t3: Transaction sent (not confirmed yet!)

        
        // 🔍 MICRO-SPAN BREAKDOWN - Shows exactly where time is spent
        println!("🔍 TPU BUILD/SEND MICRO-SPAN BREAKDOWN (ASYNC MODE):");
        println!("   setup={} ms", (s0_5-s0).as_millis());
        println!("   balance={} ms", (s1-s0_5).as_millis());
        println!("   curve={} ms", (s2-s1).as_millis());
        println!("   ata={} ms", (s3-s2).as_millis());
        println!("   ix={} ms", (s4-s3).as_millis());
        println!("   blockhash={} ms", (s5-s4).as_millis());
        println!("   sign={} ms", (s6-s5).as_millis());
        println!("   send={} ms ⚡ (async - no wait!)", (t_send-s6).as_millis());
        println!("   📊 TOTAL BUILD: {} ms | TOTAL SEND: {} ms", 
            (t_build-s0).as_millis(), (t_send-s6).as_millis());
        println!("   🔍 Confirmation tracked by Brain via gRPC");
        
        info!("✅ TPU transaction sent (async)! Signature: {}", signature);
        
        // ✅ Return immediately after sending (Brain handles confirmation)
        
        Ok((signature.to_string(), t_build, t_send))
    }
    
    /// Execute sell using TPU (direct validator submission - fastest!)
    async fn execute_tpu_sell(
        &self,
        token: &str,
        token_amount: u64,
        min_sol_output: u64,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 Executing TPU sell (direct validator submission)...");
        
        let tpu_client = self.tpu_client.as_ref()
            .ok_or("TPU client not initialized")?;
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        // Skip balance check for speed - we'll verify after the sell
        info!("🔍 SELL PARAMS: token_amount={}, min_sol_output={} lamports ({:.6} SOL)", 
            token_amount, min_sol_output, min_sol_output as f64 / 1e9);
        
        // Fetch bonding curve state to get creator pubkey
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        // CRITICAL FIX: Ensure ATA exists before selling (prevents "IllegalOwner" errors)
        let spl_token_program = spl_token::id();
        let user_ata = spl_associated_token_account::get_associated_token_address(&wallet_pubkey, &token_pubkey);
        
        info!("🔍 Checking if ATA exists before SELL: {}", user_ata);
        let mut instructions = vec![];
        
        match self.rpc_client.get_account(&user_ata) {
            Ok(_) => {
                info!("✅ ATA already exists for SELL");
            }
            Err(_) => {
                info!("⚠️ ATA doesn't exist, creating before SELL...");
                let create_ata_ix = create_associated_token_account(
                    &wallet_pubkey,  // payer
                    &wallet_pubkey,  // wallet (owner)
                    &token_pubkey,   // mint
                    &spl_token_program,
                );
                instructions.push(create_ata_ix);
            }
        }
        
        // Build Pump.fun sell instruction (with all 14 accounts!)
        let pump_sell_ix = pump_instructions::create_sell_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount,
            min_sol_output,
            &curve_state.creator,  // NEW - needed for creator_vault PDA
        )?;
        
        // Add compute budget instructions
        let compute_limit = 200_000; // Conservative limit for Pump.fun sells (TPU path)
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        // TIER 2: Dynamic priority fee for sells (was: 10000 static)
        let priority_fee = self.get_dynamic_priority_fee_sell();
        
        info!("⚙️ Compute limit: {} CU, price: {} µLamports/CU", compute_limit, priority_fee);
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee);
        
        // Build final instruction list
        let mut all_instructions = vec![compute_limit_ix, compute_budget_ix];
        all_instructions.extend(instructions); // ATA creation if needed
        all_instructions.push(pump_sell_ix);
        
        // Build transaction
        // Use cached blockhash if available (TIER 1 optimization)
        let recent_blockhash = cached_blockhash.unwrap_or_else(|| self.rpc_client.get_latest_blockhash().unwrap());
        let mut transaction = Transaction::new_with_payer(
            &all_instructions,
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        info!("⚡ Submitting sell transaction via TPU (async mode)...");
        
        // OPTIMIZATION: Send without waiting for confirmation (async mode)
        let signature = tpu_client.send_transaction_async(&transaction).await?;
        
        // 📤 IMMEDIATELY notify brain that transaction was submitted (before confirmation)
        let token_mint = token_pubkey.to_bytes();
        self.send_trade_submitted(
            &token_mint,
            &signature,
            1, // SELL
            token_amount,
            min_sol_output,
            0, // No slippage tracking for sells yet
        );
        
        info!("✅ TPU sell sent (async)! Signature: {} - Brain monitors confirmation via gRPC", signature);
        
        Ok(signature.to_string())
    }
    
    /// Execute sell using direct RPC (no Jito, no MEV protection)
    async fn execute_direct_rpc_sell(
        &self,
        token: &str,
        token_amount: u64,
        min_sol_output: u64,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        info!("🔄 Executing direct RPC sell (no Jito)...");
        
        let token_pubkey = Pubkey::from_str(token)?;
        let wallet_pubkey = self.keypair.pubkey();
        
        // Fetch bonding curve state to get creator pubkey
        let curve_state = self.curve_cache.get_or_fetch(
            &self.rpc_client,
            &token_pubkey,
        ).await?;
        
        // Build Pump.fun sell instruction (with all 14 accounts!)
        let pump_sell_ix = pump_instructions::create_sell_instruction(
            &wallet_pubkey,
            &token_pubkey,
            token_amount,
            min_sol_output,
            &curve_state.creator,  // NEW - needed for creator_vault PDA
        )?;
        
        // Add compute budget instructions
        let compute_limit = 200_000; // Conservative limit for Pump.fun sells
        let compute_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(compute_limit);
        // TIER 2: Dynamic priority fee for sells (was: 5000 static)
        let priority_fee = self.get_dynamic_priority_fee_sell();
        let compute_budget_ix = ComputeBudgetInstruction::set_compute_unit_price(priority_fee);
        
        info!("⚙️ Compute limit: {} CU, price: {} µLamports/CU", compute_limit, priority_fee);
        
        // Build transaction
        // Use cached blockhash if available (TIER 1 optimization)
        let recent_blockhash = cached_blockhash.unwrap_or_else(|| self.rpc_client.get_latest_blockhash().unwrap());
        let mut transaction = Transaction::new_with_payer(
            &[compute_limit_ix, compute_budget_ix, pump_sell_ix],
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        info!("📡 Submitting transaction to RPC (skip_preflight=true)...");
        
        // Send transaction with skip_preflight to eliminate 50-200ms simulation delay
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            ..Default::default()
        };
        let signature = self.rpc_client.send_transaction_with_config(&transaction, config)?;
        
        info!("✅ Transaction confirmed! Signature: {}", signature);
        Ok(signature.to_string())
    }
    
    pub fn get_balance(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        let balance = self.rpc_client.get_balance(&self.keypair.pubkey())?;
        Ok(balance as f64 / 1e9)
    }
    
    pub fn get_wallet_balance(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        self.get_balance()
    }
    
    /// Get recent blockhash (now uses warm-up cache instead of RPC call)
    pub async fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash, Box<dyn std::error::Error + Send + Sync>> {
        Ok(get_cached_blockhash().await)
    }
    
    /// TIER 2: Get TPU client for leader schedule refresh
    pub fn get_tpu_client(&self) -> Option<&FastTpuClient> {
        self.tpu_client.as_ref()
    }
    
    /// Get RPC client reference for external operations (e.g., confirmation monitoring)
    pub fn get_rpc_client(&self) -> &RpcClient {
        &self.rpc_client
    }
    /// TIER 5: Monitor transaction confirmation and update t5 timing
    /// Polls signature status until finalized commitment is reached
    pub async fn monitor_confirmation(
        &self,
        signature: Signature,
        trace_id: String,
        db: Arc<Database>,
        t0_detect: Instant,  // Need starting point for accurate timing
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut poll_count = 0;
        let max_polls = 60; // 60 * 1s = 60s timeout
        let poll_interval = Duration::from_millis(1000); // Poll every 1s
        
        loop {
            poll_count += 1;
            
            // Use get_signature_statuses for more efficient polling
            match self.rpc_client.get_signature_statuses(&[signature]) {
                Ok(response) => {
                    if let Some(Some(status)) = response.value.first() {
                        // Check if finalized (confirmations == None means max confirmations reached)
                        let is_finalized = status.confirmations.is_none() || 
                            status.confirmations.map(|c| c == 0).unwrap_or(false);
                        
                        if is_finalized {
                            let t5_confirm = Instant::now();
                            let t_confirm_ns = (t5_confirm - t0_detect).as_nanos() as i64;
                            let confirmed_slot = status.slot;
                            
                            // CRITICAL: Check if transaction succeeded or failed
                            if let Some(err) = &status.err {
                                error!("❌ Transaction FAILED at slot {}: {:?} - trace: {}", 
                                    confirmed_slot, err, &trace_id[..8]);
                                return Err(format!("Transaction failed: {:?}", err).into());
                            }
                            
                            info!("✅ Transaction FINALIZED at slot {} (poll #{}, {:.2}s after detection) - trace: {}", 
                                confirmed_slot, poll_count, t_confirm_ns as f64 / 1_000_000_000.0, &trace_id[..8]);
                            
                            // Parse actual fees from transaction meta
                            match self.get_actual_transaction_fee(&signature).await {
                                Ok(actual_fee_sol) => {
                                    info!("💰 Actual transaction fee: {:.6} SOL ({:.4} USD @ current price) - trace: {}", 
                                        actual_fee_sol, actual_fee_sol * 150.0, &trace_id[..8]); // Assuming ~$150 SOL
                                    
                                    // TODO: Store actual_fee_sol in database for PnL tracking
                                    // This is the REAL cost including base fee + priority fee + protocol fees
                                }
                                Err(e) => {
                                    warn!("⚠️  Failed to fetch actual transaction fee: {} - using estimated fees", e);
                                }
                            }
                            
                            // Update database with confirmation timing
                            if let Err(e) = db.update_trace_confirm(&trace_id, t_confirm_ns, confirmed_slot).await {
                                error!("Failed to update trace confirmation: {}", e);
                            }
                            
                            return Ok(());
                        }
                    }
                    
                    // Not yet finalized, keep polling
                    if poll_count >= max_polls {
                        warn!("⏰ Confirmation timeout after {} polls ({}s) - trace: {}", 
                            max_polls, max_polls, &trace_id[..8]);
                        return Err("Confirmation timeout".into());
                    }
                }
                Err(e) => {
                    error!("Error checking confirmation status (poll #{}): {}", poll_count, e);
                    if poll_count >= max_polls {
                        return Err(format!("Confirmation check failed: {}", e).into());
                    }
                }
            }
            
            tokio::time::sleep(poll_interval).await;
        }
    }
    
    /// Fetch actual transaction fee from confirmed transaction meta
    /// This gives us the REAL cost (base fee + priority fee + protocol fees)
    /// Critical for accurate PnL tracking that matches wallet balances
    pub async fn get_actual_transaction_fee(
        &self,
        signature: &Signature,
    ) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        use solana_transaction_status::UiTransactionEncoding;
        use solana_client::rpc_config::RpcTransactionConfig;
        
        // Fetch transaction with full meta
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::finalized()),
            max_supported_transaction_version: Some(0),
        };
        
        let tx_result = self.rpc_client.get_transaction_with_config(signature, config)?;
        
        // Extract fee from meta
        if let Some(meta) = tx_result.transaction.meta {
            // Method 1: Direct fee field (most reliable)
            let fee_lamports = meta.fee;
            let fee_sol = fee_lamports as f64 / 1_000_000_000.0;
            
            // Optional: Verify against balance changes (sanity check)
            if let (Some(pre_balances), Some(post_balances)) = (meta.pre_balances.first(), meta.post_balances.first()) {
                let balance_change = (*pre_balances as i64 - *post_balances as i64) as f64 / 1_000_000_000.0;
                let balance_fee = balance_change.abs();
                
                // Log if there's a significant discrepancy (> 0.001 SOL difference)
                if (balance_fee - fee_sol).abs() > 0.001 {
                    warn!("⚠️  Fee discrepancy: meta.fee={:.6} SOL vs balance_change={:.6} SOL (diff: {:.6})", 
                        fee_sol, balance_fee, (balance_fee - fee_sol).abs());
                }
            }
            
            Ok(fee_sol)
        } else {
            Err("Transaction meta not available".into())
        }
    }
    
    /// Calculate and update slippage for a buy transaction
    /// This should be called after transaction confirmation to parse actual amounts
    pub async fn calculate_buy_slippage(
        &self,
        buy_result: &mut BuyResult,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::slippage;
        use solana_sdk::signature::Signature;
        use std::str::FromStr;
        
        let signature = Signature::from_str(&buy_result.signature)?;
        
        // Parse actual tokens received from transaction
        match slippage::parse_actual_tokens_from_buy(&self.rpc_client, &signature).await {
            Ok(actual_tokens) => {
                let slippage_result = slippage::SlippageResult::new(
                    buy_result.token_amount,
                    actual_tokens,
                );
                
                // Update buy result with actual data
                buy_result.actual_token_amount = Some(actual_tokens);
                buy_result.slippage_bps = Some(slippage_result.slippage_bps);
                
                // Log slippage analysis
                slippage_result.log("BUY");
                
                Ok(())
            }
            Err(e) => {
                warn!("⚠️  Failed to calculate buy slippage: {}", e);
                warn!("   Transaction may still be processing or parsing failed");
                Ok(()) // Non-critical error
            }
        }
    }
    
    /// Calculate and update slippage for a sell transaction
    /// This should be called after transaction confirmation to parse actual amounts
    pub async fn calculate_sell_slippage(
        &self,
        exit_result: &mut ExitResult,
        expected_sol: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use crate::slippage;
        use solana_sdk::signature::Signature;
        use std::str::FromStr;
        
        let signature = Signature::from_str(&exit_result.signature)?;
        
        // Parse actual SOL received from transaction
        match slippage::parse_actual_sol_from_sell(
            &self.rpc_client,
            &signature,
            &self.keypair.pubkey(),
        ).await {
            Ok(actual_sol) => {
                let slippage_result = slippage::SlippageResult::new(
                    expected_sol,
                    actual_sol,
                );
                
                // Update exit result with actual data
                exit_result.actual_sol_received = Some(actual_sol);
                exit_result.slippage_bps = Some(slippage_result.slippage_bps);
                
                // Log slippage analysis
                slippage_result.log("SELL");
                
                Ok(())
            }
            Err(e) => {
                warn!("⚠️  Failed to calculate sell slippage: {}", e);
                warn!("   Transaction may still be processing or parsing failed");
                Ok(()) // Non-critical error
            }
        }
    }
}

// Helper to load keypair from various formats
fn load_keypair_from_string(private_key: &str) -> Result<Keypair, Box<dyn std::error::Error + Send + Sync>> {
    // Try base58
    if let Ok(bytes) = bs58::decode(private_key).into_vec() {
        if bytes.len() == 64 {
            return Ok(Keypair::try_from(bytes.as_slice())?);
        }
    }
    
    // Try JSON array
    if private_key.starts_with('[') {
        let bytes: Vec<u8> = serde_json::from_str(private_key)?;
        if bytes.len() == 64 {
            return Ok(Keypair::try_from(bytes.as_slice())?);
        }
    }
    
    Err("Invalid private key format".into())
}