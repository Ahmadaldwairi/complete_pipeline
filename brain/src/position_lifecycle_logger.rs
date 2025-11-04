//! 📝 Position Lifecycle Logger - Track Complete Trade Lifecycle
//!
//! Logs the full journey of each position:
//! 1. BUY decision → TX sent → confirmation
//! 2. gRPC price updates → mint_cache updates
//! 3. Exit condition triggered → SELL decision → TX sent
//! 4. Position closed → final P&L
//!
//! Provides observability for production trading with structured logs.

use log::{info, warn};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;

/// Position lifecycle events
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    /// BUY decision made
    BuyDecision {
        mint: String,
        size_sol: f64,
        size_usd: f64,
        confidence: u8,
        entry_price_sol: f64,
        trigger_source: String,
    },
    
    /// BUY transaction sent to executor
    BuyTxSent {
        mint: String,
        signature: Option<String>,
    },
    
    /// BUY transaction confirmed on-chain
    BuyConfirmed {
        mint: String,
        signature: String,
        actual_tokens: u64,
        actual_sol: u64,
        fees_lamports: u64,
        confirmation_time_ms: u64,
    },
    
    /// Price update received from gRPC
    PriceUpdate {
        mint: String,
        old_price_sol: f64,
        new_price_sol: f64,
        mc_sol: f64,
        update_source: String, // "gRPC" or "UDP"
        hold_duration_secs: u64,
    },
    
    /// Exit condition triggered
    ExitConditionTriggered {
        mint: String,
        reason: String,
        exit_percent: u8,
        current_price_sol: f64,
        entry_price_sol: f64,
        pnl_percent: f64,
        hold_duration_secs: u64,
    },
    
    /// SELL decision made
    SellDecision {
        mint: String,
        size_sol: f64,
        exit_percent: u8,
        reason: String,
    },
    
    /// SELL transaction sent to executor
    SellTxSent {
        mint: String,
        signature: Option<String>,
    },
    
    /// SELL transaction confirmed on-chain
    SellConfirmed {
        mint: String,
        signature: String,
        actual_sol: u64,
        fees_lamports: u64,
        confirmation_time_ms: u64,
    },
    
    /// Position fully closed
    PositionClosed {
        mint: String,
        total_hold_duration_secs: u64,
        entry_sol: f64,
        exit_sol: f64,
        total_fees_sol: f64,
        net_pnl_sol: f64,
        net_pnl_usd: f64,
        roi_percent: f64,
    },
}

/// Lifecycle state for a single position
#[derive(Debug, Clone)]
struct PositionLifecycle {
    mint: String,
    entry_time: Instant,
    entry_timestamp: u64,
    
    // Entry phase
    buy_decision_time: Option<Instant>,
    buy_tx_sent_time: Option<Instant>,
    buy_confirmed_time: Option<Instant>,
    buy_signature: Option<String>,
    
    // Position details
    size_sol: f64,
    size_usd: f64,
    entry_price_sol: f64,
    confidence: u8,
    trigger_source: String,
    
    // Monitoring phase
    price_update_count: u32,
    last_price_sol: f64,
    last_mc_sol: f64,
    
    // Exit phase
    exit_condition_time: Option<Instant>,
    exit_reason: Option<String>,
    sell_decision_time: Option<Instant>,
    sell_tx_sent_time: Option<Instant>,
    sell_confirmed_time: Option<Instant>,
    sell_signature: Option<String>,
    
    // Final metrics
    entry_sol_actual: Option<u64>,
    exit_sol_actual: Option<u64>,
    total_fees_lamports: u64,
}

/// Position lifecycle logger
pub struct PositionLifecycleLogger {
    /// Active position lifecycles
    lifecycles: HashMap<String, PositionLifecycle>,
    
    /// Completed position count
    completed_count: u64,
}

impl PositionLifecycleLogger {
    /// Create new lifecycle logger
    pub fn new() -> Self {
        Self {
            lifecycles: HashMap::new(),
            completed_count: 0,
        }
    }
    
