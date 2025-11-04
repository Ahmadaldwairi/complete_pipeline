//! 👛 Wallet Feature Cache
//!
//! Lock-free cache of trader statistics updated from SQLite database.
//! Provides <50µs read access to wallet features for copy-trade decisions.

use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;
use rusqlite::{Connection, params};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use log::{info, warn, error, debug};
use anyhow::{Result, Context};

/// Wallet tier classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WalletTier {
    Discovery = 0,  // Unknown/new wallet
    C = 1,          // Win≥50%, PnL≥15 SOL, conf 75-84
    B = 2,          // Win≥55%, PnL≥40 SOL, conf 85-89
    A = 3,          // Win≥60%, PnL≥100 SOL, conf 92-95
}

impl WalletTier {
    /// Get confidence score for this tier
    pub fn confidence(&self) -> u8 {
        match self {
            WalletTier::Discovery => 50,
            WalletTier::C => 80,
            WalletTier::B => 87,
            WalletTier::A => 93,
        }
    }
    
    /// Check if tier meets minimum for copy trading
    pub fn meets_copy_threshold(&self) -> bool {
        *self >= WalletTier::C
    }
}

/// Last trade info for a wallet
#[derive(Debug, Clone)]
pub struct LastTrade {
    pub mint: Pubkey,
    pub side: u8,           // 0=BUY, 1=SELL
    pub size_sol: f64,
    pub timestamp: u64,     // Unix seconds
}

/// Features extracted for each wallet
#[derive(Debug, Clone)]
pub struct WalletFeatures {
    /// Win rate over last 7 days (0.0-1.0)
    pub win_rate_7d: f64,
    
    /// Realized profit/loss over last 7 days (SOL)
    pub realized_pnl_7d: f64,
    
    /// Total number of completed trades (7 days)
    pub trade_count: u32,
    
    /// Average position size (SOL)
    pub avg_size: f64,
    
    /// Tier classification (A/B/C/Discovery)
    pub tier: WalletTier,
    
    /// Confidence score (0-100) for copy trading
    pub confidence: u8,
    
    /// Last trade information
    pub last_trade: Option<LastTrade>,
    
    /// Last update timestamp (Unix seconds)
    pub last_update: u64,
    
    /// Bootstrap formula score for discovery wallets
    /// score = min(90, 50 + wins×2 + (pnl_7d/5))
    pub bootstrap_score: u8,
}

impl Default for WalletFeatures {
    fn default() -> Self {
        Self {
            win_rate_7d: 0.0,
            realized_pnl_7d: 0.0,
            trade_count: 0,
            avg_size: 0.0,
            tier: WalletTier::Discovery,
            confidence: 50,
            last_trade: None,
            last_update: 0,
            bootstrap_score: 50,
        }
    }
}

impl WalletFeatures {
    /// Check if data is stale (>5 seconds old)
    pub fn is_stale(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.last_update) > 5
    }
    
    /// Format wallet address for logging (first 12 chars)
    pub fn wallet_short(wallet: &Pubkey) -> String {
        let s = wallet.to_string();
        s.chars().take(12).collect()
    }
    
    /// Classify wallet into tier based on performance
    pub fn classify_tier(win_rate: f64, pnl_7d: f64, trade_count: u32) -> WalletTier {
        // Require minimum trade count for tier classification
        if trade_count < 10 {
            return WalletTier::Discovery;
        }
        
        // Tier A: Win≥60%, PnL≥100 SOL
        if win_rate >= 0.60 && pnl_7d >= 100.0 {
            return WalletTier::A;
        }
        
        // Tier B: Win≥55%, PnL≥40 SOL
        if win_rate >= 0.55 && pnl_7d >= 40.0 {
            return WalletTier::B;
        }
        
        // Tier C: Win≥50%, PnL≥15 SOL
        if win_rate >= 0.50 && pnl_7d >= 15.0 {
            return WalletTier::C;
        }
        
        WalletTier::Discovery
    }
    
    /// Calculate confidence score (0-100)
    pub fn calculate_confidence(tier: WalletTier, win_rate: f64, trade_count: u32) -> u8 {
        let base = tier.confidence();
        
        // Boost confidence for very high win rates
        let win_boost = if win_rate > 0.70 {
            ((win_rate - 0.70) * 20.0) as u8  // +0-6 points
        } else {
            0
        };
        
        // Boost confidence for high trade count (experience)
        let experience_boost = if trade_count > 50 {
            (trade_count.min(200) / 50) as u8  // +1-4 points
        } else {
            0
        };
        
        (base + win_boost + experience_boost).min(100)
    }
    
    /// Calculate bootstrap score for discovery wallets
    /// score = min(90, 50 + wins×2 + (pnl_7d/5))
    pub fn calculate_bootstrap_score(wins: u32, pnl_7d: f64) -> u8 {
        let base = 50;
        let win_contribution = (wins * 2) as i32;
        let pnl_contribution = (pnl_7d / 5.0) as i32;
        
        let score = base + win_contribution + pnl_contribution;
        score.clamp(0, 90) as u8
    }
}

