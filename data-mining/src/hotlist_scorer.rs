/// Hotlist Scorer - Real-time explosive token detection
/// 
/// Periodically scores recent launches using 7-signal algorithm:
/// - Signal 1: Creator wallet reputation (+2.0)
/// - Signal 2: Speed of first 10 buyers (+2.0)
/// - Signal 3: Liquidity vs MC ratio <4 (+1.5)
/// - Signal 4: Wallet overlap with winners (+2.0)
/// - Signal 5: Buy concentration <70% (+1.0)
/// - Signal 6: Volume acceleration 2x (+1.5)
/// - Signal 7: MC velocity >1000 SOL/min (+3.0)

use std::sync::{Arc, Mutex};
use std::time::Duration;
use anyhow::Result;
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::udp::AdvisorySender;
use crate::window_tracker::WindowTracker;

/// Configuration for hotlist scoring
pub struct HotlistScorerConfig {
    /// How often to run scoring (seconds)
    pub scoring_interval_sec: u64,
    /// Minimum age before scoring (seconds, to accumulate data)
    pub min_age_sec: i64,
    /// Maximum age to score (seconds, focus on fresh launches)
    pub max_age_sec: i64,
    /// Minimum score to broadcast to Brain
    pub min_broadcast_score: f64,
}

impl Default for HotlistScorerConfig {
    fn default() -> Self {
        Self {
            scoring_interval_sec: 5,    // Score every 5 seconds
            min_age_sec: 10,             // Wait 10s for data accumulation
            max_age_sec: 300,            // Only score tokens <5min old
            min_broadcast_score: 6.0,    // Broadcast score ≥6.0
        }
    }
}

/// Spawn background hotlist scorer task
pub fn spawn_hotlist_scorer(
    db: Arc<Mutex<Database>>,
    advisory_sender: Option<AdvisorySender>,
    window_tracker: Arc<Mutex<WindowTracker>>,
    config: HotlistScorerConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🎯 Hotlist Scorer: Started (interval={}s, min_score={:.1})", 
              config.scoring_interval_sec, config.min_broadcast_score);
        
        let mut interval = tokio::time::interval(Duration::from_secs(config.scoring_interval_sec));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = run_scoring_cycle(&db, &advisory_sender, &window_tracker, &config).await {
                warn!("⚠️  Hotlist scoring cycle failed: {}", e);
            }
        }
    })
}

/// Run one scoring cycle
async fn run_scoring_cycle(
    db: &Arc<Mutex<Database>>,
    advisory_sender: &Option<AdvisorySender>,
    window_tracker: &Arc<Mutex<WindowTracker>>,
    config: &HotlistScorerConfig,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let min_launch_time = now - config.max_age_sec;
    let max_launch_time = now - config.min_age_sec;
    
    // Get recent tokens to score
    let tokens = {
        let db_guard = db.lock().unwrap();
        get_recent_tokens(&db_guard, min_launch_time, max_launch_time)?
    };
    
    if tokens.is_empty() {
        return Ok(());
    }
    
    debug!("🎯 Scoring {} recent tokens...", tokens.len());
    
    for (mint, launch_time) in tokens {
        // Calculate 7-signal score
        let score_result = calculate_token_score(&db, &window_tracker, &mint, launch_time, now).await;
        
        match score_result {
            Ok(score_data) => {
                // Store in hotlist table
                {
                    let mut db_guard = db.lock().unwrap();
                    db_guard.upsert_hotlist(
                        &mint,
                        score_data.total,
                        score_data.creator,
                        score_data.buyer_speed,
                        score_data.liquidity,
                        score_data.wallet_overlap,
                        score_data.concentration,
                        score_data.volume_accel,
                        score_data.mc_velocity,
                        score_data.mc_vel_value,
                        score_data.unique_buyers,
                    )?;
                }
                
                // Broadcast to Brain if score is high enough
                if score_data.total >= config.min_broadcast_score {
                    if let Some(ref sender) = advisory_sender {
                        let confidence = ((score_data.total / 15.0) * 100.0).min(100.0) as u8;
                        let horizon_sec = 60; // Valid for 60 seconds
                        
                        let _ = sender.send_rank_opportunity(&mint, horizon_sec, confidence);
                        
                        info!("🔥 HIGH-SCORE TOKEN: {} | score: {:.1}/15.0 | MC velocity: {:.0} SOL/min | conf: {}%",
                              &mint[..8], score_data.total, score_data.mc_vel_value, confidence);
                    }
                }
            }
            Err(e) => {
                debug!("⚠️  Failed to score {}: {}", &mint[..8], e);
            }
        }
    }
    
    // Cleanup old entries (>5 minutes)
    {
        let mut db_guard = db.lock().unwrap();
        db_guard.cleanup_old_hotlist(300)?;
    }
    
    Ok(())
}