    /// Log a lifecycle event
    pub fn log_event(&mut self, event: LifecycleEvent) {
        match event {
            LifecycleEvent::BuyDecision {
                mint,
                size_sol,
                size_usd,
                confidence,
                entry_price_sol,
                trigger_source,
            } => {
                let now = Instant::now();
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                
                info!(
                    "🔵 [LIFECYCLE] BUY_DECISION | mint: {}...{} | size: {:.3} SOL (${:.2}) | conf: {} | price: {:.9} SOL | source: {}",
                    &mint[..8], &mint[mint.len()-8..], size_sol, size_usd, confidence, entry_price_sol, trigger_source
                );
                
                self.lifecycles.insert(
                    mint.clone(),
                    PositionLifecycle {
                        mint: mint.clone(),
                        entry_time: now,
                        entry_timestamp: timestamp,
                        buy_decision_time: Some(now),
                        buy_tx_sent_time: None,
                        buy_confirmed_time: None,
                        buy_signature: None,
                        size_sol,
                        size_usd,
                        entry_price_sol,
                        confidence,
                        trigger_source,
                        price_update_count: 0,
                        last_price_sol: entry_price_sol,
                        last_mc_sol: 0.0,
                        exit_condition_time: None,
                        exit_reason: None,
                        sell_decision_time: None,
                        sell_tx_sent_time: None,
                        sell_confirmed_time: None,
                        sell_signature: None,
                        entry_sol_actual: None,
                        exit_sol_actual: None,
                        total_fees_lamports: 0,
                    },
                );
            }
            
            LifecycleEvent::BuyTxSent { mint, signature } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.buy_tx_sent_time = Some(Instant::now());
                    lifecycle.buy_signature = signature.clone();
                    
                    let since_decision = lifecycle.buy_decision_time
                        .map(|t| t.elapsed().as_millis())
                        .unwrap_or(0);
                    
                    info!(
                        "📤 [LIFECYCLE] BUY_TX_SENT | mint: {}...{} | sig: {} | latency: {}ms",
                        &mint[..8], &mint[mint.len()-8..],
                        signature.as_deref().unwrap_or("pending"),
                        since_decision
                    );
                }
            }
            
            LifecycleEvent::BuyConfirmed {
                mint,
                signature,
                actual_tokens,
                actual_sol,
                fees_lamports,
                confirmation_time_ms,
            } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.buy_confirmed_time = Some(Instant::now());
                    lifecycle.buy_signature = Some(signature.clone());
                    lifecycle.entry_sol_actual = Some(actual_sol);
                    lifecycle.total_fees_lamports += fees_lamports;
                    
                    let since_sent = lifecycle.buy_tx_sent_time
                        .map(|t| t.elapsed().as_millis())
                        .unwrap_or(0);
                    