/// Lock-free cache of wallet features
#[derive(Clone)]
pub struct WalletCache {
    cache: Arc<DashMap<Pubkey, WalletFeatures>>,
    db_path: String,
}

impl WalletCache {
    /// Create new wallet cache
    pub fn new(db_path: String) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            db_path,
        }
    }
    
    /// Get features for a wallet (returns None if not in cache)
    pub fn get(&self, wallet: &Pubkey) -> Option<WalletFeatures> {
        self.cache.get(wallet).map(|entry| entry.value().clone())
    }
    
    /// Insert or update features for a wallet
    pub fn insert(&self, wallet: Pubkey, features: WalletFeatures) {
        self.cache.insert(wallet, features);
    }
    
    /// Check if wallet exists in cache
    pub fn contains(&self, wallet: &Pubkey) -> bool {
        self.cache.contains_key(wallet)
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
            info!("👛 Starting Wallet Cache updater (interval: {}ms)", update_interval_ms);
            let mut interval = tokio::time::interval(Duration::from_millis(update_interval_ms));
            
            loop {
                interval.tick().await;
                
                match self.update_cache().await {
                    Ok(count) => {
                        debug!("✅ Updated {} wallets in cache", count);
                    }
                    Err(e) => {
                        error!("❌ Failed to update wallet cache: {:?}", e);
                    }
                }
            }
        })
    }
    
    /// Update cache from SQLite database
    pub async fn update_cache(&self) -> Result<usize> {
        // Run DB query in blocking task to avoid blocking async runtime
        let db_path = self.db_path.clone();
        let features = tokio::task::spawn_blocking(move || {
            Self::query_wallet_features(&db_path)
        })
        .await
        .context("Failed to join blocking task")??;
        
        // Update cache with new features
        let count = features.len();
        for (wallet, feature) in features {
            self.cache.insert(wallet, feature);
        }
        
        // Remove stale entries (>10 minutes old)
        self.cache.retain(|_, v| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now.saturating_sub(v.last_update) < 600
        });
        
        Ok(count)
    }
    
    /// Query wallet features from SQLite database
    fn query_wallet_features(db_path: &str) -> Result<Vec<(Pubkey, WalletFeatures)>> {
        let conn = Connection::open(db_path)
            .context("Failed to open SQLite database")?;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        let seven_days_ago = now - (7 * 24 * 60 * 60);
        
        // Query wallet statistics from actual schema
        let mut stmt = conn.prepare(
            "SELECT 
                wallet,
                realized_wins,
                realized_losses,
                net_pnl_sol,
                total_trades,
                is_tracked,
                win_rate,
                last_seen
             FROM wallet_stats
             WHERE last_seen > ?1
               AND total_trades > 0
               AND is_tracked = 1
             ORDER BY net_pnl_sol DESC
             LIMIT 1000"
        )?;
        
        let rows = stmt.query_map(params![seven_days_ago], |row| {
            let wallet_str: String = row.get(0)?;
            let wins: u32 = row.get(1)?;
            let losses: u32 = row.get(2)?;
            let pnl: f64 = row.get(3)?;
            let trade_count: u32 = row.get(4)?;
            let is_tracked: i32 = row.get(5)?;
            let win_rate: f64 = row.get(6)?;
            let last_seen: i64 = row.get(7)?;
            
            Ok((
                wallet_str,
                wins,
                losses,
                pnl,
                trade_count,
                win_rate,
                last_seen,
            ))
        })?;
        
        let mut features = Vec::new();
        
        for row_result in rows {
            let (wallet_str, wins, losses, pnl, trade_count, win_rate, last_seen) = row_result?;
            
            // Parse wallet address
            let wallet = match Pubkey::from_str(&wallet_str) {
                Ok(w) => w,
                Err(e) => {
                    warn!("Invalid wallet address {}: {}", wallet_str, e);
                    continue;
                }
            };
            
            // Use win_rate from database (already calculated)
            // Classify tier
            let tier = WalletFeatures::classify_tier(win_rate, pnl, trade_count);
            
            // Calculate confidence
            let confidence = WalletFeatures::calculate_confidence(tier, win_rate, trade_count);
            
            // Calculate bootstrap score
            let bootstrap_score = WalletFeatures::calculate_bootstrap_score(wins, pnl);
            
            // Calculate average size from total PnL and trades
            let avg_size = if trade_count > 0 {
                pnl.abs() / trade_count as f64
            } else {
                0.0
            };
            
            let feature = WalletFeatures {
                win_rate_7d: win_rate,
                realized_pnl_7d: pnl,
                trade_count,
                avg_size,
                tier,
                confidence,
                last_trade: None, // No last trade info in current schema
                last_update: now,
                bootstrap_score,
            };
            
            features.push((wallet, feature));
        }
        
        info!("📊 Loaded {} wallet features from database", features.len());
        
        Ok(features)
    }
}
mod tests {
    
    
    #[test]
    fn test_wallet_tier_classification() {
        // Tier A
        assert_eq!(
            WalletFeatures::classify_tier(0.65, 150.0, 50),
            WalletTier::A
        );
        
        // Tier B
        assert_eq!(
            WalletFeatures::classify_tier(0.58, 50.0, 30),
            WalletTier::B
        );
        
        // Tier C
        assert_eq!(
            WalletFeatures::classify_tier(0.52, 20.0, 25),
            WalletTier::C
        );
        
        // Discovery (low trade count)
        assert_eq!(
            WalletFeatures::classify_tier(0.80, 200.0, 5),
            WalletTier::Discovery
        );
        
        // Discovery (below thresholds)
        assert_eq!(
            WalletFeatures::classify_tier(0.45, 10.0, 20),
            WalletTier::Discovery
        );
    }
    
