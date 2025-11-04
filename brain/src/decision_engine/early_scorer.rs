//! 🎯 Early Scoring Engine - 7-Signal Algorithm
//!
//! Predicts which tokens will reach 1M+ market cap before they explode.
//! Uses 7 signals from on-chain data to score new launches in real-time.
//!
//! Score ≥ 6.0 → High-confidence entry trigger
//! Score ≥ 8.0 → Max position size
//!
//! ## Signals:
//! 1. Creator wallet reputation (+2.0)
//! 2. Speed of first 10 buyers (+2.0)
//! 3. Liquidity vs MC ratio (+1.5)
//! 4. Wallet overlap with past winners (+2.0)
//! 5. Buy concentration check (+1.0)
//! 6. Volume acceleration (+1.5)
//! 7. MC velocity (+3.0)

use anyhow::Result;
use log::{debug, info};

/// Early scoring result with signal breakdown
#[derive(Debug, Clone)]
pub struct EarlyScore {
    /// Total score (0.0 - 15.0 max)
    pub total: f64,
    
    /// Signal 1: Creator reputation score
    pub creator_score: f64,
    
    /// Signal 2: Buyer speed score
    pub buyer_speed_score: f64,
    
    /// Signal 3: Liquidity ratio score
    pub liquidity_score: f64,
    
    /// Signal 4: Wallet overlap score
    pub wallet_overlap_score: f64,
    
    /// Signal 5: Buy concentration score
    pub concentration_score: f64,
    
    /// Signal 6: Volume acceleration score
    pub volume_accel_score: f64,
    
    /// Signal 7: MC velocity score
    pub mc_velocity_score: f64,
    
    /// Human-readable breakdown
    pub breakdown: String,
}

impl EarlyScore {
    /// Check if score meets minimum threshold for entry
    pub fn is_high_confidence(&self, min_score: f64) -> bool {
        self.total >= min_score
    }
    
    /// Check if score qualifies for max position size
    pub fn is_ultra_high_confidence(&self, threshold: f64) -> bool {
        self.total >= threshold
    }
    
    /// Get confidence level as percentage (0-100)
    pub fn confidence_pct(&self) -> u8 {
        let pct = (self.total / 15.0 * 100.0).min(100.0);
        pct as u8
    }
    