                    info!(
                        "✅ [LIFECYCLE] BUY_CONFIRMED | mint: {}...{} | sig: {}...{} | tokens: {} | sol: {} lamports | fees: {} lamports | conf_time: {}ms | latency: {}ms",
                        &mint[..8], &mint[mint.len()-8..],
                        &signature[..8], &signature[signature.len()-8..],
                        actual_tokens, actual_sol, fees_lamports, confirmation_time_ms, since_sent
                    );
                }
            }
            
            LifecycleEvent::PriceUpdate {
                mint,
                old_price_sol,
                new_price_sol,
                mc_sol,
                update_source,
                hold_duration_secs,
            } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.price_update_count += 1;
                    lifecycle.last_price_sol = new_price_sol;
                    lifecycle.last_mc_sol = mc_sol;
                    
                    let price_change_pct = ((new_price_sol - old_price_sol) / old_price_sol) * 100.0;
                    let entry_pnl_pct = ((new_price_sol - lifecycle.entry_price_sol) / lifecycle.entry_price_sol) * 100.0;
                    
                    // Log every 5th update or significant price changes (>5%)
                    if lifecycle.price_update_count % 5 == 0 || price_change_pct.abs() > 5.0 {
                        info!(
                            "📈 [LIFECYCLE] PRICE_UPDATE #{} | mint: {}...{} | price: {:.9} → {:.9} SOL ({:+.2}%) | mc: {:.2} SOL | entry_pnl: {:+.2}% | hold: {}s | source: {}",
                            lifecycle.price_update_count,
                            &mint[..8], &mint[mint.len()-8..],
                            old_price_sol, new_price_sol, price_change_pct,
                            mc_sol, entry_pnl_pct, hold_duration_secs, update_source
                        );
                    }
                }
            }
            
            LifecycleEvent::ExitConditionTriggered {
                mint,
                reason,
                exit_percent,
                current_price_sol,
                entry_price_sol,
                pnl_percent,
                hold_duration_secs,
            } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.exit_condition_time = Some(Instant::now());
                    lifecycle.exit_reason = Some(reason.clone());
                    
                    info!(
                        "🚨 [LIFECYCLE] EXIT_CONDITION | mint: {}...{} | reason: {} | exit: {}% | price: {:.9} → {:.9} SOL | pnl: {:+.2}% | hold: {}s",
                        &mint[..8], &mint[mint.len()-8..],
                        reason, exit_percent, entry_price_sol, current_price_sol, pnl_percent, hold_duration_secs
                    );
                }
            }
            
            LifecycleEvent::SellDecision {
                mint,
                size_sol,
                exit_percent,
                reason,
            } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.sell_decision_time = Some(Instant::now());
                    
                    let since_exit_condition = lifecycle.exit_condition_time
                        .map(|t| t.elapsed().as_millis())
                        .unwrap_or(0);
                    
                    info!(
                        "🔴 [LIFECYCLE] SELL_DECISION | mint: {}...{} | size: {:.3} SOL | exit: {}% | reason: {} | latency: {}ms",
                        &mint[..8], &mint[mint.len()-8..],
                        size_sol, exit_percent, reason, since_exit_condition
                    );
                }
            }
            
            LifecycleEvent::SellTxSent { mint, signature } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.sell_tx_sent_time = Some(Instant::now());
                    lifecycle.sell_signature = signature.clone();
                    
                    let since_decision = lifecycle.sell_decision_time
                        .map(|t| t.elapsed().as_millis())
                        .unwrap_or(0);
                    
                    info!(
                        "📤 [LIFECYCLE] SELL_TX_SENT | mint: {}...{} | sig: {} | latency: {}ms",
                        &mint[..8], &mint[mint.len()-8..],
                        signature.as_deref().unwrap_or("pending"),
                        since_decision
                    );
                }
            }
            
            LifecycleEvent::SellConfirmed {
                mint,
                signature,
                actual_sol,
                fees_lamports,
                confirmation_time_ms,
            } => {
                if let Some(lifecycle) = self.lifecycles.get_mut(&mint) {
                    lifecycle.sell_confirmed_time = Some(Instant::now());
                    lifecycle.sell_signature = Some(signature.clone());
                    lifecycle.exit_sol_actual = Some(actual_sol);
                    lifecycle.total_fees_lamports += fees_lamports;
                    
                    let since_sent = lifecycle.sell_tx_sent_time
                        .map(|t| t.elapsed().as_millis())
                        .unwrap_or(0);
                    
                    info!(
                        "✅ [LIFECYCLE] SELL_CONFIRMED | mint: {}...{} | sig: {}...{} | sol: {} lamports | fees: {} lamports | conf_time: {}ms | latency: {}ms",
                        &mint[..8], &mint[mint.len()-8..],
                        &signature[..8], &signature[signature.len()-8..],
                        actual_sol, fees_lamports, confirmation_time_ms, since_sent
                    );
                    
                    // Trigger position closed event
                    self.close_position(&mint);
                }
            }
            
            LifecycleEvent::PositionClosed { .. } => {
                // Already handled in close_position()
            }
        }
    }
    
    /// Mark position as closed and log final metrics
    fn close_position(&mut self, mint: &str) {
        if let Some(lifecycle) = self.lifecycles.remove(mint) {
            let total_duration = lifecycle.entry_time.elapsed();
            let total_secs = total_duration.as_secs();
            
            // Calculate P&L
            let entry_sol = lifecycle.entry_sol_actual.unwrap_or(0) as f64 / 1e9;
            let exit_sol = lifecycle.exit_sol_actual.unwrap_or(0) as f64 / 1e9;
            let fees_sol = lifecycle.total_fees_lamports as f64 / 1e9;
            let net_pnl_sol = exit_sol - entry_sol - fees_sol;
            let net_pnl_usd = net_pnl_sol * 150.0; // TODO: Use real SOL price
            let roi_percent = if entry_sol > 0.0 {
                (net_pnl_sol / entry_sol) * 100.0
            } else {
                0.0
            };
            
            self.completed_count += 1;
            
            info!(
                "🏁 [LIFECYCLE] POSITION_CLOSED #{} | mint: {}...{} | hold: {}s | entry: {:.4} SOL | exit: {:.4} SOL | fees: {:.4} SOL | net_pnl: {:+.4} SOL (${:+.2}) | roi: {:+.2}% | updates: {} | trigger: {}",
                self.completed_count,
                &mint[..8], &mint[mint.len()-8..],
                total_secs, entry_sol, exit_sol, fees_sol, net_pnl_sol, net_pnl_usd, roi_percent,
                lifecycle.price_update_count, lifecycle.trigger_source
            );
            
            // Log detailed timeline
            if let (Some(buy_decision), Some(buy_sent), Some(buy_conf)) = (
                lifecycle.buy_decision_time,
                lifecycle.buy_tx_sent_time,
                lifecycle.buy_confirmed_time,
            ) {
                let decision_to_sent = buy_sent.duration_since(buy_decision).as_millis();
                let sent_to_conf = buy_conf.duration_since(buy_sent).as_millis();
                let total_entry = buy_conf.duration_since(buy_decision).as_millis();
                
                info!(
                    "  📊 Entry Timeline: decision→sent: {}ms | sent→conf: {}ms | total: {}ms",
                    decision_to_sent, sent_to_conf, total_entry
                );
            }
            
            if let (Some(exit_cond), Some(sell_dec), Some(sell_conf)) = (
                lifecycle.exit_condition_time,
                lifecycle.sell_decision_time,
                lifecycle.sell_confirmed_time,
            ) {
                let cond_to_dec = sell_dec.duration_since(exit_cond).as_millis();
                let dec_to_conf = sell_conf.duration_since(sell_dec).as_millis();
                let total_exit = sell_conf.duration_since(exit_cond).as_millis();
                
                info!(
                    "  📊 Exit Timeline: condition→decision: {}ms | decision→conf: {}ms | total: {}ms",
                    cond_to_dec, dec_to_conf, total_exit
                );
            }
        }
    }
    
    /// Get statistics
    pub fn get_stats(&self) -> (usize, u64) {
        (self.lifecycles.len(), self.completed_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lifecycle_tracking() {
        let mut logger = PositionLifecycleLogger::new();
        let mint = "7GCihgDB8fe6KNjn2MYtkzZcRjQy3t9GHdC8uHYmW2hr".to_string();
        
        // BUY phase
        logger.log_event(LifecycleEvent::BuyDecision {
            mint: mint.clone(),
            size_sol: 0.1,
            size_usd: 15.0,
            confidence: 75,
            entry_price_sol: 0.00000123,
            trigger_source: "rank".to_string(),
        });
        
        logger.log_event(LifecycleEvent::BuyTxSent {
            mint: mint.clone(),
            signature: Some("sig123".to_string()),
        });
        
        // Price updates
        logger.log_event(LifecycleEvent::PriceUpdate {
            mint: mint.clone(),
            old_price_sol: 0.00000123,
            new_price_sol: 0.00000145,
            mc_sol: 85000.0,
            update_source: "gRPC".to_string(),
            hold_duration_secs: 2,
        });
        
        // Exit
        logger.log_event(LifecycleEvent::ExitConditionTriggered {
            mint: mint.clone(),
            reason: "profit_target".to_string(),
            exit_percent: 100,
            current_price_sol: 0.00000145,
            entry_price_sol: 0.00000123,
            pnl_percent: 17.9,
            hold_duration_secs: 5,
        });
        
        logger.log_event(LifecycleEvent::SellDecision {
            mint: mint.clone(),
            size_sol: 0.1,
            exit_percent: 100,
            reason: "profit_target".to_string(),
        });
        
        assert_eq!(logger.lifecycles.len(), 1);
        assert_eq!(logger.completed_count, 0);
    }
}
