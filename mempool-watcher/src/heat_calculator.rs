use crate::decoder::{DecodedTransaction, WalletType};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
}

impl HeatCalculator {
    pub fn new(window_secs: u64, whale_threshold_sol: f64, bot_repeat_threshold: usize) -> Self {
        Self {
            window_secs,
            whale_threshold_sol,
            bot_repeat_threshold,
            recent_txs: Arc::new(DashMap::new()),
            wallet_tx_count: Arc::new(DashMap::new()),
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
        let mut mint_wallets: DashMap<String, Vec<String>> = DashMap::new();

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
        // Urgency based on amount and recency
        let amount_score = (tx.amount_sol / self.whale_threshold_sol * 50.0).min(50.0);
        let recency_score = 50.0; // Recent = urgent
        
        (amount_score + recency_score) as u8
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
