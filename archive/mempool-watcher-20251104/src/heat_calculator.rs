use crate::decoder::{DecodedTransaction, WalletType};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use tokio::sync::RwLock;

/// Mempool heat index (0-100 scale)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatIndex {
    pub score: u8,
    pub tx_rate: f64,           // Transactions per second
    pub whale_activity: f64,    // SOL volume from whales
    pub bot_density: f64,       // % of transactions from bots
    pub copy_trade_score: f64,  // Copy-trading pattern strength
    pub timestamp: u64,
}

/// Hot signal - immediate frontrunning opportunity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSignal {
    pub mint: String,
    pub whale_wallet: String,
    pub amount_sol: f64,
    pub action: String,
    pub urgency: u8,  // 0-100, how hot is this signal
    pub timestamp: u64,
}

/// Heat calculator - computes real-time mempool metrics
pub struct HeatCalculator {
    window_secs: u64,
    whale_threshold_sol: f64,
    bot_repeat_threshold: usize,
    
    // Recent transactions (keyed by timestamp)
    recent_txs: Arc<DashMap<u64, DecodedTransaction>>,
    
    // Wallet activity tracking
    wallet_tx_count: Arc<DashMap<String, usize>>,
    
    // Signal deduplication (5-second cooldown)
    signal_cache: Arc<RwLock<HashSet<(String, String)>>>, // (mint, wallet) pairs
    last_cleanup: Arc<RwLock<Instant>>,
    
    // Curve PDA tracking (prevent duplicate bonding curve detection)
    curve_pda_cache: Arc<RwLock<HashSet<String>>>, // Set of curve PDAs
}