/// Score breakdown for a token
#[derive(Debug)]
struct ScoreBreakdown {
    total: f64,
    creator: f64,
    buyer_speed: f64,
    liquidity: f64,
    wallet_overlap: f64,
    concentration: f64,
    volume_accel: f64,
    mc_velocity: f64,
    mc_vel_value: f64,
    unique_buyers: u32,
}

/// Calculate 7-signal score for a token
async fn calculate_token_score(
    db: &Arc<Mutex<Database>>,
    window_tracker: &Arc<Mutex<WindowTracker>>,
    mint: &str,
    launch_time: i64,
    now: i64,
) -> Result<ScoreBreakdown> {
    let age_sec = now - launch_time;
    
    // Get recent trades, creator stats, initial liquidity, and MC velocity
    let (trades, creator_stats, initial_liquidity, mc_velocity_opt) = {
        let db_guard = db.lock().unwrap();
        let trades = get_recent_trades(&db_guard, mint, 60)?;
        
        // Get creator wallet and stats for Signal 1
        let creator_stats = match db_guard.get_creator_wallet(mint) {
            Ok(creator_wallet) => db_guard.get_creator_stats(&creator_wallet).ok().flatten(),
            Err(_) => None,
        };
        
        // Get initial liquidity for Signal 3
        let initial_liquidity = db_guard.get_initial_liquidity(mint).ok().flatten();
        
        // Release db lock before acquiring window_tracker lock
        drop(db_guard);
        
        // Get MC velocity from window tracker
        let mut tracker_guard = window_tracker.lock().unwrap();
        // We need current MC to get metrics, but we'll estimate from recent price
        let estimated_mc = if let Some((_trader, _side, last_price)) = trades.last() {
            last_price * 1_000_000_000.0 // Rough estimate: price * 1B tokens supply
        } else {
            0.0
        };
        let mc_velocity = tracker_guard.get_metrics_if_ready(mint, estimated_mc)
            .map(|metrics| metrics.mc_velocity_sol_per_min);
        
        (trades, creator_stats, initial_liquidity, mc_velocity)
    };
    
    if trades.is_empty() {
        anyhow::bail!("No trades found for {}", mint);
    }
    
    let mut score = ScoreBreakdown {
        total: 0.0,
        creator: 0.0,
        buyer_speed: 0.0,
        liquidity: 0.0,
        wallet_overlap: 0.0,
        concentration: 0.0,
        volume_accel: 0.0,
        mc_velocity: 0.0,
        mc_vel_value: 0.0,
        unique_buyers: 0,
    };
    
    // Signal 1: Creator wallet reputation
    if let Some((net_pnl, create_count)) = creator_stats {
        if net_pnl >= 500.0 && create_count >= 5 {
            score.creator = 2.0; // Proven creator (>500 SOL profit, 5+ tokens)
            debug!("🏆 Signal 1: {} | creator profit {:.0} SOL, {} tokens (proven)", mint, net_pnl, create_count);
        } else if net_pnl >= 200.0 && create_count >= 3 {
            score.creator = 1.5; // Good creator (>200 SOL profit, 3+ tokens)
            debug!("⭐ Signal 1: {} | creator profit {:.0} SOL, {} tokens (good)", mint, net_pnl, create_count);
        } else if net_pnl >= 50.0 {
            score.creator = 1.0; // Profitable creator (>50 SOL profit)
            debug!("👍 Signal 1: {} | creator profit {:.0} SOL, {} tokens (profitable)", mint, net_pnl, create_count);
        } else {
            score.creator = 0.0; // New or unprofitable creator
        }
    } else {
        score.creator = 0.0; // No stats for creator yet
    }
    
    // Signal 2: Speed of first 10 buyers
    let first_10_buyers = trades.iter()
        .filter(|(_, side, _)| side == "buy")
        .take(10)
        .count();
    
    if first_10_buyers >= 10 && age_sec <= 30 {
        score.buyer_speed = 2.0; // All 10 buyers in 30s = max score
    } else if first_10_buyers >= 10 && age_sec <= 60 {
        score.buyer_speed = 1.5; // All 10 buyers in 60s
    } else if first_10_buyers >= 7 {
        score.buyer_speed = 1.0; // Partial credit
    }
    
    // Signal 3: Liquidity ratio (initial liquidity / estimated market cap)
    if let Some(liquidity) = initial_liquidity {
        // Estimate current market cap from recent price
        let estimated_mc = if let Some((_trader, _side, last_price)) = trades.last() {
            last_price * 1_000_000_000.0 // price * 1B supply
        } else {
            0.0
        };
        
        if estimated_mc > 0.0 && liquidity > 0.0 {
            let liquidity_ratio = liquidity / estimated_mc;
            
            if liquidity_ratio < 0.03 {
                score.liquidity = 1.5; // Healthy liquidity (<3% of MC)
                debug!("✅ Signal 3: {} | liquidity ratio {:.1}% (healthy)", mint, liquidity_ratio * 100.0);
            } else if liquidity_ratio < 0.05 {
                score.liquidity = 1.0; // Moderate liquidity (3-5% of MC)
                debug!("⚠️  Signal 3: {} | liquidity ratio {:.1}% (moderate)", mint, liquidity_ratio * 100.0);
            } else {
                score.liquidity = 0.0; // Thin liquidity (>5% of MC - red flag)
                debug!("🚨 Signal 3: {} | liquidity ratio {:.1}% (thin)", mint, liquidity_ratio * 100.0);
            }
        } else {
            score.liquidity = 0.0; // Can't calculate ratio
        }
    } else {
        score.liquidity = 0.0; // No liquidity data
    }
    
    // Signal 4: Wallet overlap with proven winners
    let profitable_wallets = {
        let db_guard = db.lock().unwrap();
        db_guard.get_profitable_wallets(100.0, 0.5, 100).unwrap_or_default()
    };
    
    if !profitable_wallets.is_empty() {
        // Get unique buyers from trades
        let buyers: std::collections::HashSet<_> = trades.iter()
            .filter(|(_, side, _)| side == "buy")
            .map(|(buyer, _, _)| buyer.clone())
            .collect();
        
        // Count how many proven winners bought this token
        let overlap_count = buyers.iter()
            .filter(|buyer| profitable_wallets.contains(buyer))
            .count();
        
        if overlap_count >= 3 {
            score.wallet_overlap = 2.0; // 3+ proven winners = max score
            debug!("🎯 Signal 4: {} | {}/3 proven winners detected", mint, overlap_count);
        } else if overlap_count >= 2 {
            score.wallet_overlap = 1.5; // 2 proven winners
            debug!("🎯 Signal 4: {} | {}/3 proven winners detected", mint, overlap_count);
        } else if overlap_count >= 1 {
            score.wallet_overlap = 1.0; // 1 proven winner
            debug!("🎯 Signal 4: {} | {}/3 proven winners detected", mint, overlap_count);
        } else {
            score.wallet_overlap = 0.0; // No overlap
        }
    } else {
        // No profitable wallets in database yet
        score.wallet_overlap = 0.0;
    }
    
    // Signal 5: Buy concentration (lower = better distribution)
    let concentration_pct = calculate_buy_concentration(&trades);
    if concentration_pct < 70.0 {
        score.concentration = 1.0; // Healthy distribution
        debug!("✅ Signal 5: {} | concentration {:.1}% (healthy)", mint, concentration_pct);
    } else if concentration_pct < 80.0 {
        score.concentration = 0.5; // Moderate concentration
        debug!("⚠️  Signal 5: {} | concentration {:.1}% (moderate)", mint, concentration_pct);
    } else {
        score.concentration = 0.0; // High concentration (red flag)
        debug!("🚨 Signal 5: {} | concentration {:.1}% (high risk)", mint, concentration_pct);
    }
    
    // Signal 6: Volume acceleration (compare recent vs baseline volume)
    let volume_acceleration = calculate_volume_acceleration(&trades, age_sec);
    if volume_acceleration >= 2.0 {
        score.volume_accel = 1.5; // 2X+ acceleration (explosive)
        debug!("🚀 Signal 6: {} | volume acceleration {:.2}X (explosive)", mint, volume_acceleration);
    } else if volume_acceleration >= 1.5 {
        score.volume_accel = 1.0; // 1.5X+ acceleration (strong)
        debug!("📈 Signal 6: {} | volume acceleration {:.2}X (strong)", mint, volume_acceleration);
    } else {
        score.volume_accel = 0.0; // Low acceleration
        debug!("📊 Signal 6: {} | volume acceleration {:.2}X (low)", mint, volume_acceleration);
    }
    
    // Signal 7: MC velocity (from window_tracker)
    if let Some(velocity) = mc_velocity_opt {
        score.mc_vel_value = velocity;
        
        if velocity >= 1000.0 {
            score.mc_velocity = 3.0; // Explosive growth (>1000 SOL/min)
            info!("🚀 Signal 7: {} | MC velocity {:.0} SOL/min (EXPLOSIVE)", mint, velocity);
        } else if velocity >= 500.0 {
            score.mc_velocity = 2.0; // Strong growth (500-1000 SOL/min)
            info!("📈 Signal 7: {} | MC velocity {:.0} SOL/min (strong)", mint, velocity);
        } else if velocity >= 200.0 {
            score.mc_velocity = 1.0; // Moderate growth (200-500 SOL/min)
            debug!("📊 Signal 7: {} | MC velocity {:.0} SOL/min (moderate)", mint, velocity);
        } else {
            score.mc_velocity = 0.0; // Low velocity
            debug!("Signal 7: {} | MC velocity {:.0} SOL/min (low)", mint, velocity);
        }
    } else {
        // No metrics available yet
        score.mc_velocity = 0.0;
        score.mc_vel_value = 0.0;
    }
    
    // Count unique buyers
    let unique_buyers: std::collections::HashSet<_> = trades.iter()
        .filter(|(_, side, _)| side == "buy")
        .map(|(buyer, _, _)| buyer)
        .collect();
    score.unique_buyers = unique_buyers.len() as u32;
    
    // Calculate total
    score.total = score.creator + score.buyer_speed + score.liquidity 
                + score.wallet_overlap + score.concentration 
                + score.volume_accel + score.mc_velocity;
    
    Ok(score)
}

