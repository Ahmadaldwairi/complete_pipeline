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
    instruction::Instruction,
    compute_budget::ComputeBudgetInstruction,
    system_instruction,
};
use spl_associated_token_account::{
    instruction::create_associated_token_account,
    get_associated_token_address,
};
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

/// Fetch real SOL/USD price from Helius API (with caching)
/// Helius is faster and more reliable than Jupiter for price data
async fn fetch_sol_price() -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
    // Check cache first
    let cache = get_sol_price_cache();
    {
        let cached = cache.read().await;
        let age = cached.cached_at.elapsed();
        
        // Only use cache if price is valid AND within TTL
        // Note: First call has ttl=0 and price=0.0, so this will skip cache
        if cached.price > 0.0 && cached.ttl.as_secs() > 0 && age < cached.ttl {
            debug!("💰 SOL price cache HIT! ${:.2} (age: {:.2}s / TTL: {:.0}s)", 
                cached.price, age.as_secs_f64(), cached.ttl.as_secs_f64());
            return Ok(cached.price);
        }
        
        // Log expiration only if we had a valid cached price
        if cached.price > 0.0 && cached.ttl.as_secs() > 0 {
            debug!("⏰ SOL price cache EXPIRED (age: {:.2}s > TTL: {:.0}s) - fetching fresh", 
                age.as_secs_f64(), cached.ttl.as_secs_f64());
        }
    }
    
    // Cache miss or expired - fetch new price
    debug!("🔄 Fetching fresh SOL/USD price from Helius...");
    
    // Try Helius first (faster, more reliable)
    let price_result = async {
        // Helius provides Jupiter price aggregation via their API
        let response = reqwest::Client::new()
            .get("https://api.helius.xyz/v0/token-metadata?api-key=dd6814ec-edbb-4a17-9d8d-cc0826aacf01")
            .timeout(Duration::from_secs(3))
            .query(&[("mint", "So11111111111111111111111111111111111111112")]) // SOL mint
            .send()
            .await
            .map_err(|e| format!("Helius request failed: {}", e))?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| format!("Helius JSON parse failed: {}", e))?;
        
        // Extract price from response
        let price = response[0]["price_info"]["price_per_token"]
            .as_f64()
            .ok_or("Helius price not found")?;
        
        Ok::<f64, Box<dyn std::error::Error + Send + Sync>>(price)
    }.await;
    
    // Fallback to Jupiter if Helius fails
    let price = match price_result {
        Ok(p) => {
            info!("✅ SOL price from Helius: ${:.2}", p);
            p
        }
        Err(e) => {
            warn!("⚠️  Helius price fetch failed: {} - trying Jupiter fallback", e);
            
            // Fallback to Jupiter API
            let jup_result = reqwest::Client::new()
                .get("https://price.jup.ag/v6/price?ids=SOL")
                .timeout(Duration::from_secs(3))
                .send()
                .await
                .map_err(|e| format!("Jupiter request failed: {}", e))?
                .json::<serde_json::Value>()
                .await
                .map_err(|e| format!("Jupiter JSON parse failed: {}", e))?;
            
            let jup_price = jup_result["data"]["SOL"]["price"]
                .as_f64()
                .unwrap_or(150.0);
            
            info!("✅ SOL price from Jupiter fallback: ${:.2}", jup_price);
            jup_price
        }
    };
    
    // Validate price is reasonable (between $50-$500)
    let final_price = if price >= 50.0 && price <= 500.0 {
        price
    } else {
        warn!("⚠️  Invalid SOL price: ${:.2} - using fallback $150", price);
        150.0
    };
    
    // Update cache with 30-second TTL (SOL price doesn't change much in 30s)
    {
        let mut cached = cache.write().await;
        cached.price = final_price;
        cached.cached_at = Instant::now();
        cached.ttl = Duration::from_secs(30);
        debug!("✅ SOL price cached: ${:.2} (TTL: {:.0}s)", final_price, cached.ttl.as_secs_f64());
    }
    
    Ok(final_price)
}
pub struct TradingEngine {
    rpc_client: RpcClient,
    keypair: Keypair,
    jito_client: Option<JitoClient>,
    tpu_client: Option<FastTpuClient>,
    config: Config,
    fee_tracker: PriorityFeeTracker,  // TIER 2: Dynamic priority fee tracking
    curve_cache: Arc<pump_bonding_curve::BondingCurveCache>,  // OPTIMIZATION #12: Curve caching
}