impl HeatCalculator {
    pub fn new(window_secs: u64, whale_threshold_sol: f64, bot_repeat_threshold: usize) -> Self {
        Self {
            window_secs,
            whale_threshold_sol,
            bot_repeat_threshold,
            recent_txs: Arc::new(DashMap::new()),
            wallet_tx_count: Arc::new(DashMap::new()),
            signal_cache: Arc::new(RwLock::new(HashSet::new())),
            last_cleanup: Arc::new(RwLock::new(Instant::now())),
            curve_pda_cache: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Add a transaction to the tracking window
    pub fn add_transaction(&self, tx: DecodedTransaction) {
        let timestamp = tx.timestamp;
        
        // Track wallet activity
        self.wallet_tx_count
            .entry(tx.wallet.clone())
            .and_modify(|count| *count += 1)
            .or_insert(1);

        // Store transaction
        self.recent_txs.insert(timestamp, tx);

        // Cleanup old transactions
        self.cleanup_old_transactions();
    }

    /// Calculate current heat index
    pub fn calculate_heat(&self) -> HeatIndex {
        let now = current_timestamp();
        let window_start = now.saturating_sub(self.window_secs);

        // Filter transactions in window
        let recent: Vec<_> = self.recent_txs
            .iter()
            .filter(|entry| *entry.key() >= window_start)
            .map(|entry| entry.value().clone())
            .collect();

        let tx_count = recent.len() as f64;
        
        // Calculate metrics
        let tx_rate = if tx_count > 0.0 {
            tx_count / self.window_secs as f64
        } else {
            0.0
        };

        let whale_volume: f64 = recent
            .iter()
            .filter(|tx| tx.wallet_type == WalletType::Whale)
            .map(|tx| tx.amount_sol)
            .sum();

        let bot_count = recent
            .iter()
            .filter(|tx| tx.wallet_type == WalletType::Bot)
            .count() as f64;

        let bot_density = if tx_count > 0.0 {
            (bot_count / tx_count) * 100.0
        } else {
            0.0
        };

        // Copy-trading detection (simplified)
        let copy_trade_score = self.detect_copy_trading(&recent);

        // Composite heat score (0-100)
        let score = self.calculate_composite_score(
            tx_rate,
            whale_volume,
            bot_density,
            copy_trade_score,
        );

        HeatIndex {
            score,
            tx_rate,
            whale_activity: whale_volume,
            bot_density,
            copy_trade_score,
            timestamp: now,
        }
    }

    /// Detect potential hot signals (whale movements)
    pub fn check_hot_signals(&self) -> Vec<HotSignal> {
        let now = current_timestamp();
        let window_start = now.saturating_sub(5); // Last 5 seconds

        self.recent_txs
            .iter()
            .filter(|entry| *entry.key() >= window_start)
            .filter(|entry| entry.value().wallet_type == WalletType::Whale)
            .map(|entry| {
                let tx = entry.value();
                let urgency = self.calculate_urgency(&tx);
                
                HotSignal {
                    mint: tx.mint.clone(),
                    whale_wallet: tx.wallet.clone(),
                    amount_sol: tx.amount_sol,
                    action: format!("{:?}", tx.action),
                    urgency,
                    timestamp: tx.timestamp,
                }
            })
            .collect()
    }

    /// Calculate composite heat score
    fn calculate_composite_score(
        &self,
        tx_rate: f64,
        whale_volume: f64,
        bot_density: f64,
        copy_trade_score: f64,
    ) -> u8 {
        // Weights for each component
        const TX_RATE_WEIGHT: f64 = 0.25;
        const WHALE_WEIGHT: f64 = 0.35;
        const BOT_WEIGHT: f64 = 0.20;
        const COPY_WEIGHT: f64 = 0.20;

        // Normalize components to 0-100 scale
        let tx_score = (tx_rate * 10.0).min(100.0); // 10 tx/s = max
        let whale_score = (whale_volume * 2.0).min(100.0); // 50 SOL = max
        let bot_score = bot_density.min(100.0);
        let copy_score = copy_trade_score;

        // Weighted average
        let composite = (tx_score * TX_RATE_WEIGHT)
            + (whale_score * WHALE_WEIGHT)
            + (bot_score * BOT_WEIGHT)
            + (copy_score * COPY_WEIGHT);

        composite.clamp(0.0, 100.0) as u8
    }

    /// Detect copy-trading patterns (simplified)
    fn detect_copy_trading(&self, recent: &[DecodedTransaction]) -> f64 {
        // Look for multiple wallets trading same mint within short window
        let mint_wallets: DashMap<String, Vec<String>> = DashMap::new();

        for tx in recent {
            mint_wallets
                .entry(tx.mint.clone())
                .or_insert_with(Vec::new)
                .push(tx.wallet.clone());
        }

        // Calculate max copy intensity
        let max_copies = mint_wallets
            .iter()
            .map(|entry| entry.value().len())
            .max()
            .unwrap_or(0) as f64;

        // Score: multiple wallets on same mint = copy trading
        ((max_copies - 1.0) * 20.0).min(100.0)
    }

    /// Calculate urgency for a hot signal
    fn calculate_urgency(&self, tx: &DecodedTransaction) -> u8 {
        // Urgency formula from spec: (amount_score × 0.6) + (wallet_score × 0.4)
        // Clamped to 50-255 range as documented
        
        // Amount score: scale linearly to 10 SOL cap (0-100 scale)
        let amount_score = ((tx.amount_sol / 10.0) * 100.0).min(100.0);
        
        // Wallet score: based on wallet type classification (0-100 scale)
        let wallet_score = match tx.wallet_type {
            WalletType::Whale => 100.0,    // Max score for whales
            WalletType::Bot => 30.0,       // Lower score for bots  
            WalletType::Retail => 10.0,    // Minimal score for retail
            WalletType::Unknown => 0.0,    // No score for unknown
        };
        
        // Apply weights: 60% amount, 40% wallet type
        let composite_score = (amount_score * 0.6) + (wallet_score * 0.4);
        
        // Scale to 50-255 range as specified
        // 0-100 composite maps to 50-255 output
        let urgency = 50.0 + (composite_score * 2.05); // 2.05 = (255-50)/100
        
        urgency.clamp(50.0, 255.0) as u8
    }

    /// Remove transactions older than window
    fn cleanup_old_transactions(&self) {
        let now = current_timestamp();
        let cutoff = now.saturating_sub(self.window_secs);

        self.recent_txs.retain(|timestamp, _| *timestamp >= cutoff);
    }

    /// Get current transaction count in window
    pub fn get_transaction_count(&self) -> usize {
        self.recent_txs.len()
    }

    /// Get wallet activity count
    pub fn get_wallet_activity(&self, wallet: &str) -> usize {
        self.wallet_tx_count.get(wallet).map(|c| *c).unwrap_or(0)
    }

    /// Check if signal should be deduplicated (5-second cooldown)
    pub async fn should_send_signal(&self, mint: &str, wallet: &str) -> bool {
        let signal_key = (mint.to_string(), wallet.to_string());
        
        // Check deduplication cache
        {
            let cache = self.signal_cache.read().await;
            if cache.contains(&signal_key) {
                return false; // Signal already sent recently
            }
        }
        
        // Add to cache (signal is new)
        {
            let mut cache = self.signal_cache.write().await;
            cache.insert(signal_key);
        }
        
        // Trigger cleanup if needed (every 10 seconds)
        self.cleanup_signal_cache_if_needed().await;
        
        true
    }

    /// Cleanup signal cache periodically (every 10 seconds)
    async fn cleanup_signal_cache_if_needed(&self) {
        let should_cleanup = {
            let last_cleanup = self.last_cleanup.read().await;
            last_cleanup.elapsed().as_secs() >= 10
        };

        if should_cleanup {
            // Note: In a real implementation, we'd need timestamp-based entries
            // For now, clear the entire cache every 10 seconds
            {
                let mut cache = self.signal_cache.write().await;
                cache.clear();
            }
            
            {
                let mut last_cleanup = self.last_cleanup.write().await;
                *last_cleanup = Instant::now();
            }
            
            log::debug!("🧹 Cleaned signal deduplication cache");
        }
    }

    /// Get current signal cache size (for monitoring)
    pub async fn get_signal_cache_size(&self) -> usize {
        let cache = self.signal_cache.read().await;
        cache.len()
    }

    /// Check if this curve PDA has already been detected (prevent duplicate rug detection)
    pub async fn is_curve_pda_seen(&self, curve_pda: &str) -> bool {
        let cache = self.curve_pda_cache.read().await;
        cache.contains(curve_pda)
    }

    /// Mark a curve PDA as seen to prevent duplicate detection
    pub async fn mark_curve_pda_seen(&self, curve_pda: &str) {
        let mut cache = self.curve_pda_cache.write().await;
        cache.insert(curve_pda.to_string());
        
        // Prevent unbounded growth - keep only last 1000 curve PDAs
        if cache.len() > 1000 {
            let items: Vec<_> = cache.iter().cloned().collect();
            cache.clear();
            // Keep the most recent half (this is a simple approach - could be improved)
            for item in items.into_iter().skip(500) {
                cache.insert(item);
            }
        }
    }

    /// Get curve PDA cache size (for monitoring)
    pub async fn get_curve_pda_cache_size(&self) -> usize {
        let cache = self.curve_pda_cache.read().await;
        cache.len()
    }
}

/// Get current unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::{TransactionAction, ProgramType};

    #[test]
    fn test_heat_calculation() {
        let calculator = HeatCalculator::new(10, 10.0, 3);

        // Add some test transactions
        for i in 0..5 {
            calculator.add_transaction(DecodedTransaction {
                signature: format!("sig{}", i),
                mint: "test_mint".to_string(),
                action: TransactionAction::Buy,
                amount_sol: 5.0,
                wallet: format!("wallet{}", i),
                wallet_type: WalletType::Retail,
                timestamp: current_timestamp(),
                program: ProgramType::PumpFun,
            });
        }

        let heat = calculator.calculate_heat();
        assert!(heat.score <= 100);
        assert!(heat.tx_rate > 0.0);
    }

    #[test]
    fn test_hot_signal_detection() {
        let calculator = HeatCalculator::new(10, 10.0, 3);

        // Add a whale transaction
        calculator.add_transaction(DecodedTransaction {
            signature: "whale_sig".to_string(),
            mint: "hot_mint".to_string(),
            action: TransactionAction::Buy,
            amount_sol: 20.0,
            wallet: "whale_wallet".to_string(),
            wallet_type: WalletType::Whale,
            timestamp: current_timestamp(),
            program: ProgramType::PumpFun,
        });

        let signals = calculator.check_hot_signals();
        assert!(!signals.is_empty());
        assert_eq!(signals[0].mint, "hot_mint");
    }
}
