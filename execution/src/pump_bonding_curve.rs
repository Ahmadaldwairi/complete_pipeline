use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use log::info;

// Pump.fun program constants
pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const PUMP_CURVE_SEED: &[u8] = b"bonding-curve";
pub const PUMP_CURVE_STATE_SIGNATURE: [u8; 8] = [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60];

// Bonding curve field offsets
const OFFSET_VIRTUAL_TOKEN_RESERVES: usize = 0x08;
const OFFSET_VIRTUAL_SOL_RESERVES: usize = 0x10;
const OFFSET_REAL_TOKEN_RESERVES: usize = 0x18;
const OFFSET_REAL_SOL_RESERVES: usize = 0x20;
const OFFSET_TOKEN_TOTAL_SUPPLY: usize = 0x28;
const OFFSET_COMPLETE: usize = 0x30;
const OFFSET_CREATOR: usize = 0x31;  // After complete (1 byte bool)

const PUMP_CURVE_TOKEN_DECIMALS: u32 = 6;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

#[derive(Debug, Clone)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Pubkey,  // NEW - needed for creator_vault PDA
}

impl BondingCurveState {
    /// Parse bonding curve state from raw account data
    pub fn from_account_data(data: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if data.len() < 81 {  // 8 + 8 + 8 + 8 + 8 + 8 + 1 + 32 = 81 bytes minimum
            return Err("Account data too small".into());
        }
        
        // Verify signature
        let signature = &data[0..8];
        if signature != PUMP_CURVE_STATE_SIGNATURE {
            return Err("Invalid bonding curve signature".into());
        }
        
        // Read u64 values (little-endian)
        let virtual_token_reserves = u64::from_le_bytes(
            data[OFFSET_VIRTUAL_TOKEN_RESERVES..OFFSET_VIRTUAL_TOKEN_RESERVES + 8]
                .try_into()?
        );
        let virtual_sol_reserves = u64::from_le_bytes(
            data[OFFSET_VIRTUAL_SOL_RESERVES..OFFSET_VIRTUAL_SOL_RESERVES + 8]
                .try_into()?
        );
        let real_token_reserves = u64::from_le_bytes(
            data[OFFSET_REAL_TOKEN_RESERVES..OFFSET_REAL_TOKEN_RESERVES + 8]
                .try_into()?
        );
        let real_sol_reserves = u64::from_le_bytes(
            data[OFFSET_REAL_SOL_RESERVES..OFFSET_REAL_SOL_RESERVES + 8]
                .try_into()?
        );
        let token_total_supply = u64::from_le_bytes(
            data[OFFSET_TOKEN_TOTAL_SUPPLY..OFFSET_TOKEN_TOTAL_SUPPLY + 8]
                .try_into()?
        );
        let complete = data[OFFSET_COMPLETE] != 0;
        
        // Read creator pubkey (32 bytes)
        let creator_bytes: [u8; 32] = data[OFFSET_CREATOR..OFFSET_CREATOR + 32]
            .try_into()?;
        let creator = Pubkey::from(creator_bytes);
        
        Ok(BondingCurveState {
            virtual_token_reserves,
            virtual_sol_reserves,
            real_token_reserves,
            real_sol_reserves,
            token_total_supply,
            complete,
            creator,
        })
    }
    
    /// Calculate token price in SOL using constant product formula
    /// Price = virtual_sol_reserves / virtual_token_reserves
    pub fn calculate_price(&self) -> f64 {
        if self.virtual_token_reserves == 0 || self.virtual_sol_reserves == 0 {
            return 0.0;
        }
        
        let sol_in_lamports = self.virtual_sol_reserves as f64;
        let tokens_in_base_units = self.virtual_token_reserves as f64;
        
        // Convert to human-readable units
        let sol_amount = sol_in_lamports / LAMPORTS_PER_SOL as f64;
        let token_amount = tokens_in_base_units / 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32);
        
