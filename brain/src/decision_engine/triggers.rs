//! 🎯 Entry Trigger Paths
//!
//! Implements the four entry trigger mechanisms:
//! - Path A: Rank-based (top 2 launches with follow-through)
//! - Path B: Momentum-based (high recent activity)
//! - Path C: Copy-trade (following profitable wallets)
//! - Path D: Late opportunity (mature launches)

use solana_sdk::pubkey::Pubkey;
use anyhow::{Result, bail};
use log::debug;
use crate::feature_cache::{MintFeatures, WalletFeatures};
use crate::decision_engine::{TradeValidator, ValidatedTrade};
use crate::udp_bus::messages::TradeDecision;

/// Entry trigger type for logging and analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryTrigger {
    RankBased,      // Path A: Top-ranked launch
    Momentum,       // Path B: High recent activity
    CopyTrade,      // Path C: Following wallet
    LateOpportunity, // Path D: Mature launch
}

impl EntryTrigger {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryTrigger::RankBased => "rank",
            EntryTrigger::Momentum => "momentum",
            EntryTrigger::CopyTrade => "copy",
            EntryTrigger::LateOpportunity => "late",
        }
    }
}

/// Configuration for entry triggers (PATH-SPECIFIC for 1M+ MC hunting)
#[derive(Debug, Clone)]
pub struct TriggerConfig {
    // ========================================
    // PATH A: RANK-BASED (Hot New Launches)
    // ========================================
    // Objective: Catch new coins before spike (first 10 buyers)
    pub max_rank_for_instant: u8,          // Default: 5 (1M+ MC mode)
    pub min_follow_through_rank: u8,       // Default: 25 (1M+ MC mode)
    pub min_decision_conf_rank: u8,        // Path-specific confidence: 55
    pub rank_position_size_sol: f64,       // Default: 50.0 SOL (1M+ MC mode)
    pub rank_max_hold_secs: u64,           // Default: 30s
    
    // ========================================
    // PATH B: MOMENTUM (Surge Rider)
    // ========================================
    // Objective: Enter during confirmed surges (100K→1M MC)
    pub min_buyers_2s: u32,                // Default: 3 (1M+ MC mode)
    pub min_vol_5s_sol: f64,               // Default: 4.0 SOL (1M+ MC mode)
    pub min_follow_through_momentum: u8,   // Default: 35 (1M+ MC mode)
    pub min_decision_conf_momentum: u8,    // Path-specific confidence: 65
    pub momentum_position_size_sol: f64,   // Default: 75.0 SOL (1M+ MC mode)
    pub momentum_max_hold_secs: u64,       // Default: 120s (2 min)
    
    // ========================================
    // PATH C: COPY-TRADE (Whale Following)
    // ========================================
    // Objective: Fast scalp following proven wallets
    pub min_copy_tier: u8,                 // Default: 1 (Tier C)
    pub min_copy_confidence: u8,           // Default: 65 (1M+ MC mode)
    pub min_decision_conf_copy: u8,        // Path-specific confidence: 70
    pub min_copy_size_sol: f64,            // Default: 0.25 SOL
    pub copy_multiplier: f64,              // Default: 1.2x wallet's size
    pub copy_position_size_base: f64,      // Default: 25.0 SOL (1M+ MC mode)
    pub copy_max_hold_secs: u64,           // Default: 15s
    
    // ========================================
    // PATH D: LATE-OPPORTUNITY (DISABLED)
    // ========================================
    // Objective: Second waves (not for 1M+ MC hunting)
    pub enable_late_opportunity: bool,     // Default: false (disabled for 1M+ MC)
    pub min_launch_age_seconds: u64,       // Default: 1200 (20 min)
    pub min_vol_60s_late: f64,             // Default: 35.0 SOL
    pub min_buyers_60s_late: u32,          // Default: 40
    pub min_follow_through_late: u8,       // Default: 70
    pub late_position_size_sol: f64,       // Default: 5.0 SOL
    