    /// Generate human-readable breakdown string
    fn generate_breakdown(&self) -> String {
        let mut parts = Vec::new();
        
        if self.creator_score > 0.0 {
            parts.push(format!("creator:{:.1}", self.creator_score));
        }
        if self.buyer_speed_score > 0.0 {
            parts.push(format!("buyers:{:.1}", self.buyer_speed_score));
        }
        if self.liquidity_score > 0.0 {
            parts.push(format!("liq:{:.1}", self.liquidity_score));
        }
        if self.wallet_overlap_score > 0.0 {
            parts.push(format!("overlap:{:.1}", self.wallet_overlap_score));
        }
        if self.concentration_score > 0.0 {
            parts.push(format!("conc:{:.1}", self.concentration_score));
        }
        if self.volume_accel_score > 0.0 {
            parts.push(format!("vol:{:.1}", self.volume_accel_score));
        }
        if self.mc_velocity_score > 0.0 {
            parts.push(format!("mcvel:{:.1}", self.mc_velocity_score));
        }
        
        if parts.is_empty() {
            "no signals".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Early scoring engine configuration
#[derive(Debug, Clone)]
pub struct EarlyScorerConfig {
    // Signal 2: Buyer speed
    pub min_buyers_for_bonus: u32,           // Default: 10
    pub buyer_speed_window_secs: u64,        // Default: 10s
    
    // Signal 3: Liquidity ratio
    pub optimal_liq_ratio: f64,              // Default: 4.0
    
    // Signal 5: Buy concentration
    pub max_top3_share: f64,                 // Default: 0.7 (70%)
    
    // Signal 6: Volume acceleration
    pub vol_accel_window_secs: u64,          // Default: 30s
    pub vol_accel_threshold: f64,            // Default: 2.0x
    
    // Signal 7: MC velocity
    pub high_mc_velocity_sol_per_min: f64,   // Default: 1000.0
}

impl Default for EarlyScorerConfig {
    fn default() -> Self {
        Self {
            min_buyers_for_bonus: 10,
            buyer_speed_window_secs: 10,
            optimal_liq_ratio: 4.0,
            max_top3_share: 0.7,
            vol_accel_window_secs: 30,
            vol_accel_threshold: 2.0,
            high_mc_velocity_sol_per_min: 1000.0,
        }
    }
}

/// Early scoring engine that predicts explosive tokens
pub struct EarlyScorer {
    config: EarlyScorerConfig,
}

impl EarlyScorer {
    /// Create new early scorer with default config
    pub fn new() -> Self {
        Self {
            config: EarlyScorerConfig::default(),
        }
    }
    
    /// Create with custom config
    pub fn with_config(config: EarlyScorerConfig) -> Self {
        Self { config }
    }
    
    /// Calculate early score for a new launch
    /// 
    /// # Arguments
    /// * `creator_is_profitable` - Creator has profitable history
    /// * `unique_buyers` - Unique buyers in time window
    /// * `time_window_secs` - Time since launch (for buyer speed)
    /// * `market_cap` - Current market cap (SOL)
    /// * `liquidity` - Current liquidity (SOL)
    /// * `top3_share` - Top 3 wallets' share of supply (0.0-1.0)
    /// * `volume_30s_ago` - Volume 30s ago (SOL)
    /// * `volume_now` - Current volume (SOL)
    /// * `mc_30s_ago` - Market cap 30s ago (SOL)
    /// * `wallet_overlap_count` - Count of buyers that are in profitable wallet DB
    /// * `total_buyers` - Total unique buyers for overlap ratio
    /// 
    /// # Returns
    /// EarlyScore with signal breakdown
    pub fn calculate_score(
        &self,
        creator_is_profitable: bool,
        unique_buyers: u32,
        time_window_secs: u64,
        market_cap: f64,
        liquidity: f64,
        top3_share: f64,
        volume_30s_ago: f64,
        volume_now: f64,
        mc_30s_ago: f64,
        wallet_overlap_count: u32,
        total_buyers: u32,
    ) -> EarlyScore {
        let mut score = EarlyScore {
            total: 0.0,
            creator_score: 0.0,
            buyer_speed_score: 0.0,
            liquidity_score: 0.0,
            wallet_overlap_score: 0.0,
            concentration_score: 0.0,
            volume_accel_score: 0.0,
            mc_velocity_score: 0.0,
            breakdown: String::new(),
        };
        
        // ========================================
        // SIGNAL 1: Creator Wallet Reputation
        // ========================================
        // If creator has profitable history, add +2.0
        if creator_is_profitable {
            score.creator_score = 2.0;
            score.total += 2.0;
            debug!("✅ Signal 1: Creator is profitable (+2.0)");
        }
        
        // ========================================
        // SIGNAL 2: Speed of First 10 Buyers
        // ========================================
        // If ≥10 unique buyers within time window, add +2.0
        if unique_buyers >= self.config.min_buyers_for_bonus 
            && time_window_secs <= self.config.buyer_speed_window_secs {
            score.buyer_speed_score = 2.0;
            score.total += 2.0;
            debug!("✅ Signal 2: Fast buyer accumulation - {} buyers in {}s (+2.0)", 
                   unique_buyers, time_window_secs);
        } else if unique_buyers >= (self.config.min_buyers_for_bonus / 2) {
            // Partial credit for 5-9 buyers
            score.buyer_speed_score = 1.0;
            score.total += 1.0;
            debug!("⚡ Signal 2: Moderate buyers - {} buyers (+1.0)", unique_buyers);
        }
        
        // ========================================
        // SIGNAL 3: Liquidity vs MC Ratio
        // ========================================
        // Lower ratio = sharper bonding curve = better
        if liquidity > 0.0 && market_cap > 0.0 {
            let liq_ratio = market_cap / liquidity;
            if liq_ratio < self.config.optimal_liq_ratio {
                score.liquidity_score = 1.5;
                score.total += 1.5;
                debug!("✅ Signal 3: Optimal liquidity ratio {:.2} (+1.5)", liq_ratio);
            } else if liq_ratio < self.config.optimal_liq_ratio * 1.5 {
                // Partial credit
                score.liquidity_score = 0.75;
                score.total += 0.75;
                debug!("⚡ Signal 3: Acceptable liq ratio {:.2} (+0.75)", liq_ratio);
            }
        }
        
        // ========================================
        // SIGNAL 4: Wallet Overlap with Winners
        // ========================================
        // If buyers overlap with profitable wallets from past 10-100X tokens
        if total_buyers > 0 {
            let overlap_pct = wallet_overlap_count as f64 / total_buyers as f64;
            if overlap_pct >= 0.3 {
                // 30%+ overlap = strong insider signal
                score.wallet_overlap_score = 2.0;
                score.total += 2.0;
                debug!("✅ Signal 4: High wallet overlap - {:.1}% ({}/{}) (+2.0)", 
                       overlap_pct * 100.0, wallet_overlap_count, total_buyers);
            } else if overlap_pct >= 0.15 {
                // 15-30% overlap = moderate signal
                score.wallet_overlap_score = 1.0;
                score.total += 1.0;
                debug!("⚡ Signal 4: Moderate wallet overlap - {:.1}% (+1.0)", 
                       overlap_pct * 100.0);
            }
        }
        
        // ========================================
        // SIGNAL 5: Buy Concentration (Rug Check)
        // ========================================
        // If top-3 wallets hold <70% = decentralized, add +1.0
        if top3_share < self.config.max_top3_share && top3_share > 0.0 {
            score.concentration_score = 1.0;
            score.total += 1.0;
            debug!("✅ Signal 5: Decentralized - top3 share {:.1}% (+1.0)", 
                   top3_share * 100.0);
        } else if top3_share >= self.config.max_top3_share {
            // Rug risk - log warning
            debug!("⚠️  Signal 5: HIGH RUG RISK - top3 share {:.1}% (no bonus)", 
                   top3_share * 100.0);
        }
        
        // ========================================
        // SIGNAL 6: Volume Acceleration
        // ========================================
        // If volume doubled in 30s window, add +1.5
        if volume_30s_ago > 0.0 && volume_now >= volume_30s_ago * self.config.vol_accel_threshold {
            score.volume_accel_score = 1.5;
            score.total += 1.5;
            debug!("✅ Signal 6: Volume surge - {:.2}x in 30s (+1.5)", 
                   volume_now / volume_30s_ago);
        } else if volume_now > volume_30s_ago * 1.3 {
            // Partial credit for 1.3x+
            score.volume_accel_score = 0.75;
            score.total += 0.75;
            debug!("⚡ Signal 6: Volume increase - {:.2}x (+0.75)", 
                   volume_now / volume_30s_ago);
        }
        
        // ========================================
        // SIGNAL 7: MC Velocity (Most Important)
        // ========================================
        // MC growth rate > 1000 SOL/min = explosive, add +3.0
        if mc_30s_ago > 0.0 && market_cap > mc_30s_ago {
            let mc_growth_sol = market_cap - mc_30s_ago;
            let mc_velocity_per_min = (mc_growth_sol / 30.0) * 60.0;  // SOL per minute
            
            if mc_velocity_per_min >= self.config.high_mc_velocity_sol_per_min {
                score.mc_velocity_score = 3.0;
                score.total += 3.0;
                debug!("✅ Signal 7: EXPLOSIVE MC velocity - {:.0} SOL/min (+3.0)", 
                       mc_velocity_per_min);
            } else if mc_velocity_per_min >= self.config.high_mc_velocity_sol_per_min * 0.5 {
                // Partial credit for 500+ SOL/min
                score.mc_velocity_score = 1.5;
                score.total += 1.5;
                debug!("⚡ Signal 7: Strong MC velocity - {:.0} SOL/min (+1.5)", 
                       mc_velocity_per_min);
            } else if mc_velocity_per_min > 0.0 {
                debug!("ℹ️  Signal 7: Moderate MC velocity - {:.0} SOL/min (no bonus)", 
                       mc_velocity_per_min);
            }
        }
        
        // Generate breakdown string
        score.breakdown = score.generate_breakdown();
        
        // Log final score with thresholds
        if score.total >= 8.0 {
            info!("🔥 ULTRA HIGH CONFIDENCE: score={:.2} | {} | conf={}%", 
                  score.total, score.breakdown, score.confidence_pct());
        } else if score.total >= 6.0 {
            info!("✅ HIGH CONFIDENCE: score={:.2} | {} | conf={}%", 
                  score.total, score.breakdown, score.confidence_pct());
        } else if score.total >= 4.0 {
            debug!("⚡ MODERATE: score={:.2} | {} | conf={}%", 
                   score.total, score.breakdown, score.confidence_pct());
        } else {
            debug!("❌ LOW SCORE: score={:.2} | {} | conf={}%", 
                   score.total, score.breakdown, score.confidence_pct());
        }
        
        score
    }
    
    /// Quick check if token meets minimum scoring threshold
    pub fn quick_check(
        &self,
        creator_is_profitable: bool,
        unique_buyers: u32,
        mc_velocity_sol_per_min: f64,
    ) -> bool {
        // Fast path: Check critical signals only
        let mut quick_score = 0.0;
        
        if creator_is_profitable {
            quick_score += 2.0;
        }
        if unique_buyers >= self.config.min_buyers_for_bonus {
            quick_score += 2.0;
        }
        if mc_velocity_sol_per_min >= self.config.high_mc_velocity_sol_per_min {
            quick_score += 3.0;
        }
        
        quick_score >= 6.0  // Minimum threshold
    }
}

impl Default for EarlyScorer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_perfect_score() {
        let scorer = EarlyScorer::new();
        
        let score = scorer.calculate_score(
            true,      // creator profitable
            15,        // 15 buyers
            8,         // in 8 seconds
            10000.0,   // 10K SOL MC
            3000.0,    // 3K SOL liquidity (ratio 3.3)
            0.4,       // top3 40%
            10.0,      // vol 30s ago
            25.0,      // vol now (2.5x)
            5000.0,    // MC 30s ago
            5,         // 5 wallet overlaps
            15,        // out of 15 buyers (33%)
        );
        
        // Should score high on all signals
        assert!(score.total >= 10.0, "Expected high score, got {}", score.total);
        assert!(score.is_high_confidence(6.0));
        assert!(score.is_ultra_high_confidence(8.0));
    }
    
    #[test]
    fn test_rug_detection() {
        let scorer = EarlyScorer::new();
        
        let score = scorer.calculate_score(
            false,     // creator not profitable
            5,         // only 5 buyers
            15,        // slow
            5000.0,    // MC
            1000.0,    // liquidity
            0.85,      // top3 85% (RUG RISK)
            10.0,      // vol
            12.0,      // vol now
            4000.0,    // MC 30s ago
            0,         // no overlaps
            5,         // total buyers
        );
        
        // Should score low due to rug risk
        assert!(score.concentration_score == 0.0, "Should detect rug risk");
        assert!(score.total < 6.0, "Should not meet threshold");
    }
    
    #[test]
    fn test_quick_check() {
        let scorer = EarlyScorer::new();
        
        // Should pass quick check
        assert!(scorer.quick_check(true, 12, 1500.0));
        
        // Should fail quick check
        assert!(!scorer.quick_check(false, 3, 200.0));
    }
}