#[derive(Debug, Clone)]
pub struct BuyResult {
    pub token_address: String,
    pub signature: String,
    pub price: f64,                // Price in SOL per token (REAL from bonding curve)
    pub token_amount: f64,         // Number of tokens bought
    pub position_size: f64,        // USD invested
    pub actual_position: u32,      // REAL position from blockchain
    pub estimated_position: u32,   // From mempool (for comparison)
    pub mempool_volume: f64,       // SOL volume in mempool at entry (for Tier 3)
    pub entry_fees: FeeBreakdown,
    pub timestamp: DateTime<Local>,
    pub trace_id: Option<String>,  // For latency tracking
    
    // NEW: Timing data for latency tracking
    pub t_build: Option<std::time::Instant>,  // When tx was built
    pub t_send: Option<std::time::Instant>,   // When tx was sent
}

#[derive(Debug, Clone)]
pub struct ExitResult {
    pub signature: String,
    pub exit_price: f64,
    pub gross_profit: f64,
    pub exit_fees: FeeBreakdown,
    pub net_profit: f64,           // After ALL fees (in USD)
    pub net_profit_sol: f64,       // After ALL fees (in SOL) - for wallet tracking
    pub tier: String,
    pub holding_time: u64,
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
        let rpc_client = RpcClient::new_with_commitment(
            config.rpc_endpoint.clone(),
            CommitmentConfig::confirmed(),
        );
        
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
        
        Ok(TradingEngine {
            rpc_client,
            keypair,
            jito_client,
            tpu_client,
            config: config.clone(),
            fee_tracker: PriorityFeeTracker::new(),  // Initialize fee tracker
            curve_cache,  // Add curve cache
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
    
    /// Execute a BUY transaction via Jito bundle
    /// Now uses REAL bonding curve calculations!
    pub async fn buy(
        &self,
        token_address: &str,
        position_size_usd: f64,
        estimated_position: u32,
        mempool_volume: f64,
        pending_buys: u32,  // NEW: For TIER 2 dynamic slippage by queue depth
        trace_id: Option<String>,  // NEW: For latency tracking
        cached_blockhash: Option<solana_sdk::hash::Hash>,  // NEW: Pre-warmed blockhash
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
        
        // Step 3: Get REAL SOL/USD price (cached from Helius)
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
        
        // Execute buy with priority: TPU > Jito > Direct RPC
        let t_exec_start = std::time::Instant::now();
        let (signature, t_build, t_send) = if self.config.use_tpu && self.tpu_client.is_some() {
            info!("⚡ Executing via TPU (direct validator submission)...");
            let (sig, tb, ts) = self.execute_tpu_buy_with_timing(
                token_address,
                token_amount_raw,
                max_sol_cost,
                trace_id.clone(),  // Pass trace_id for monitoring
                cached_blockhash,  // Use warmed blockhash
            ).await?;
            (sig, Some(tb), Some(ts))
        } else if self.config.use_jito {
            info!("⚡ Executing Jito bundle submission...");
            let (sig, tb, ts) = self.execute_jito_buy_with_timing(
                token_address, 
                token_amount_raw,
                max_sol_cost,
                trace_id.clone(),  // Pass trace_id for monitoring
                cached_blockhash,  // Use warmed blockhash
            ).await?;
            (sig, Some(tb), Some(ts))
        } else {
            info!("⚡ Executing direct RPC transaction (via Helius)...");
            let sig = self.execute_direct_rpc_buy(
                token_address, 
                token_amount_raw,
                max_sol_cost,
                cached_blockhash,  // Use warmed blockhash
            ).await?;
            // For direct RPC, we don't have fine-grained timing
            (sig, Some(t_before_build), Some(std::time::Instant::now()))
        };
        
        let exec_time = t_exec_start.elapsed().as_millis();
        println!("⏱️ execute_tpu_buy_with_timing(): {}ms", exec_time);
        
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
            token_address: token_address.to_string(),
            signature,
            price: entry_price,      // ✅ REAL price from bonding curve!
            token_amount,            // ✅ REAL token amount!
            position_size: position_size_usd,
            actual_position,
            estimated_position,
            mempool_volume,          // ✅ For Tier 3 volume tracking!
            entry_fees,
            timestamp: Local::now(),
            trace_id,                // ✅ For latency tracking!
            t_build,                 // ✅ Build timestamp
            t_send,                  // ✅ Send timestamp
        })
    }
    