    // ========================================
    // GENERAL & SAFETY
    // ========================================
    pub default_slippage_bps: u16,         // Default: 150 (1.5%)
    pub max_position_size_sol: f64,        // Default: 150.0 SOL (hard cap)
    pub min_position_size_sol: f64,        // Default: 25.0 SOL (testing minimum)
    
    // Early scoring integration
    pub min_early_score: f64,              // Default: 6.0 (7-signal system)
    pub high_confidence_score: f64,        // Default: 8.0 (max position size)
}

impl Default for TriggerConfig {
    fn default() -> Self {
        // ==========================================
        // 🎯 1M+ MC HUNTING MODE
        // ==========================================
        // Optimized for catching tokens before they hit 1M market cap
        // Path-specific thresholds for different entry strategies
        // Position sizes scaled for $100 entries
        // ==========================================
        
        Self {
            // ========================================
            // PATH A: RANK-BASED (Hot New Launches)
            // ========================================
            max_rank_for_instant: 5,               // Expand from 2 → top 5 launches
            min_follow_through_rank: 25,           // Loosen for faster entry
            min_decision_conf_rank: 55,            // Lower than global for speed
            rank_position_size_sol: 50.0,          // Scale up for $100 entries
            rank_max_hold_secs: 30,                // Quick exits
            
            // ========================================
            // PATH B: MOMENTUM (Surge Rider)
            // ========================================
            min_buyers_2s: 3,                      // Raise from 2 for quality
            min_vol_5s_sol: 4.0,                   // Raise from 2.0 for genuine interest
            min_follow_through_momentum: 35,       // Balance speed vs quality
            min_decision_conf_momentum: 65,        // Moderate confidence
            momentum_position_size_sol: 75.0,      // Scale up for confirmed waves
            momentum_max_hold_secs: 120,           // 2 min max (velocity-based exit)
            
            // ========================================
            // PATH C: COPY-TRADE (Whale Following)
            // ========================================
            min_copy_tier: 1,                      // Tier C (80 confidence)
            min_copy_confidence: 65,               // Raise from 50 for quality
            min_decision_conf_copy: 70,            // High quality wallets only
            min_copy_size_sol: 0.25,               // Minimum whale position
            copy_multiplier: 1.2,                  // Our size = 1.2x whale
            copy_position_size_base: 25.0,         // Base size for copies
            copy_max_hold_secs: 15,                // Fast in/out
            
            // ========================================
            // PATH D: LATE-OPPORTUNITY (DISABLED)
            // ========================================
            enable_late_opportunity: false,        // 🚫 DISABLED for 1M+ MC hunting
            min_launch_age_seconds: 1200,          // 20 min (not used when disabled)
            min_vol_60s_late: 35.0,                // Production value
            min_buyers_60s_late: 40,               // Production value
            min_follow_through_late: 70,           // Production value
            late_position_size_sol: 5.0,           // Production value
            
            // ========================================
            // GENERAL & SAFETY
            // ========================================
            default_slippage_bps: 150,             // 1.5% slippage
            max_position_size_sol: 150.0,          // Hard cap per trade
            min_position_size_sol: 25.0,           // Testing minimum
            
            // Early scoring (7-signal system integration)
            min_early_score: 6.0,                  // Minimum score to trigger
            high_confidence_score: 8.0,            // Max position size threshold
        }
    }
}

/// Entry trigger engine that evaluates conditions and creates trade decisions
pub struct TriggerEngine {
    config: TriggerConfig,
    validator: TradeValidator,
}

impl TriggerEngine {
    /// Create new trigger engine with default config
    pub fn new() -> Self {
        Self {
            config: TriggerConfig::default(),
            validator: TradeValidator::new(),
        }
    }
    
    /// Create with custom config
    pub fn with_config(config: TriggerConfig) -> Self {
        Self {
            config,
            validator: TradeValidator::new(),
        }
    }
    
