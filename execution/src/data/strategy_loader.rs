use anyhow::{Result, bail};
use rusqlite::{Connection, params};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// Configuration for strategy loading
#[derive(Clone, Debug)]
pub struct StrategyConfig {
    pub path: String,
    pub reload_secs: u64,
    pub min_confidence: f64,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            path: "data/strategies.db".to_string(),
            reload_secs: 600, // 10 minutes
            min_confidence: 0.45,
        }
    }
}

/// Live strategy from backtesting database
#[derive(Debug, Clone)]
pub struct LiveStrategy {
    pub id: String,
    pub strategy_type: String,   // e.g. "pattern_based", "scalp"
    pub entry_rule: String,       // e.g. "Enter when volume > 50 SOL detected"
    pub exit_rule: String,        // e.g. "Exit at $2 profit or 120s"
    pub profit_target_usd: f64,   // take-profit target
    pub holding_time_sec: f64,    // max hold time
    pub latency_ms_used: f64,     // latency assumption from backtest
    pub slippage_percent: f64,    // slippage assumption from backtest
    pub win_rate: f64,            // win rate from backtest
    pub avg_profit_usd: f64,      // average profit from backtest
    pub profit_factor: f64,       // profit factor from backtest
    pub execution_confidence: f64, // confidence score
    pub rank: i64,                // strategy rank (1 = best)
    pub score: f64,               // overall score
    pub enabled: bool,            // whether strategy is active
}

/// Parsed rules for quick decision making
#[derive(Debug, Clone, Default)]
pub struct ParsedRules {
    pub min_volume_sol: Option<f64>,
    pub min_unique_buyers: Option<u32>,
    pub profit_target_usd: Option<f64>,
    pub max_hold_sec: Option<u64>,
}

impl LiveStrategy {
    /// Parse entry and exit rules into structured data
    pub fn parse_rules(&self) -> ParsedRules {
        let mut rules = ParsedRules::default();
        
        // Parse entry rule for volume threshold
        if self.entry_rule.contains("volume >") {
            if let Some(vol_str) = self.entry_rule.split("volume >").nth(1) {
                if let Some(num_str) = vol_str.split_whitespace().next() {
                    rules.min_volume_sol = num_str.parse().ok();
                }
            }
        }
        
        // Parse entry rule for buyer threshold
        if self.entry_rule.contains("buyers") {
            if let Some(buyer_str) = self.entry_rule.split_whitespace()
                .find(|s| s.ends_with('+') || s.parse::<u32>().is_ok()) {
                let num_str = buyer_str.trim_end_matches('+');
                rules.min_unique_buyers = num_str.parse().ok();
            }
        }
        
        // Parse exit rule for profit target
        if self.exit_rule.contains("$") {
            if let Some(profit_str) = self.exit_rule.split('$').nth(1) {
                if let Some(num_str) = profit_str.split_whitespace().next() {
                    rules.profit_target_usd = num_str.parse().ok();
                }
            }
        }
        
        // Parse exit rule for time limit
        if self.exit_rule.contains("s") {
            if let Some(time_part) = self.exit_rule.split(" or ").nth(1) {
                let time_str = time_part.trim_end_matches('s');
                rules.max_hold_sec = time_str.parse().ok();
            }
        }
        
        rules
    }
    
    /// Validate strategy parameters are within safe bounds
    pub fn is_valid(&self) -> bool {
        self.slippage_percent >= 0.5 && self.slippage_percent <= 50.0 &&
        self.holding_time_sec >= 5.0 && self.holding_time_sec <= 3600.0 &&
        self.latency_ms_used >= 10.0 && self.latency_ms_used <= 500.0 &&
        self.profit_target_usd >= 0.0 &&
        self.execution_confidence >= 0.0 && self.execution_confidence <= 1.0 &&
        self.enabled
    }
}

