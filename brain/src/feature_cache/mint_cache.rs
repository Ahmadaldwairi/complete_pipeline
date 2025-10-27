//! 🪙 Mint Feature Cache
//!
//! Lock-free cache of token metrics updated from LaunchTracker SQLite database.
//! Provides <50µs read access to mint features for decision-making.

use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;
use rusqlite::{Connection, params};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use log::{info, warn, error, debug};
use anyhow::{Result, Context};

/// Features extracted for each token mint
#[derive(Debug, Clone)]
pub struct MintFeatures {
    /// Seconds since token launch
    pub age_since_launch: u64,
    
    /// Current token price in SOL (from last trade)
    pub current_price: f64,
    
    /// Trading volume in last 60 seconds (SOL)
    pub vol_60s_sol: f64,
    
    /// Number of unique buyers in last 60 seconds
    pub buyers_60s: u32,
    
    /// Ratio of buys to sells in last 60 seconds
    pub buys_sells_ratio: f64,
    
    /// Proxy for bonding curve depth (total_supply - burned_tokens)
    pub curve_depth_proxy: u64,
    
    /// Follow-through score (0-100) computed from momentum
    pub follow_through_score: u8,
    
    /// Last update timestamp (Unix seconds)
    pub last_update: u64,
    
    /// Number of buyers in last 2 seconds (for Path B trigger)
    pub buyers_2s: u32,
    
    /// Trading volume in last 5 seconds (SOL, for Path B trigger)
    pub vol_5s_sol: f64,
}

impl Default for MintFeatures {
    fn default() -> Self {
        Self {
            age_since_launch: 0,
            current_price: 0.0,
            vol_60s_sol: 0.0,
            buyers_60s: 0,
            buys_sells_ratio: 1.0,
            curve_depth_proxy: 0,
            follow_through_score: 0,
            last_update: 0,
            buyers_2s: 0,
            vol_5s_sol: 0.0,
        }
    }
}

impl MintFeatures {
    /// Check if data is stale (>2 seconds old)
    pub fn is_stale(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_update) > 2
    }
    
    /// Format mint address for logging (first 12 chars)
    pub fn mint_short(mint: &Pubkey) -> String {
        let s = mint.to_string();
        s.chars().take(12).collect()
    }
}

/// Lock-free cache of mint features
#[derive(Clone)]
pub struct MintCache {
    cache: Arc<DashMap<Pubkey, MintFeatures>>,
    db_path: String,
}