    /// Calculate position size based on early_score and entry path
    /// 
    /// Higher scores → larger positions (up to max cap)
    /// Each path has different base sizing strategy
    /// 
    /// Returns: Position size in SOL
    pub fn calculate_position_size(&self, early_score: f64, path: EntryTrigger) -> f64 {
        let size = match path {
            EntryTrigger::RankBased => {
                // Rank-based: Scale by score
                if early_score >= 9.0 {
                    self.config.rank_position_size_sol * 2.0  // 100 SOL
                } else if early_score >= self.config.high_confidence_score {
                    self.config.rank_position_size_sol * 1.5  // 75 SOL
                } else if early_score >= 7.0 {
                    self.config.rank_position_size_sol        // 50 SOL
                } else {
                    self.config.min_position_size_sol         // 25 SOL (testing)
                }
            },
            EntryTrigger::Momentum => {
                // Momentum: Scale by score (larger base)
                if early_score >= 9.0 {
                    self.config.momentum_position_size_sol * 1.33  // 100 SOL
                } else if early_score >= self.config.high_confidence_score {
                    self.config.momentum_position_size_sol         // 75 SOL
                } else if early_score >= 7.0 {
                    self.config.momentum_position_size_sol * 0.67  // 50 SOL
                } else {
                    self.config.min_position_size_sol              // 25 SOL
                }
            },
            EntryTrigger::CopyTrade => {
                // Copy-trade: Conservative sizing (quick scalps)
                if early_score >= self.config.high_confidence_score {
                    self.config.copy_position_size_base * 2.0  // 50 SOL
                } else {
                    self.config.copy_position_size_base        // 25 SOL
                }
            },
            EntryTrigger::LateOpportunity => {
                // Late: Not used for 1M+ MC hunting
                self.config.late_position_size_sol
            }
        };
        
        // Apply safety caps
        size.max(self.config.min_position_size_sol)
            .min(self.config.max_position_size_sol)
    }
    
    /// Get path-specific confidence threshold
    pub fn get_min_decision_conf(&self, path: EntryTrigger) -> u8 {
        match path {
            EntryTrigger::RankBased => self.config.min_decision_conf_rank,
            EntryTrigger::Momentum => self.config.min_decision_conf_momentum,
            EntryTrigger::CopyTrade => self.config.min_decision_conf_copy,
            EntryTrigger::LateOpportunity => 75,  // Not used
        }
    }
    
    /// Get path-specific max hold time
    pub fn get_max_hold_time(&self, path: EntryTrigger) -> u64 {
        match path {
            EntryTrigger::RankBased => self.config.rank_max_hold_secs,
            EntryTrigger::Momentum => self.config.momentum_max_hold_secs,
            EntryTrigger::CopyTrade => self.config.copy_max_hold_secs,
            EntryTrigger::LateOpportunity => 300,  // Not used
        }
    }
    
    /// Path A: Rank-based trigger
    /// 
    /// Fires for top-ranked launches (rank ≤ 2) with sufficient follow-through.
    /// No pool size threshold required - these are the hottest launches.
    /// 
    /// Conditions:
    /// - rank ≤ max_rank_for_instant
    /// - follow_through_score ≥ min_follow_through_rank
    /// 
    /// Returns: Validated trade decision ready for execution
    pub fn try_rank_based(
        &self,
        rank: u8,
        mint: Pubkey,
        mint_features: &MintFeatures,
        creator: Option<Pubkey>,
    ) -> Result<ValidatedTrade> {
        // Check rank threshold
        if rank > self.config.max_rank_for_instant {
            bail!("Rank {} exceeds threshold {}", rank, self.config.max_rank_for_instant);
        }
        
        // Check follow-through score
        if mint_features.follow_through_score < self.config.min_follow_through_rank {
            bail!(
                "Follow-through score {} below threshold {}",
                mint_features.follow_through_score,
                self.config.min_follow_through_rank
            );
        }
        
        // Validate trade
        let validated = self.validator.validate(
            mint,
            mint_features,
            self.config.rank_position_size_sol,
            self.config.default_slippage_bps,
            mint_features.follow_through_score,
            creator,
        )?;
        
        debug!(
            "✅ Rank-based trigger fired: rank={}, mint={}..., score={}, size=${:.2}",
            rank,
            &mint.to_string()[..8],
            mint_features.follow_through_score,
            validated.size_usd
        );
        
        Ok(validated)
    }
    