/// Get recent tokens to score
fn get_recent_tokens(
    db: &Database,
    min_launch_time: i64,
    max_launch_time: i64,
) -> Result<Vec<(String, i64)>> {
    db.get_recent_tokens_for_scoring(min_launch_time, max_launch_time)
}

/// Get recent trades for a token (buyer, side, amount_sol)
fn get_recent_trades(
    db: &Database,
    mint: &str,
    lookback_sec: i64,
) -> Result<Vec<(String, String, f64)>> {
    db.get_recent_trades_for_scoring(mint, lookback_sec)
}

/// Calculate volume acceleration by comparing recent vs baseline period
/// Returns acceleration ratio (recent_volume / baseline_volume)
fn calculate_volume_acceleration(trades: &[(String, String, f64)], age_sec: i64) -> f64 {
    // Need at least 60 seconds of data for comparison
    if age_sec < 60 || trades.is_empty() {
        return 1.0; // No acceleration if insufficient data
    }
    
    // Split trades list in half as proxy for baseline (older) vs recent (newer)
    // Trades are ordered by time, so second half is more recent
    let midpoint = trades.len() / 2;
    
    let mut baseline_volume = 0.0;
    let mut recent_volume = 0.0;
    
    for (i, (_, side, amount)) in trades.iter().enumerate() {
        if side == "buy" {
            if i < midpoint {
                baseline_volume += amount; // Older half (30-60s ago)
            } else {
                recent_volume += amount;   // Newer half (0-30s ago)
            }
        }
    }
    
    // Calculate acceleration ratio
    if baseline_volume < 0.1 {
        // Not enough baseline volume to compare
        return 1.0;
    }
    
    recent_volume / baseline_volume
}


