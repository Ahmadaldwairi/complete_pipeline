use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::db::Database;
use crate::types::{Trade, TradeSide, Window};

pub struct WindowAggregator {
    intervals: Vec<u64>,
}

impl WindowAggregator {
    pub fn new(intervals: Vec<u64>) -> Self {
        Self { intervals }
    }

    pub fn update_windows(
        &self,
        db: &mut Database,
        mint: &str,
        current_block_time: i64,
        current_slot: u64,
    ) -> Result<()> {
        let mut windows_updated = 0;
        
        for &window_sec in &self.intervals {
            let window_start = (current_block_time / window_sec as i64) * window_sec as i64;
            let window_end = window_start + window_sec as i64;

            // Get all trades in this window
            let trades = db.get_trades_for_window(mint, window_start, window_end)?;

            if trades.is_empty() {
                continue;
            }

            let window = self.compute_window_stats(
                mint,
                window_sec,
                window_start,
                window_end,
                current_slot,
                &trades,
            );

            db.upsert_window(&window)?;
            windows_updated += 1;
            
            debug!(
                "Updated {}-sec window for {} (buys: {}, sells: {}, vol: {} SOL, volatility: {:.10})",
                window_sec, mint, window.num_buys, window.num_sells, window.vol_sol, window.price_volatility
            );
        }
        
        if windows_updated > 0 {
            debug!("📊 Aggregated {} windows for {}", windows_updated, mint);
        }

        Ok(())
    }

    fn compute_window_stats(
        &self,
        mint: &str,
        window_sec: u64,
        start_time: i64,
        end_time: i64,
        slot: u64,
        trades: &[Trade],
    ) -> Window {
        let mut num_buys = 0u64;
        let mut num_sells = 0u64;
        let mut unique_buyers = HashSet::new();
        let mut vol_sol = 0.0;
        let mut vol_tokens = 0.0;
        let mut high = 0.0;
        let mut low = f64::MAX;
        let mut close = 0.0;
        let mut open = 0.0;
        let mut total_sol_weighted = 0.0;
        let mut prices: Vec<f64> = Vec::new();
        
        // For concentration metrics
        let mut buyer_volumes: HashMap<String, f64> = HashMap::new();

        for (i, trade) in trades.iter().enumerate() {
            match trade.side {
                TradeSide::Buy => {
                    num_buys += 1;
                    unique_buyers.insert(trade.trader.clone());
                    *buyer_volumes.entry(trade.trader.clone()).or_insert(0.0) += trade.amount_sol;
                }
                TradeSide::Sell => num_sells += 1,
            }

            vol_sol += trade.amount_sol;
            vol_tokens += trade.amount_tokens;

            if trade.price > high {
                high = trade.price;
            }
            if trade.price < low {
                low = trade.price;
            }
            
            // First trade = open, last trade = close
            if i == 0 {
                open = trade.price;
            }
            close = trade.price;
            
            // Collect prices for volatility calculation
            prices.push(trade.price);

            total_sol_weighted += trade.amount_sol * trade.price;
        }

        let vwap = if vol_sol > 0.0 {
            total_sol_weighted / vol_sol
        } else {
            0.0
        };

        if low == f64::MAX {
            low = 0.0;
        }
        
        // Calculate price volatility (standard deviation)
        let price_volatility = if prices.len() > 1 {
            let mean = prices.iter().sum::<f64>() / prices.len() as f64;
            let variance = prices.iter()
                .map(|p| (p - mean).powi(2))
                .sum::<f64>() / prices.len() as f64;
            variance.sqrt()
        } else {
            0.0
        };

        // Compute top buyer concentration with normalization for <3 buyers
        let (top1_share, top3_share, top5_share) = if !buyer_volumes.is_empty() {
            let mut volumes: Vec<f64> = buyer_volumes.values().copied().collect();
            volumes.sort_by(|a, b| b.partial_cmp(a).unwrap());
            
            let total_buy_vol: f64 = volumes.iter().sum();
            let unique_buyer_count = volumes.len();
            
            // Normalize based on number of unique buyers
            let top1 = volumes.get(0).copied().unwrap_or(0.0) / total_buy_vol.max(1e-9);
            let top3 = if unique_buyer_count >= 3 {
                volumes.iter().take(3).sum::<f64>() / total_buy_vol.max(1e-9)
            } else {
                // For <3 buyers, normalize to actual count
                volumes.iter().sum::<f64>() / total_buy_vol.max(1e-9)
            };
            let top5 = if unique_buyer_count >= 5 {
                volumes.iter().take(5).sum::<f64>() / total_buy_vol.max(1e-9)
            } else {
                // For <5 buyers, normalize to actual count
                volumes.iter().sum::<f64>() / total_buy_vol.max(1e-9)
            };
            
            (top1, top3, top5)
        } else {
            (0.0, 0.0, 0.0)
        };
        
        debug!(
            "Window stats: buyers={}, volatility={:.10}, open={:.10}, close={:.10}",
            unique_buyers.len(), price_volatility, open, close
        );

        Window {
            mint: mint.to_string(),
            window_sec,
            start_slot: slot,
            start_time,
            end_time,
            num_buys,
            num_sells,
            uniq_buyers: unique_buyers.len() as u64,
            vol_tokens,
            vol_sol,
            high,
            low,
            close,
            vwap,
            top1_share,
            top3_share,
            top5_share,
            price_volatility,
            open,
        }
    }
}