    /// Convert validated trade to TradeDecision packet
    pub fn to_trade_decision(
        &self,
        validated: &ValidatedTrade,
        trigger: EntryTrigger,
    ) -> TradeDecision {
        // Convert EntryTrigger to entry_type u8
        let entry_type = match trigger {
            EntryTrigger::RankBased => 0,
            EntryTrigger::Momentum => 1,
            EntryTrigger::CopyTrade => 2,
            EntryTrigger::LateOpportunity => 3,
        };
        
        // Use new_buy() constructor which automatically calculates checksum
        TradeDecision::new_buy(
            validated.mint.to_bytes(),
            validated.size_lamports,
            validated.slippage_bps,
            validated.follow_through_score,
            entry_type,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature_cache::MintFeatures;
    
    fn mock_mint_features(score: u8, vol_60s: f64, buyers_60s: u32) -> MintFeatures {
        MintFeatures {
            age_since_launch: 60,
            current_price: 0.001,
            vol_60s_sol: vol_60s,
            buyers_60s,
            buys_sells_ratio: 2.5,
            curve_depth_proxy: 1_000_000,
            follow_through_score: score,
            buyers_2s: 10,
            vol_5s_sol: 15.0,
            last_update: 0,
            volatility_60s: 0.10,
        }
    }
    
    #[test]
    fn test_rank_based_trigger_success() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_mint_features(75, 25.0, 20);
        
        let result = engine.try_rank_based(1, mint, &features, None);
        assert!(result.is_ok(), "Rank-based trigger should succeed");
        
        let validated = result.unwrap();
        assert_eq!(validated.follow_through_score, 75);
        assert_eq!(validated.size_usd, 10.0);
    }
    
    #[test]
    fn test_rank_based_trigger_rank_too_high() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_mint_features(75, 25.0, 20);
        
        let result = engine.try_rank_based(5, mint, &features, None);
        assert!(result.is_err(), "Should reject rank > 2");
        assert!(result.unwrap_err().to_string().contains("Rank"));
    }
    
    #[test]
    fn test_rank_based_trigger_score_too_low() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_mint_features(45, 25.0, 20);
        
        let result = engine.try_rank_based(1, mint, &features, None);
        assert!(result.is_err(), "Should reject low follow-through score");
    }
    
    #[test]
    fn test_trigger_config_defaults() {
        let config = TriggerConfig::default();
        assert_eq!(config.max_rank_for_instant, 2);
        assert_eq!(config.min_follow_through_rank, 60);
        assert_eq!(config.rank_position_size_sol, 10.0);
    }
    
    #[test]
    fn test_entry_trigger_strings() {
        assert_eq!(EntryTrigger::RankBased.as_str(), "rank");
        assert_eq!(EntryTrigger::Momentum.as_str(), "momentum");
        assert_eq!(EntryTrigger::CopyTrade.as_str(), "copy");
        assert_eq!(EntryTrigger::LateOpportunity.as_str(), "late");
    }
    
    #[test]
    fn test_to_trade_decision() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_mint_features(80, 30.0, 25);
        
        let validated = engine.try_rank_based(1, mint, &features, None).unwrap();
        let decision = engine.to_trade_decision(&validated, EntryTrigger::RankBased);
        
        assert_eq!(decision.msg_type, 1);
        assert_eq!(decision.side, 0);
        assert_eq!(decision.mint, mint.to_bytes());
        assert_eq!(decision.confidence, 80);
        assert_eq!(decision.slippage_bps, 150);
    }
}