/// Get creator wallet for a token
fn get_creator_wallet(db: &Database, mint: &str) -> Result<String> {
    db.get_creator_wallet(mint)
}

/// Calculate top-3 buyer concentration percentage
/// Returns: (top 3 buyers SOL) / (total buys SOL) * 100
/// Lower percentage = better distribution (less manipulation risk)
fn calculate_buy_concentration(trades: &[(String, String, f64)]) -> f64 {
    use std::collections::HashMap;
    
    // Aggregate buys by trader
    let mut buyer_amounts: HashMap<String, f64> = HashMap::new();
    for (trader, side, amount) in trades {
        if side == "buy" {
            *buyer_amounts.entry(trader.clone()).or_insert(0.0) += amount;
        }
    }
    
    if buyer_amounts.is_empty() {
        return 100.0; // No buys = max concentration (red flag)
    }
    
    // Edge case: Only 1-2 buyers means high concentration by definition
    if buyer_amounts.len() <= 2 {
        return 100.0; // 1-2 buyers = automatically high risk
    }
    
    // Get top 3 buyers
    let mut amounts: Vec<f64> = buyer_amounts.values().copied().collect();
    amounts.sort_by(|a, b| b.partial_cmp(a).unwrap());
    
    let top3_sum: f64 = amounts.iter().take(3).sum();
    let total_sum: f64 = amounts.iter().sum();
    
    if total_sum > 0.0 {
        (top3_sum / total_sum) * 100.0
    } else {
        100.0 // No volume = red flag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concentration_healthy_distribution() {
        // 10 buyers with equal amounts (10 SOL each)
        let trades: Vec<(String, String, f64)> = (0..10)
            .map(|i| (format!("buyer_{}", i), "buy".to_string(), 10.0))
            .collect();
        
        let concentration = calculate_buy_concentration(&trades);
        // Top 3 = 30 SOL, Total = 100 SOL => 30%
        assert!((concentration - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_concentration_manipulated() {
        // 1 whale (90 SOL) + 9 small buyers (1 SOL each)
        let mut trades = vec![("whale".to_string(), "buy".to_string(), 90.0)];
        for i in 0..9 {
            trades.push((format!("buyer_{}", i), "buy".to_string(), 1.0));
        }
        
        let concentration = calculate_buy_concentration(&trades);
        // Top 3 = 90+1+1=92 SOL, Total = 99 SOL => ~93%
        assert!(concentration > 90.0);
    }

    #[test]
    fn test_concentration_edge_case_one_buyer() {
        let trades = vec![("buyer_1".to_string(), "buy".to_string(), 100.0)];
        let concentration = calculate_buy_concentration(&trades);
        // Only 1 buyer => 100% concentration
        assert_eq!(concentration, 100.0);
    }

    #[test]
    fn test_concentration_edge_case_two_buyers() {
        let trades = vec![
            ("buyer_1".to_string(), "buy".to_string(), 60.0),
            ("buyer_2".to_string(), "buy".to_string(), 40.0),
        ];
        let concentration = calculate_buy_concentration(&trades);
        // Only 2 buyers => 100% concentration (red flag)
        assert_eq!(concentration, 100.0);
    }

    #[test]
    fn test_concentration_no_buys() {
        let trades = vec![
            ("seller_1".to_string(), "sell".to_string(), 50.0),
            ("seller_2".to_string(), "sell".to_string(), 30.0),
        ];
        let concentration = calculate_buy_concentration(&trades);
        // No buys => 100% concentration
        assert_eq!(concentration, 100.0);
    }

    #[test]
    fn test_concentration_moderate() {
        // 5 buyers: 25, 20, 15, 15, 25 SOL
        let trades = vec![
            ("buyer_1".to_string(), "buy".to_string(), 25.0),
            ("buyer_2".to_string(), "buy".to_string(), 20.0),
            ("buyer_3".to_string(), "buy".to_string(), 15.0),
            ("buyer_4".to_string(), "buy".to_string(), 15.0),
            ("buyer_5".to_string(), "buy".to_string(), 25.0),
        ];
        let concentration = calculate_buy_concentration(&trades);
        // Top 3 = 25+25+20=70 SOL, Total = 100 SOL => 70%
        assert!((concentration - 70.0).abs() < 0.1);
    }
}