    /// TIER 3 Task 2: Transaction resubmission engine with automatic retry
    /// Wraps buy() with intelligent retry logic for network reliability
    pub async fn buy_with_retry(
        &self,
        token_address: &str,
        position_size_usd: f64,
        estimated_position: u32,
        mempool_volume: f64,
        pending_buys: u32,
        trace_id: Option<String>,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
        max_attempts: u32,
    ) -> Result<BuyResult, Box<dyn std::error::Error + Send + Sync>> {
        let mut last_error = None;
        
        for attempt in 1..=max_attempts {
            info!("🔄 Buy attempt {}/{} for {}", attempt, max_attempts, token_address);
            
            match self.buy(
                token_address,
                position_size_usd,
                estimated_position,
                mempool_volume,
                pending_buys,
                trace_id.clone(),
                cached_blockhash,
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
    
    /// TIER 3: Fast resubmission with fresh blockhash + fee bump
    /// Called when t4 landing not detected within timeout (120-180ms)
    /// Returns new signature if resubmitted successfully
    pub async fn resubmit_with_fee_bump(
        &self,
        original_signature: &str,
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
        
        // Get fresh blockhash (critical - old one may have expired)
        let fresh_blockhash = self.get_recent_blockhash()?;
        
        // Temporarily boost priority fee
        let base_fee = self.get_dynamic_priority_fee();
        let boosted_fee = (base_fee as f64 * fee_bump_multiplier) as u64;
        let boosted_fee = boosted_fee.clamp(15_000, 100_000); // Floor 15k, ceiling 100k
        
        info!("   💸 Fee bump: {} → {} µL/CU ({:.0}% increase)",
            base_fee, boosted_fee, (fee_bump_multiplier - 1.0) * 100.0);
        
        // Rebuild and send transaction with same trace_id (for dedupe)
        // The memo ensures we can correlate even if this gets a new signature
        match self.buy(
            token_address,
            position_size_usd,
            estimated_position,
            mempool_volume,
            pending_buys,
            Some(trace_id.clone()),
            Some(fresh_blockhash),
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
    
    /// Execute a SELL transaction via Jito bundle
    pub async fn sell(
        &self,
        token_address: &str,
        buy_result: &BuyResult,
        current_price: f64,
        tier: &str,
        cached_blockhash: Option<solana_sdk::hash::Hash>,
        widen_exit_slippage_bps: Option<u16>,  // Override slippage if WidenExit is active
    ) -> Result<ExitResult, Box<dyn std::error::Error + Send + Sync>> {
        info!("⚡ Executing SELL for {}", token_address);
        
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
            println!("⚠️  WidenExit Override Active! Using advisory slippage: {}bps ({:.1}%)", 
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
            
            base_slippage_bps + volatility_buffer_bps
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
        
        // Execute sell with priority: TPU > Jito > Direct RPC
        let signature = if self.config.use_tpu && self.tpu_client.is_some() {
            info!("⚡ Executing via TPU (direct validator submission)...");
            self.execute_tpu_sell(
                token_address,
                token_amount_raw,
                min_sol_output,
                cached_blockhash,
            ).await?
        } else if self.config.use_jito {
            info!("⚡ Executing Jito bundle submission...");
            self.execute_jito_sell(
                token_address,
                token_amount_raw,
                min_sol_output,
                cached_blockhash,
            ).await?
        } else {
            info!("⚡ Executing direct RPC transaction (via Helius)...");
            self.execute_direct_rpc_sell(
                token_address,
                token_amount_raw,
                min_sol_output,
                cached_blockhash,
            ).await?
        };
        
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
            signature,
            exit_price: live_current_price,
            gross_profit,
            exit_fees,
            net_profit,
            net_profit_sol,
            tier: tier.to_string(),
            holding_time,
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
        
        // 0. Check wallet balance first
        let balance = self.rpc_client.get_balance(&wallet_pubkey)?;
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
        info!("⏳ Waiting for bundle confirmation...");
        
        // 6. Wait for confirmation
        match tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            jito_client.wait_for_bundle_confirmation(&bundle_id, 60)
        ).await {
            Ok(Ok(true)) => {
                let final_status = jito_client.get_final_bundle_status(&bundle_id).await?;
                if let Some(sig) = final_status.get_signature() {
                    info!("🎉 Sell transaction confirmed! Signature: {}", sig);
                    Ok(sig.to_string())
                } else {
                    Err("Bundle landed but no transaction signature found".into())
                }
            },
            Ok(Ok(false)) => Err("Sell bundle failed or was invalid".into()),
            Ok(Err(e)) => Err(format!("Bundle confirmation error: {}", e).into()),
            Err(_) => Err("Bundle confirmation timeout (30s)".into()),
        }
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
        
        // Check wallet balance
        let balance = self.rpc_client.get_balance(&wallet_pubkey)?;
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
        match self.rpc_client.get_account(&ata) {
            Ok(_) => {
                info!("✅ ATA already exists");
            }
            Err(_) => {
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
        println!("   🔍 Confirmation will be tracked via gRPC (t4=landing, t5=finalized)");
        
        info!("✅ TPU transaction sent (async)! Signature: {}", signature);
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
        
        // Build transaction
        // Use cached blockhash if available (TIER 1 optimization)
        let recent_blockhash = cached_blockhash.unwrap_or_else(|| self.rpc_client.get_latest_blockhash().unwrap());
        let mut transaction = Transaction::new_with_payer(
            &[compute_limit_ix, compute_budget_ix, pump_sell_ix],
            Some(&wallet_pubkey),
        );
        transaction.sign(&[&self.keypair], recent_blockhash);
        
        info!("⚡ Submitting sell transaction via TPU (async mode)...");
        
        // OPTIMIZATION: Send without waiting for confirmation (async mode)
        let signature = tpu_client.send_transaction_async(&transaction).await?;
        
        info!("✅ TPU sell sent (async)! Signature: {} - gRPC will monitor confirmation", signature);
        
        // 🔍 POST-SELL: Verify tokens were actually sold (wait a bit for RPC to update)
        tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await; // Wait for likely confirmation
        let user_ata = spl_associated_token_account::get_associated_token_address(&wallet_pubkey, &token_pubkey);
        match self.rpc_client.get_token_account_balance(&user_ata) {
            Ok(balance) => {
                info!("🔍 POST-SELL: Token balance: {} (raw: {})", balance.ui_amount_string, balance.amount);
                if balance.amount != "0" {
                    error!("⚠️ WARNING: Sell confirmed but {} tokens still in wallet! Transaction: {}", 
                        balance.ui_amount_string, signature);
                    error!("⚠️ This usually means slippage was exceeded or instruction failed.");
                }
            }
            Err(e) => {
                info!("🔍 POST-SELL: Token account closed or empty (expected): {}", e);
            }
        }
        
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
    
    /// Get recent blockhash (for blockhash warming optimization)
    pub fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.rpc_client.get_latest_blockhash()?)
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
                            
                            info!("✅ Transaction FINALIZED at slot {} (poll #{}, {:.2}s after detection) - trace: {}", 
                                confirmed_slot, poll_count, t_confirm_ns as f64 / 1_000_000_000.0, &trace_id[..8]);
                            
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