impl TriggerEngine {
    /// Path B: Momentum-based trigger
    /// 
    /// Fires when recent activity shows strong momentum.
    /// Requires high recent buyer count and volume.
    /// 
    /// Conditions:
    /// - buyers_2s ≥ min_buyers_2s
    /// - vol_5s_sol ≥ min_vol_5s_sol
    /// - follow_through_score ≥ min_follow_through_momentum
    /// 
    /// Returns: Validated trade decision
    pub fn try_momentum(
        &self,
        mint: Pubkey,
        mint_features: &MintFeatures,
        creator: Option<Pubkey>,
    ) -> Result<ValidatedTrade> {
        // Check recent buyer count
        if mint_features.buyers_2s < self.config.min_buyers_2s {
            bail!(
                "Recent buyers {} below threshold {}",
                mint_features.buyers_2s,
                self.config.min_buyers_2s
            );
        }
        
        // Check recent volume
        if mint_features.vol_5s_sol < self.config.min_vol_5s_sol {
            bail!(
                "Recent volume {:.2} SOL below threshold {:.2} SOL",
                mint_features.vol_5s_sol,
                self.config.min_vol_5s_sol
            );
        }
        
        // Check follow-through score
        if mint_features.follow_through_score < self.config.min_follow_through_momentum {
            bail!(
                "Follow-through score {} below threshold {}",
                mint_features.follow_through_score,
                self.config.min_follow_through_momentum
            );
        }
        
        // Validate trade
        let validated = self.validator.validate(
            mint,
            mint_features,
            self.config.momentum_position_size_sol,
            self.config.default_slippage_bps,
            mint_features.follow_through_score,
            creator,
        )?;
        
        debug!(
            "⚡ Momentum trigger fired: buyers_2s={}, vol_5s={:.1} SOL, score={}, size=${:.2}",
            mint_features.buyers_2s,
            mint_features.vol_5s_sol,
            mint_features.follow_through_score,
            validated.size_usd
        );
        
        Ok(validated)
    }
    
    /// Path C: Copy-trade trigger
    /// 
    /// Fires when a high-tier wallet makes a trade worth copying.
    /// 
    /// Conditions:
    /// - wallet.tier ≥ min_copy_tier (default: Tier C)
    /// - wallet.confidence ≥ min_copy_confidence
    /// - trade_size_sol ≥ min_copy_size_sol
    /// 
    /// Position size: wallet's size × copy_multiplier (default 1.2x)
    /// 
    /// Returns: Validated trade decision
    pub fn try_copy_trade(
        &self,
        mint: Pubkey,
        mint_features: &MintFeatures,
        wallet_features: &WalletFeatures,
        wallet_trade_size_sol: f64,
        creator: Option<Pubkey>,
    ) -> Result<ValidatedTrade> {
        // Check wallet tier
        if (wallet_features.tier as u8) < self.config.min_copy_tier {
            bail!(
                "Wallet tier {:?} below threshold",
                wallet_features.tier
            );
        }
        
        // Check wallet confidence
        if wallet_features.confidence < self.config.min_copy_confidence {
            bail!(
                "Wallet confidence {} below threshold {}",
                wallet_features.confidence,
                self.config.min_copy_confidence
            );
        }
        
        // Check trade size
        if wallet_trade_size_sol < self.config.min_copy_size_sol {
            bail!(
                "Trade size {:.2} SOL below threshold {:.2} SOL",
                wallet_trade_size_sol,
                self.config.min_copy_size_sol
            );
        }
        
        // Calculate position size based on wallet's trade
        let position_size_sol = wallet_trade_size_sol * self.config.copy_multiplier;
        let position_size_usd = position_size_sol; // Assuming 1 SOL ≈ $1 for now
        
        // Use wallet confidence as follow-through score
        let follow_through_score = wallet_features.confidence;
        
        // Validate trade
        let validated = self.validator.validate(
            mint,
            mint_features,
            position_size_usd,
            self.config.default_slippage_bps,
            follow_through_score,
            creator,
        )?;
        
        debug!(
            "🎭 Copy-trade trigger fired: tier={:?}, conf={}, wallet_size={:.2} SOL, our_size=${:.2}",
            wallet_features.tier,
            wallet_features.confidence,
            wallet_trade_size_sol,
            validated.size_usd
        );
        
        Ok(validated)
    }
    