impl MintCache {
    /// Create new mint cache
    pub fn new(db_path: String) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            db_path,
        }
    }
    
    /// Get features for a mint (returns None if not in cache)
    pub fn get(&self, mint: &Pubkey) -> Option<MintFeatures> {
        self.cache.get(mint).map(|entry| entry.value().clone())
    }
    
    /// Insert or update features for a mint
    pub fn insert(&self, mint: Pubkey, features: MintFeatures) {
        self.cache.insert(mint, features);
    }
    
    /// Check if mint exists in cache
    pub fn contains(&self, mint: &Pubkey) -> bool {
        self.cache.contains_key(mint)
    }
    
    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
    
    /// Start background updater task
    pub fn start_updater(self: Arc<Self>, update_interval_ms: u64) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("🪙 Starting Mint Cache updater (interval: {}ms)", update_interval_ms);
            let mut interval = tokio::time::interval(Duration::from_millis(update_interval_ms));
            
            loop {
                interval.tick().await;
                
                match self.update_cache().await {
                    Ok(count) => {
                        debug!("✅ Updated {} mints in cache", count);
                    }
                    Err(e) => {
                        error!("❌ Failed to update mint cache: {:?}", e);
                    }
                }
            }
        })
    }
    
    /// Update cache from SQLite database
    async fn update_cache(&self) -> Result<usize> {
        // Run DB query in blocking task to avoid blocking async runtime
        let db_path = self.db_path.clone();
        let features = tokio::task::spawn_blocking(move || {
            Self::query_mint_features(&db_path)
        })
        .await
        .context("Failed to join blocking task")??;
        
        // Update cache with new features
        let count = features.len();
        for (mint, feature) in features {
            self.cache.insert(mint, feature);
        }
        
        // Remove stale entries (>5 minutes old)
        self.cache.retain(|_, v| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now.saturating_sub(v.last_update) < 300
        });
        
        Ok(count)
    }
    
    /// Query mint features from SQLite database
    fn query_mint_features(db_path: &str) -> Result<Vec<(Pubkey, MintFeatures)>> {
        let conn = Connection::open(db_path)
            .context("Failed to open SQLite database")?;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // Query active tokens from windows table (60s aggregates) joined with tokens for launch time
        // Gets most active tokens by volume from last 2 minutes
        let mut stmt = conn.prepare(
            "SELECT 
                w60.mint,
                t.launch_block_time,
                w60.close as current_price_sol,
                w60.vol_sol as vol_60s_sol,
                w60.uniq_buyers as buyers_60s,
                w60.num_buys as buys_60s,
                w60.num_sells as sells_60s,
                0 as total_supply,
                COALESCE(w2.uniq_buyers, 0) as buyers_2s,
                COALESCE(w5.vol_sol, 0.0) as vol_5s_sol
             FROM windows w60
             INNER JOIN tokens t ON w60.mint = t.mint
             LEFT JOIN windows w2 ON w60.mint = w2.mint AND w2.window_sec = 2
             LEFT JOIN windows w5 ON w60.mint = w5.mint AND w5.window_sec = 5
             WHERE w60.window_sec = 60
               AND w60.end_time > ?1
               AND t.launch_block_time > ?2
             ORDER BY w60.vol_sol DESC
             LIMIT 500"
        )?;
        
        let recent_cutoff = now - 259200; // Active in last 3 days (relaxed for testing with historical data)
        let launch_cutoff = now - 259200; // Launched in last 3 days (relaxed for testing)
        
        let rows = stmt.query_map(params![recent_cutoff, launch_cutoff], |row| {
            let mint_str: String = row.get(0)?;
            let launch_ts: u64 = row.get(1)?;
            let price: f64 = row.get(2)?;
            let vol_60s: f64 = row.get(3)?;
            let buyers_60s: u32 = row.get(4)?;
            let buys_60s: u32 = row.get(5)?;
            let sells_60s: u32 = row.get(6)?;
            let total_supply: u64 = row.get(7)?;
            let buyers_2s: u32 = row.get(8)?;
            let vol_5s: f64 = row.get(9)?;
            
            Ok((
                mint_str,
                launch_ts,
                price,
                vol_60s,
                buyers_60s,
                buys_60s,
                sells_60s,
                total_supply,
                buyers_2s,
                vol_5s,
            ))
        })?;
        
        let mut features = Vec::new();
        
        for row_result in rows {
            let (mint_str, launch_ts, price, vol_60s, buyers_60s, buys_60s, sells_60s, 
                 total_supply, buyers_2s, vol_5s) = row_result?;
            
            // Parse mint address
            let mint = match Pubkey::from_str(&mint_str) {
                Ok(m) => m,
                Err(e) => {
                    warn!("Invalid mint address {}: {}", mint_str, e);
                    continue;
                }
            };
            
            // Compute derived features
            let age_since_launch = now.saturating_sub(launch_ts);
            
            let buys_sells_ratio = if sells_60s > 0 {
                buys_60s as f64 / sells_60s as f64
            } else {
                if buys_60s > 0 { 10.0 } else { 1.0 }
            };
            
            // Simple follow-through score based on momentum indicators
            // Score = 0.4 * buyers_score + 0.4 * volume_score + 0.2 * ratio_score
            let buyers_score = ((buyers_60s.min(50) as f64 / 50.0) * 100.0) as u8;
            let volume_score = ((vol_60s.min(100.0) / 100.0) * 100.0) as u8;
            let ratio_score = ((buys_sells_ratio.min(3.0) / 3.0) * 100.0) as u8;
            let follow_through_score = (
                (buyers_score as f64 * 0.4) +
                (volume_score as f64 * 0.4) +
                (ratio_score as f64 * 0.2)
            ) as u8;
            
            let feature = MintFeatures {
                age_since_launch,
                current_price: price,
                vol_60s_sol: vol_60s,
                buyers_60s,
                buys_sells_ratio,
                curve_depth_proxy: total_supply,
                follow_through_score,
                last_update: now,
                buyers_2s,
                vol_5s_sol: vol_5s,
            };
            
            features.push((mint, feature));
        }
        
        Ok(features)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mint_features_default() {
        let features = MintFeatures::default();
        assert_eq!(features.age_since_launch, 0);
        assert_eq!(features.buyers_60s, 0);
        assert_eq!(features.follow_through_score, 0);
    }
    
    #[test]
    fn test_mint_features_staleness() {
        let mut features = MintFeatures::default();
        features.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() - 10; // 10 seconds ago
        
        assert!(features.is_stale());
        
        features.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        assert!(!features.is_stale());
    }
    
    #[test]
    fn test_mint_short() {
        let mint = Pubkey::new_unique();
        let short = MintFeatures::mint_short(&mint);
        assert_eq!(short.len(), 12);
    }
}