        // Price = SOL per token
        sol_amount / token_amount
    }
    
    /// Calculate how many tokens you get for a given SOL amount
    /// Uses constant product formula: x * y = k
    pub fn calculate_buy_tokens(&self, sol_amount: f64) -> f64 {
        if self.complete {
            return 0.0;
        }
        
        let sol_lamports = (sol_amount * LAMPORTS_PER_SOL as f64) as u64;
        
        // Constant product: k = virtual_sol_reserves * virtual_token_reserves
        let k = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);
        
        // New SOL reserves after buy
        let new_sol_reserves = self.virtual_sol_reserves + sol_lamports;
        
        // Calculate new token reserves: new_token_reserves = k / new_sol_reserves
        let new_token_reserves = (k / new_sol_reserves as u128) as u64;
        
        // Tokens to receive
        let tokens_base_units = self.virtual_token_reserves.saturating_sub(new_token_reserves);
        
        // Convert to human-readable
        tokens_base_units as f64 / 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32)
    }
    
    /// Calculate how much SOL you get for selling tokens
    pub fn calculate_sell_sol(&self, token_amount: f64, fee_basis_points: u64) -> f64 {
        if self.complete {
            return 0.0;
        }
        
        let tokens_base_units = (token_amount * 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32)) as u64;
        
        // Calculate SOL received using constant product
        let k = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);
        let new_token_reserves = self.virtual_token_reserves + tokens_base_units;
        let new_sol_reserves = (k / new_token_reserves as u128) as u64;
        
        let sol_received_lamports = self.virtual_sol_reserves.saturating_sub(new_sol_reserves);
        
        // Apply fee
        let fee_lamports = (sol_received_lamports * fee_basis_points) / 10000;
        let net_sol_lamports = sol_received_lamports - fee_lamports;
        
        net_sol_lamports as f64 / LAMPORTS_PER_SOL as f64
    }
    
    /// Calculate bonding curve progress (0.0 to 1.0)
    pub fn calculate_progress(&self) -> f64 {
        const INITIAL_REAL_TOKEN_RESERVES: u64 = 793_100_000_000_000; // 793.1M tokens with 6 decimals
        
        if self.real_token_reserves >= INITIAL_REAL_TOKEN_RESERVES {
            return 0.0;
        }
        
        1.0 - (self.real_token_reserves as f64 / INITIAL_REAL_TOKEN_RESERVES as f64)
    }
}

/// Derive bonding curve address from token mint
pub fn find_bonding_curve_address(token_mint: &Pubkey) -> Result<Pubkey, Box<dyn std::error::Error + Send + Sync>> {
    let program_id = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    
    let (pda, _bump) = Pubkey::find_program_address(
        &[PUMP_CURVE_SEED, token_mint.as_ref()],
        &program_id,
    );
    
    Ok(pda)
}

/// Fetch bonding curve state from blockchain
pub async fn fetch_bonding_curve_state(
    rpc_client: &RpcClient,
    token_mint: &Pubkey,
) -> Result<BondingCurveState, Box<dyn std::error::Error + Send + Sync>> {
    let curve_address = find_bonding_curve_address(token_mint)?;
    
    info!("Fetching bonding curve: {}", curve_address);
    
    // Try to fetch the account, handle if it doesn't exist yet
    let account = match rpc_client.get_account(&curve_address) {
        Ok(acc) => acc,
        Err(e) => {
            // Account not found - token might be too new
            // Return error with helpful message
            return Err(format!("Bonding curve not found (token too new?): {}", e).into());
        }
    };
    
    BondingCurveState::from_account_data(&account.data)
}

// ============================================================================
// OPTIMIZATION #12: Bonding Curve Cache with Micro-TTL
// ============================================================================

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use log::debug;

/// Cache entry for bonding curve state
#[derive(Clone)]
pub struct CurveCacheEntry {
    pub state: BondingCurveState,
    pub cached_at: Instant,
    pub ttl_ms: u64,
}

impl CurveCacheEntry {
    /// Check if cache entry has expired
    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed().as_millis() > self.ttl_ms as u128
    }
    
    /// Get age of cache entry in milliseconds
    pub fn age_ms(&self) -> u128 {
        self.cached_at.elapsed().as_millis()
    }
}

/// Cache performance statistics
#[derive(Clone, Debug)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub expirations: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// Thread-safe bonding curve cache with automatic expiration
pub struct BondingCurveCache {
    cache: Arc<RwLock<HashMap<Pubkey, CurveCacheEntry>>>,
    ttl_ms: u64,
    stats: Arc<RwLock<CacheStats>>,
}