    /// Path D: Late opportunity trigger
    /// 
    /// Fires for mature launches that show sustained activity.
    /// Lower priority - should be aborted if a hot launch fires.
    /// 
    /// Conditions:
    /// - age_since_launch > min_launch_age_seconds (default: 20 min)
    /// - vol_60s_sol ≥ min_vol_60s_late
    /// - buyers_60s ≥ min_buyers_60s_late
    /// - follow_through_score ≥ min_follow_through_late
    /// 
    /// Returns: Validated trade decision
    pub fn try_late_opportunity(
        &self,
        mint: Pubkey,
        mint_features: &MintFeatures,
        creator: Option<Pubkey>,
    ) -> Result<ValidatedTrade> {
        // Check launch age
        if mint_features.age_since_launch <= self.config.min_launch_age_seconds {
            bail!(
                "Launch age {}s below threshold {}s",
                mint_features.age_since_launch,
                self.config.min_launch_age_seconds
            );
        }
        
        // Check volume
        if mint_features.vol_60s_sol < self.config.min_vol_60s_late {
            bail!(
                "Volume {:.2} SOL below threshold {:.2} SOL",
                mint_features.vol_60s_sol,
                self.config.min_vol_60s_late
            );
        }
        
        // Check buyer count
        if mint_features.buyers_60s < self.config.min_buyers_60s_late {
            bail!(
                "Buyers {} below threshold {}",
                mint_features.buyers_60s,
                self.config.min_buyers_60s_late
            );
        }
        
        // Check follow-through score
        if mint_features.follow_through_score < self.config.min_follow_through_late {
            bail!(
                "Follow-through score {} below threshold {}",
                mint_features.follow_through_score,
                self.config.min_follow_through_late
            );
        }
        
        // Validate trade
        let validated = self.validator.validate(
            mint,
            mint_features,
            self.config.late_position_size_sol,
            self.config.default_slippage_bps,
            mint_features.follow_through_score,
            creator,
        )?;
        
        debug!(
            "🕐 Late opportunity trigger fired: age={}s, vol={:.1} SOL, buyers={}, score={}, size=${:.2}",
            mint_features.age_since_launch,
            mint_features.vol_60s_sol,
            mint_features.buyers_60s,
            mint_features.follow_through_score,
            validated.size_usd
        );
        
        Ok(validated)
    }
}

#[cfg(test)]
mod momentum_tests {
    use super::*;
    use crate::feature_cache::MintFeatures;
    
    fn mock_features(buyers_2s: u32, vol_5s: f64, score: u8) -> MintFeatures {
        MintFeatures {
            age_since_launch: 60,
            current_price: 0.001,
            vol_60s_sol: 25.0,
            buyers_60s: 20,
            buys_sells_ratio: 2.5,
            curve_depth_proxy: 1_000_000,
            follow_through_score: score,
            buyers_2s,
            vol_5s_sol: vol_5s,
            last_update: 0,
            volatility_60s: 0.08,
        }
    }
    
    #[test]
    fn test_momentum_trigger_success() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_features(8, 15.0, 70);
        
        let result = engine.try_momentum(mint, &features, None);
        assert!(result.is_ok(), "Momentum trigger should succeed");
        
        let validated = result.unwrap();
        assert_eq!(validated.size_usd, 8.0);
    }
    
    #[test]
    fn test_momentum_trigger_low_buyers() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_features(3, 15.0, 70);
        
        let result = engine.try_momentum(mint, &features, None);
        assert!(result.is_err(), "Should reject low buyers");
        assert!(result.unwrap_err().to_string().contains("buyers"));
    }
    
    #[test]
    fn test_momentum_trigger_low_volume() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_features(8, 5.0, 70);
        
        let result = engine.try_momentum(mint, &features, None);
        assert!(result.is_err(), "Should reject low volume");
        assert!(result.unwrap_err().to_string().contains("volume"));
    }
}