/// Load strategies from SQLite database
pub fn load_live_strategies(db_path: &str, min_conf: f64) -> Result<Vec<LiveStrategy>> {
    // Open database connection
    let conn = Connection::open(db_path)?;
    
    // Verify table exists
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='live_strategies';",
        [], 
        |r| r.get(0)
    )?;
    
    if exists == 0 {
        bail!("live_strategies table not found in {}", db_path);
    }
    
    // Query enabled strategies with minimum confidence
    let mut stmt = conn.prepare(
        r#"SELECT strategy_id, strategy_type, entry_rule, exit_rule,
                  profit_target_usd, holding_time_sec, latency_ms_used, slippage_percent,
                  win_rate, avg_profit_usd, profit_factor, execution_confidence,
                  rank, score, enabled
           FROM live_strategies
           WHERE enabled = 1 AND execution_confidence >= ?
           ORDER BY rank ASC, score DESC;"#
    )?;
    
    let rows = stmt.query_map([min_conf], |row| {
        Ok(LiveStrategy {
            id: row.get(0)?,
            strategy_type: row.get(1)?,
            entry_rule: row.get(2)?,
            exit_rule: row.get(3)?,
            profit_target_usd: row.get(4)?,
            holding_time_sec: row.get(5)?,
            latency_ms_used: row.get(6)?,
            slippage_percent: row.get(7)?,
            win_rate: row.get(8)?,
            avg_profit_usd: row.get(9)?,
            profit_factor: row.get(10)?,
            execution_confidence: row.get(11)?,
            rank: row.get(12)?,
            score: row.get(13)?,
            enabled: row.get::<_, i64>(14)? == 1,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;
    
    if rows.is_empty() {
        bail!("No enabled strategies meeting confidence >= {}", min_conf);
    }
    
    // Filter to only valid strategies
    let valid_strategies: Vec<_> = rows.into_iter()
        .filter(|s| s.is_valid())
        .collect();
    
    if valid_strategies.is_empty() {
        bail!("No valid strategies after validation");
    }
    
    Ok(valid_strategies)
}

/// Thread-safe strategy store
pub type StrategyStore = Arc<RwLock<Vec<LiveStrategy>>>;

/// Initialize strategy store with initial strategies
pub fn strategy_store_init(initial: Vec<LiveStrategy>) -> StrategyStore {
    Arc::new(RwLock::new(initial))
}

/// Background task to periodically reload strategies
pub async fn strategy_reloader(store: StrategyStore, cfg: StrategyConfig) {
    loop {
        tokio::time::sleep(Duration::from_secs(cfg.reload_secs)).await;
        
        match load_live_strategies(&cfg.path, cfg.min_confidence) {
            Ok(new_list) if !new_list.is_empty() => {
                let old_count = store.read().await.len();
                *store.write().await = new_list.clone();
                log::info!("🔄 Strategies reloaded from {} ({} -> {} strategies)", 
                    cfg.path, old_count, new_list.len());
                
                // Log top 3 strategies
                for (i, s) in new_list.iter().take(3).enumerate() {
                    log::info!("  {}. [{}] {} | entry='{}' | tp=${:.2} | hold={}s | conf={:.1}%",
                        i + 1, s.strategy_type, s.id, s.entry_rule, 
                        s.profit_target_usd, s.holding_time_sec, s.execution_confidence * 100.0);
                }
            }
            Ok(_) => {
                log::warn!("Strategy reload returned 0 rows; keeping previous set");
            }
            Err(e) => {
                log::error!("Strategy reload failed: {} - keeping previous set", e);
            }
        }
    }
}

/// Context for live trading decisions
#[derive(Debug, Clone, Default)]
pub struct LiveContext {
    pub volume_last_5s_sol: f64,
    pub unique_buyers_last_2s: u32,
    pub token_age_seconds: u64,
    pub price_surge_detected: bool,
}

/// Select best strategy based on current market context
pub fn pick_strategy<'a>(
    strategies: &'a [LiveStrategy],
    ctx: &LiveContext,
) -> Option<&'a LiveStrategy> {
    if strategies.is_empty() {
        return None;
    }
    
    // Prefer pattern-based strategies when conditions match
    
    // Check for volume-based strategies
    for strategy in strategies {
        if strategy.strategy_type == "pattern_based" {
            let rules = strategy.parse_rules();
            
            // Match volume threshold
            if let Some(min_vol) = rules.min_volume_sol {
                if ctx.volume_last_5s_sol >= min_vol {
                    log::info!("📊 Selected strategy: {} (volume {:.1} >= {:.1} SOL)", 
                        strategy.id, ctx.volume_last_5s_sol, min_vol);
                    return Some(strategy);
                }
            }
            
            // Match buyer threshold
            if let Some(min_buyers) = rules.min_unique_buyers {
                if ctx.unique_buyers_last_2s >= min_buyers {
                    log::info!("📊 Selected strategy: {} (buyers {} >= {})", 
                        strategy.id, ctx.unique_buyers_last_2s, min_buyers);
                    return Some(strategy);
                }
            }
        }
    }
    
    // Fallback to scalp strategy or first available
    if let Some(scalp) = strategies.iter().find(|s| s.strategy_type == "scalp") {
        log::info!("📊 Selected scalp strategy: {}", scalp.id);
        Some(scalp)
    } else {
        log::info!("📊 Selected default strategy: {}", strategies[0].id);
        Some(&strategies[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_volume_rule() {
        let strategy = LiveStrategy {
            id: "test".to_string(),
            strategy_type: "pattern_based".to_string(),
            entry_rule: "Enter when volume > 50 SOL detected".to_string(),
            exit_rule: "Exit at $2 profit or 120s".to_string(),
            profit_target_usd: 2.0,
            holding_time_sec: 120.0,
            latency_ms_used: 85.0,
            slippage_percent: 10.0,
            win_rate: 0.65,
            avg_profit_usd: 2.2,
            profit_factor: 15.0,
            execution_confidence: 0.67,
            rank: 1,
            score: 0.82,
            enabled: true,
        };
        
        let rules = strategy.parse_rules();
        assert_eq!(rules.min_volume_sol, Some(50.0));
        assert_eq!(rules.profit_target_usd, Some(2.0));
        assert_eq!(rules.max_hold_sec, Some(120));
    }
    
    #[test]
    fn test_strategy_validation() {
        let valid = LiveStrategy {
            id: "test".to_string(),
            strategy_type: "pattern_based".to_string(),
            entry_rule: "test".to_string(),
            exit_rule: "test".to_string(),
            profit_target_usd: 2.0,
            holding_time_sec: 120.0,
            latency_ms_used: 85.0,
            slippage_percent: 10.0,
            win_rate: 0.65,
            avg_profit_usd: 2.2,
            profit_factor: 15.0,
            execution_confidence: 0.67,
            rank: 1,
            score: 0.82,
            enabled: true,
        };
        
        assert!(valid.is_valid());
        
        // Test invalid slippage
        let mut invalid = valid.clone();
        invalid.slippage_percent = 60.0;
        assert!(!invalid.is_valid());
    }
}
