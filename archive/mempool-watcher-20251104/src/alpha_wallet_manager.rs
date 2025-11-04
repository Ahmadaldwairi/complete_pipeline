//! 🎯 Alpha Wallet Manager - Loads and manages high-performing wallets
//! 
//! Connects to SQLite database to identify alpha wallets based on:
//! - Win rate > 70%
//! - Total PnL > 10 SOL  
//! - Trade count > 10
//! 
//! Updates every 60 seconds with O(1) lookup performance

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::interval;

/// Alpha wallet manager - tracks high-performing wallets
pub struct AlphaWalletManager {
    db_path: String,
    alpha_wallets: Arc<RwLock<HashSet<String>>>,
    last_update: Arc<RwLock<Instant>>,
    fallback_wallets: Arc<RwLock<HashSet<String>>>, // LRU fallback for DB offline
}

#[derive(Debug)]
pub struct WalletStats {
    pub wallet: String,
    pub total_trades: i64,
    pub winning_trades: i64,
    pub total_pnl_sol: f64,
    pub win_rate: f64,
}

impl AlphaWalletManager {
    pub fn new(db_path: String) -> Self {
        Self {
            db_path,
            alpha_wallets: Arc::new(RwLock::new(HashSet::new())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            fallback_wallets: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Start the background update task
    pub async fn start_background_updates(&self) {
        let alpha_wallets = self.alpha_wallets.clone();
        let last_update = self.last_update.clone();
        let fallback_wallets = self.fallback_wallets.clone();
        let db_path = self.db_path.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60)); // Update every 60s
            
            // Initial load with 1s delay to ensure system is ready
            tokio::time::sleep(Duration::from_secs(1)).await;
            
            loop {
                interval.tick().await;
                
                match Self::load_alpha_wallets_from_db(&db_path).await {
                    Ok(wallets) => {
                        let count = wallets.len();
                        
                        // Update main cache
                        {
                            let mut alpha_cache = alpha_wallets.write().await;
                            *alpha_cache = wallets.clone();
                        }
                        
                        // Update fallback cache (for DB offline scenarios)
                        {
                            let mut fallback_cache = fallback_wallets.write().await;
                            *fallback_cache = wallets;
                        }
                        
                        // Update timestamp
                        {
                            let mut last_update_cache = last_update.write().await;
                            *last_update_cache = Instant::now();
                        }
                        
                        info!("✅ Updated alpha wallets: {} wallets loaded", count);
                    }
                    Err(e) => {
                        error!("❌ Failed to load alpha wallets: {} - Using fallback cache", e);
                        
                        // Use fallback cache if available
                        let fallback_count = {
                            let fallback_cache = fallback_wallets.read().await;
                            fallback_cache.len()
                        };
                        
                        if fallback_count > 0 {
                            // Copy fallback to main cache
                            let fallback_data = {
                                let fallback_cache = fallback_wallets.read().await;
                                fallback_cache.clone()
                            };
                            
                            let mut alpha_cache = alpha_wallets.write().await;
                            *alpha_cache = fallback_data;
                            
                            warn!("⚠️  Using fallback alpha wallets: {} wallets", fallback_count);
                        } else {
                            warn!("⚠️  No fallback alpha wallets available - empty cache");
                        }
                    }
                }
            }
        });
    }

    /// Check if a wallet is in the alpha list (O(1) lookup)
    pub async fn is_alpha_wallet(&self, wallet: &str) -> bool {
        let alpha_cache = self.alpha_wallets.read().await;
        alpha_cache.contains(wallet)
    }

    /// Get current alpha wallet count
    pub async fn get_alpha_wallet_count(&self) -> usize {
        let alpha_cache = self.alpha_wallets.read().await;
        alpha_cache.len()
    }

    /// Get time since last successful update
    pub async fn get_last_update_age(&self) -> Duration {
        let last_update_cache = self.last_update.read().await;
        last_update_cache.elapsed()
    }

    /// Load alpha wallets from SQLite database
    async fn load_alpha_wallets_from_db(db_path: &str) -> Result<HashSet<String>> {
        // Use tokio::task::spawn_blocking for DB operations
        let db_path = db_path.to_string();
        
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open_with_flags(
                &db_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .context("Failed to open SQLite database")?;

            // Query wallet_stats for alpha wallets
            // Criteria: win_rate > 70%, net_pnl_sol > 10.0, total_trades > 10
            let mut stmt = conn.prepare(
                "SELECT wallet, total_trades, realized_wins, net_pnl_sol, win_rate
                 FROM wallet_stats 
                 WHERE total_trades > 10 
                   AND net_pnl_sol > 10.0
                   AND win_rate > 0.70
                 ORDER BY net_pnl_sol DESC
                 LIMIT 1000"
            )?;

            let wallet_iter = stmt.query_map([], |row| {
                let total_trades: i64 = row.get(1)?;
                let realized_wins: i64 = row.get(2)?;
                let net_pnl_sol: f64 = row.get(3)?;
                let win_rate: f64 = row.get(4)?;

                Ok(WalletStats {
                    wallet: row.get(0)?,
                    total_trades,
                    winning_trades: realized_wins,
                    total_pnl_sol: net_pnl_sol,
                    win_rate: win_rate * 100.0, // Convert to percentage
                })
            })?;

            let mut alpha_wallets = HashSet::new();
            let mut loaded_count = 0;

            for wallet_result in wallet_iter {
                let wallet_stats = wallet_result?;
                alpha_wallets.insert(wallet_stats.wallet.clone());
                loaded_count += 1;

                if loaded_count <= 5 {
                    debug!("Alpha wallet: {} ({}% win rate, {:.2} SOL PnL, {} trades)",
                           &wallet_stats.wallet[..8],
                           wallet_stats.win_rate,
                           wallet_stats.total_pnl_sol,
                           wallet_stats.total_trades);
                }
            }

            debug!("Loaded {} total alpha wallets from database", loaded_count);

            Ok(alpha_wallets)
        })
        .await?
    }

    /// Force reload alpha wallets (for testing/debugging)
    pub async fn force_reload(&self) -> Result<usize> {
        let wallets = Self::load_alpha_wallets_from_db(&self.db_path).await?;
        let count = wallets.len();
        
        // Update main cache
        {
            let mut alpha_cache = self.alpha_wallets.write().await;
            *alpha_cache = wallets.clone();
        }
        
        // Update fallback cache
        {
            let mut fallback_cache = self.fallback_wallets.write().await;
            *fallback_cache = wallets;
        }
        
        // Update timestamp
        {
            let mut last_update_cache = self.last_update.write().await;
            *last_update_cache = Instant::now();
        }
        
        info!("🔄 Force reloaded {} alpha wallets", count);
        
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_alpha_wallet_manager_creation() {
        let manager = AlphaWalletManager::new("test.db".to_string());
        assert_eq!(manager.get_alpha_wallet_count().await, 0);
    }

    #[tokio::test] 
    async fn test_alpha_wallet_lookup() {
        let manager = AlphaWalletManager::new("test.db".to_string());
        
        // Should return false for non-existent wallet
        assert!(!manager.is_alpha_wallet("nonexistent").await);
    }
}