#[cfg(test)]
mod copy_trade_tests {
    use super::*;
    use crate::feature_cache::{MintFeatures, WalletFeatures, WalletTier};
    
    fn mock_wallet(tier: WalletTier, confidence: u8) -> WalletFeatures {
        WalletFeatures {
            win_rate_7d: 0.65,
            realized_pnl_7d: 100.0,
            trade_count: 50,
            avg_size: 5.0,
            tier,
            confidence,
            last_trade: None,
            last_update: 0,
            bootstrap_score: 80,
        }
    }
    
    fn mock_mint() -> MintFeatures {
        MintFeatures {
            age_since_launch: 60,
            current_price: 0.001,
            vol_60s_sol: 30.0,
            buyers_60s: 25,
            buys_sells_ratio: 2.5,
            curve_depth_proxy: 1_000_000,
            follow_through_score: 75,
            buyers_2s: 10,
            vol_5s_sol: 15.0,
            last_update: 0,
            volatility_60s: 0.06,
        }
    }
    
    #[test]
    fn test_copy_trade_success() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let mint_features = mock_mint();
        let wallet = mock_wallet(WalletTier::B, 87);
        
        let result = engine.try_copy_trade(mint, &mint_features, &wallet, 2.0, None);
        assert!(result.is_ok(), "Copy-trade should succeed for Tier B wallet");
        
        let validated = result.unwrap();
        // 2.0 SOL × 1.2 multiplier = 2.4 SOL
        assert_eq!(validated.size_usd, 2.4);
    }
    
    #[test]
    fn test_copy_trade_low_tier() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let mint_features = mock_mint();
        let wallet = mock_wallet(WalletTier::Discovery, 50);
        
        let result = engine.try_copy_trade(mint, &mint_features, &wallet, 2.0, None);
        assert!(result.is_err(), "Should reject Discovery tier");
    }
    
    #[test]
    fn test_copy_trade_small_size() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let mint_features = mock_mint();
        let wallet = mock_wallet(WalletTier::A, 93);
        
        let result = engine.try_copy_trade(mint, &mint_features, &wallet, 0.1, None);
        assert!(result.is_err(), "Should reject trades < 0.25 SOL");
    }
}

#[cfg(test)]
mod late_opportunity_tests {
    use super::*;
    use crate::feature_cache::MintFeatures;
    
    fn mock_late_launch(age: u64, vol: f64, buyers: u32) -> MintFeatures {
        MintFeatures {
            age_since_launch: age,
            current_price: 0.001,
            vol_60s_sol: vol,
            buyers_60s: buyers,
            buys_sells_ratio: 2.5,
            curve_depth_proxy: 1_000_000,
            follow_through_score: 75,
            buyers_2s: 5,
            vol_5s_sol: 10.0,
            last_update: 0,
            volatility_60s: 0.20,
        }
    }
    
    #[test]
    fn test_late_opportunity_success() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_late_launch(1500, 50.0, 60);
        
        let result = engine.try_late_opportunity(mint, &features, None);
        assert!(result.is_ok(), "Late opportunity should succeed");
        
        let validated = result.unwrap();
        assert_eq!(validated.size_usd, 5.0);
    }
    
    #[test]
    fn test_late_opportunity_too_young() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_late_launch(600, 50.0, 60);  // Only 10 min
        
        let result = engine.try_late_opportunity(mint, &features, None);
        assert!(result.is_err(), "Should reject launches < 20 min");
        assert!(result.unwrap_err().to_string().contains("age"));
    }
    
    #[test]
    fn test_late_opportunity_low_activity() {
        let engine = TriggerEngine::new();
        let mint = Pubkey::new_unique();
        let features = mock_late_launch(1500, 20.0, 25);
        
        let result = engine.try_late_opportunity(mint, &features, None);
        assert!(result.is_err(), "Should reject low activity");
    }
}