    #[test]
    fn test_confidence_calculation() {
        // Tier A with high win rate
        let conf = WalletFeatures::calculate_confidence(WalletTier::A, 0.75, 100);
        assert!(conf >= 93 && conf <= 100);
        
        // Tier C baseline
        let conf = WalletFeatures::calculate_confidence(WalletTier::C, 0.52, 15);
        assert_eq!(conf, 80);
    }
    
    #[test]
    fn test_bootstrap_score() {
        // score = min(90, 50 + wins×2 + (pnl_7d/5))
        assert_eq!(WalletFeatures::calculate_bootstrap_score(0, 0.0), 50);
        assert_eq!(WalletFeatures::calculate_bootstrap_score(10, 25.0), 75); // 50 + 20 + 5
        assert_eq!(WalletFeatures::calculate_bootstrap_score(20, 100.0), 90); // capped at 90
    }
    
    #[test]
    fn test_wallet_features_default() {
        let features = WalletFeatures::default();
        assert_eq!(features.tier, WalletTier::Discovery);
        assert_eq!(features.confidence, 50);
        assert_eq!(features.trade_count, 0);
    }
    
    #[test]
    fn test_wallet_short() {
        let wallet = Pubkey::new_unique();
        let short = WalletFeatures::wallet_short(&wallet);
        assert_eq!(short.len(), 12);
    }
    
    #[test]
    fn test_tier_confidence() {
        assert_eq!(WalletTier::A.confidence(), 93);
        assert_eq!(WalletTier::B.confidence(), 87);
        assert_eq!(WalletTier::C.confidence(), 80);
        assert_eq!(WalletTier::Discovery.confidence(), 50);
    }
    
    #[test]
    fn test_meets_copy_threshold() {
        assert!(WalletTier::A.meets_copy_threshold());
        assert!(WalletTier::B.meets_copy_threshold());
        assert!(WalletTier::C.meets_copy_threshold());
        assert!(!WalletTier::Discovery.meets_copy_threshold());
    }
}