impl BondingCurveCache {
    /// Create new cache with specified TTL in milliseconds
    pub fn new(ttl_ms: u64) -> Self {
        info!("🎯 Initializing bonding curve cache (TTL: {}ms)", ttl_ms);
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl_ms,
            stats: Arc::new(RwLock::new(CacheStats {
                hits: 0,
                misses: 0,
                expirations: 0,
            })),
        }
    }
    
    /// Get curve state from cache or fetch from RPC
    /// This is the main entry point for all curve fetches
    pub async fn get_or_fetch(
        &self,
        rpc_client: &RpcClient,
        token_mint: &Pubkey,
    ) -> Result<BondingCurveState, Box<dyn std::error::Error + Send + Sync>> {
        // Fast path: Try cache first (read lock)
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(token_mint) {
                if !entry.is_expired() {
                    // Cache HIT - return immediately
                    println!("🎯 Curve cache HIT for {} (age: {}ms, TTL: {}ms)", 
                        token_mint, entry.age_ms(), self.ttl_ms);
                    
                    // Update stats
                    let mut stats = self.stats.write().await;
                    stats.hits += 1;
                    
                    return Ok(entry.state.clone());
                } else {
                    // Entry expired
                    println!("⏰ Curve cache EXPIRED for {} (age: {}ms > TTL: {}ms)", 
                        token_mint, entry.age_ms(), self.ttl_ms);
                }
            }
        }
        
        // Slow path: Cache miss or expired - fetch from RPC
        println!("🔄 Curve cache MISS for {} - fetching from RPC", token_mint);
        
        // Update stats (separate scope to avoid holding write lock during RPC call)
        {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
        }
        
        let start = Instant::now();
        let state = match fetch_bonding_curve_state(rpc_client, token_mint).await {
            Ok(s) => s,
            Err(e) => {
                println!("❌ Curve cache fetch FAILED for {}: {:?}", token_mint, e);
                return Err(e);
            }
        };
        let fetch_time = start.elapsed().as_millis();
        
        println!("✅ RPC fetch completed in {}ms", fetch_time);
        
        // Update cache (write lock)
        {
            let mut cache = self.cache.write().await;
            cache.insert(*token_mint, CurveCacheEntry {
                state: state.clone(),
                cached_at: Instant::now(),
                ttl_ms: self.ttl_ms,
            });
        }
        
        Ok(state)
    }
    
    /// Get cache statistics for monitoring
    pub async fn get_stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        stats.clone()
    }
    
    /// Manually clear entire cache (useful for testing)
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        info!("🧹 Curve cache cleared");
    }
    
    /// Remove expired entries (automatic cleanup)
    pub async fn prune_expired(&self) -> usize {
        let mut cache = self.cache.write().await;
        let before = cache.len();
        cache.retain(|_, entry| !entry.is_expired());
        let after = cache.len();
        let removed = before - after;
        if removed > 0 {
            debug!("🧹 Pruned {} expired cache entries ({} → {})", removed, before, after);
        }
        removed
    }
    
    /// Get current cache size
    pub async fn size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_price_calculation() {
        let state = BondingCurveState {
            virtual_token_reserves: 1_073_000_000_000_000, // 1.073B tokens
            virtual_sol_reserves: 30_000_000_000, // 30 SOL
            real_token_reserves: 793_100_000_000_000,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
            creator: Pubkey::default(),  // Dummy for test
        };
        
        let price = state.calculate_price();
        
        // Initial price should be around 0.000000028 SOL
        assert!(price > 0.000000020 && price < 0.000000035, "Price: {}", price);
    }
    
    #[test]
    fn test_buy_calculation() {
        let state = BondingCurveState {
            virtual_token_reserves: 1_073_000_000_000_000,
            virtual_sol_reserves: 30_000_000_000,
            real_token_reserves: 793_100_000_000_000,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
            creator: Pubkey::default(),  // Dummy for test
        };
        
        // Buy with 1 SOL
        let tokens = state.calculate_buy_tokens(1.0);
        
        // Should get around 35M tokens
        assert!(tokens > 30_000_000.0 && tokens < 40_000_000.0, "Tokens: {}", tokens);
    }
}