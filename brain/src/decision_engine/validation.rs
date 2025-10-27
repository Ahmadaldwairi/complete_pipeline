//! ✅ Pre-Trade Validation Logic
//!
//! Validates trade decisions before execution to prevent unprofitable trades.
//! Enforces:
//! - Fee floor: min_tp ≥ max(1.00, fees_est × 2.2)
//! - Impact cap: price_impact ≤ min_tp × 0.45
//! - Follow-through threshold: score ≥ minimum
//! - Rug checks: creator blacklist, suspicious patterns

use crate::feature_cache::MintFeatures;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use log::{debug, warn};
use anyhow::{Result, bail};

/// Validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Fee multiplier for minimum profit target (default: 2.2)
    pub fee_multiplier: f64,
    
    /// Impact cap multiplier relative to min profit (default: 0.45)
    pub impact_cap_multiplier: f64,
    
    /// Minimum follow-through score required (default: 60)
    pub min_follow_through_score: u8,
    
    /// Minimum profit target in USD (default: 1.00)
    pub min_profit_target_usd: f64,
    
    /// Maximum age for "hot" launches in seconds (default: 300 = 5 min)
    pub max_hot_launch_age_secs: u64,
    
    /// Known rug creator addresses (blacklist)
    pub rug_creator_blacklist: HashSet<Pubkey>,
    
    /// Enable/disable rug checks (default: true)
    pub enable_rug_checks: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            fee_multiplier: 2.2,
            impact_cap_multiplier: 0.45,
            min_follow_through_score: 60,
            min_profit_target_usd: 1.0,
            max_hot_launch_age_secs: 300,
            rug_creator_blacklist: HashSet::new(),
            enable_rug_checks: true,
        }
    }
}

/// Estimated fees for a trade
#[derive(Debug, Clone)]
pub struct FeeEstimate {
    /// Jito tip in USD (default: ~$0.10)
    pub jito_tip_usd: f64,
    
    /// Gas fee in USD (default: ~$0.001)
    pub gas_fee_usd: f64,
    
    /// Estimated slippage in USD (depends on position size)
    pub slippage_usd: f64,
    
    /// Total estimated fees
    pub total_usd: f64,
}

impl FeeEstimate {
    /// Create fee estimate for a given position size
    pub fn for_position(position_size_usd: f64, slippage_bps: u16) -> Self {
        let jito_tip_usd = 0.10;
        let gas_fee_usd = 0.001;
        let slippage_usd = position_size_usd * (slippage_bps as f64 / 10000.0);
        
        Self {
            jito_tip_usd,
            gas_fee_usd,
            slippage_usd,
            total_usd: jito_tip_usd + gas_fee_usd + slippage_usd,
        }
    }
    
    /// Entry + Exit fees (round trip)
    pub fn round_trip(position_size_usd: f64, slippage_bps: u16) -> Self {
        let entry = Self::for_position(position_size_usd, slippage_bps);
        Self {
            jito_tip_usd: entry.jito_tip_usd * 2.0,
            gas_fee_usd: entry.gas_fee_usd * 2.0,
            slippage_usd: entry.slippage_usd * 2.0,
            total_usd: entry.total_usd * 2.0,
        }
    }
}

/// Validated trade parameters
#[derive(Debug, Clone)]
pub struct ValidatedTrade {
    /// Token mint address
    pub mint: Pubkey,
    
    /// Position size in lamports
    pub size_lamports: u64,
    
    /// Position size in USD
    pub size_usd: f64,
    
    /// Slippage in basis points
    pub slippage_bps: u16,
    
    /// Follow-through score
    pub follow_through_score: u8,
    
    /// Minimum profit target (USD)
    pub min_profit_target_usd: f64,
    
    /// Estimated total fees (USD)
    pub estimated_fees_usd: f64,
    
    /// Estimated price impact (%)
    pub estimated_impact_pct: f64,
    
    /// Expected value (profit probability × avg profit)
    pub expected_value_usd: f64,
}

/// Validation failure reasons
#[derive(Debug, Clone)]
pub enum ValidationError {
    FeeTooHigh {
        estimated_fees: f64,
        min_profit_target: f64,
    },
    ImpactTooHigh {
        estimated_impact: f64,
        max_allowed_impact: f64,
    },
    FollowThroughTooLow {
        score: u8,
        min_required: u8,
    },
    RugCreatorDetected {
        creator: Pubkey,
    },
    SuspiciousPattern {
        reason: String,
    },
    LaunchTooOld {
        age_seconds: u64,
        max_age: u64,
    },
    InsufficientLiquidity {
        available_liquidity: f64,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::FeeTooHigh { estimated_fees, min_profit_target } => {
                write!(f, "Fees too high: ${:.2} (need ${:.2} min profit)", estimated_fees, min_profit_target)
            }
            ValidationError::ImpactTooHigh { estimated_impact, max_allowed_impact } => {
                write!(f, "Impact too high: ${:.2} (max ${:.2})", estimated_impact, max_allowed_impact)
            }
            ValidationError::FollowThroughTooLow { score, min_required } => {
                write!(f, "Follow-through too low: {} (need {})", score, min_required)
            }
            ValidationError::RugCreatorDetected { creator } => {
                write!(f, "Rug creator detected: {}...", &creator.to_string()[..12])
            }
            ValidationError::SuspiciousPattern { reason } => {
                write!(f, "Suspicious pattern: {}", reason)
            }
            ValidationError::LaunchTooOld { age_seconds, max_age } => {
                write!(f, "Launch too old: {}s (max {}s)", age_seconds, max_age)
            }
            ValidationError::InsufficientLiquidity { available_liquidity } => {
                write!(f, "Insufficient liquidity: ${:.2}", available_liquidity)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Pre-trade validator
pub struct TradeValidator {
    config: ValidationConfig,
}

impl TradeValidator {
    /// Create new validator with default config
    pub fn new() -> Self {
        Self {
            config: ValidationConfig::default(),
        }
    }
    
    /// Create validator with custom config
    pub fn with_config(config: ValidationConfig) -> Self {
        Self { config }
    }
    
    /// Validate a trade decision
    /// 
    /// Performs comprehensive checks:
    /// 1. Fee floor check
    /// 2. Impact cap check
    /// 3. Follow-through score check
    /// 4. Rug checks (if enabled)
    /// 5. Age check (for hot launches)
    pub fn validate(
        &self,
        mint: Pubkey,
        mint_features: &MintFeatures,
        position_size_usd: f64,
        slippage_bps: u16,
        follow_through_score: u8,
        creator: Option<Pubkey>,
    ) -> Result<ValidatedTrade> {
        // 1. Calculate estimated fees
        let fees = FeeEstimate::round_trip(position_size_usd, slippage_bps);
        
        // 2. Calculate minimum profit target
        let min_profit_target = self.calculate_min_profit_target(fees.total_usd);
        
        // 3. Check fee floor
        if fees.total_usd > min_profit_target {
            bail!(ValidationError::FeeTooHigh {
                estimated_fees: fees.total_usd,
                min_profit_target,
            });
        }
        
        // 4. Estimate price impact (returns percentage)
        let estimated_impact_pct = self.estimate_price_impact(
            position_size_usd,
            mint_features.curve_depth_proxy,
            mint_features.vol_60s_sol,
        );
        
        // 5. Check impact cap
        // Impact should not exceed 45% of minimum profit target
        // Example: if min_profit=$1.10 and impact=50%, impact_cost=$0.50
        //          max_allowed = $1.10 * 0.45 = $0.495
        let estimated_impact_usd = (position_size_usd * estimated_impact_pct) / 100.0;
        let max_allowed_impact_usd = min_profit_target * self.config.impact_cap_multiplier;
        
        if estimated_impact_usd > max_allowed_impact_usd {
            bail!(ValidationError::ImpactTooHigh {
                estimated_impact: estimated_impact_usd,
                max_allowed_impact: max_allowed_impact_usd,
            });
        }
        
        // 6. Check follow-through score
        if follow_through_score < self.config.min_follow_through_score {
            bail!(ValidationError::FollowThroughTooLow {
                score: follow_through_score,
                min_required: self.config.min_follow_through_score,
            });
        }
        
        // 7. Rug checks (if enabled)
        if self.config.enable_rug_checks {
            if let Some(creator_addr) = creator {
                if self.config.rug_creator_blacklist.contains(&creator_addr) {
                    bail!(ValidationError::RugCreatorDetected { creator: creator_addr });
                }
            }
            
            // Check for suspicious patterns
            self.check_suspicious_patterns(mint_features)?;
        }
        
        // 8. Age check for hot launches
        if mint_features.age_since_launch > self.config.max_hot_launch_age_secs {
            warn!(
                "⚠️  Token {}... is old ({}s), consider late opportunity instead",
                &mint.to_string()[..12],
                mint_features.age_since_launch
            );
        }
        
        // 9. Calculate expected value
        let success_prob = self.estimate_success_probability(follow_through_score, mint_features);
        let expected_value = success_prob * (min_profit_target * 1.5) - (1.0 - success_prob) * fees.total_usd;
        
        debug!(
            "✅ Validation passed: {} | fees=${:.2}, target=${:.2}, impact={:.2}%, score={}, EV=${:.2}",
            &mint.to_string()[..12],
            fees.total_usd,
            min_profit_target,
            estimated_impact_pct,
            follow_through_score,
            expected_value
        );
        
        Ok(ValidatedTrade {
            mint,
            size_lamports: (position_size_usd * 1_000_000_000.0 / mint_features.current_price) as u64,
            size_usd: position_size_usd,
            slippage_bps,
            follow_through_score,
            min_profit_target_usd: min_profit_target,
            estimated_fees_usd: fees.total_usd,
            estimated_impact_pct,
            expected_value_usd: expected_value,
        })
    }
    
    /// Calculate minimum profit target: max(1.00, fees × multiplier)
    fn calculate_min_profit_target(&self, estimated_fees: f64) -> f64 {
        (estimated_fees * self.config.fee_multiplier)
            .max(self.config.min_profit_target_usd)
    }
    
    /// Estimate price impact based on position size and liquidity
    /// 
    /// Impact = (position_size / liquidity_proxy) × impact_factor
    /// Uses curve depth and recent volume as liquidity proxies
    fn estimate_price_impact(
        &self,
        position_size_usd: f64,
        curve_depth_proxy: u64,
        vol_60s_sol: f64,
    ) -> f64 {
        // Use recent volume as liquidity proxy (higher volume = more liquidity)
        let liquidity_proxy = vol_60s_sol.max(1.0);
        
        // Simple impact model: impact ∝ size / liquidity
        // Scale factor: 10 = 1% impact per $1 per SOL liquidity
        let impact_factor = 10.0;
        let raw_impact = (position_size_usd / liquidity_proxy) * impact_factor;
        
        // Adjust based on curve depth (more depth = less impact)
        let depth_factor = if curve_depth_proxy > 0 {
            let depth_ratio = curve_depth_proxy as f64 / 1_000_000.0;
            // Use sqrt to dampen the effect: less depth = higher factor
            1.0 / depth_ratio.sqrt().max(0.5)  // Min factor of 2.0
        } else {
            2.0  // Higher impact if no depth data
        };
        
        (raw_impact * depth_factor).min(100.0)  // Cap at 100%
    }
    
    /// Check for suspicious patterns that might indicate a rug
    fn check_suspicious_patterns(&self, mint_features: &MintFeatures) -> Result<()> {
        // Pattern 1: Very low buyer count with high volume (bot trading)
        if mint_features.vol_60s_sol > 20.0 && mint_features.buyers_60s < 5 {
            bail!(ValidationError::SuspiciousPattern {
                reason: format!(
                    "High volume ({:.1} SOL) with low buyers ({})",
                    mint_features.vol_60s_sol,
                    mint_features.buyers_60s
                )
            });
        }
        
        // Pattern 2: Extremely high buy/sell ratio (potential wash trading)
        if mint_features.buys_sells_ratio > 10.0 {
            bail!(ValidationError::SuspiciousPattern {
                reason: format!(
                    "Suspicious buy/sell ratio: {:.1}:1",
                    mint_features.buys_sells_ratio
                )
            });
        }
        
        // Pattern 3: Zero or near-zero price (likely broken token)
        if mint_features.current_price < 0.000001 {
            bail!(ValidationError::SuspiciousPattern {
                reason: format!("Price too low: {:.10}", mint_features.current_price)
            });
        }
        
        Ok(())
    }
    
    /// Estimate success probability based on score and metrics
    fn estimate_success_probability(&self, score: u8, mint_features: &MintFeatures) -> f64 {
        // Base probability from score (sigmoid)
        let base_prob = 1.0 / (1.0 + (-(score as f64 - 50.0) / 15.0).exp());
        let base_prob = 0.1 + base_prob * 0.8;  // Map to [0.1, 0.9]
        
        // Adjust based on buy/sell ratio
        let ratio_factor = if mint_features.buys_sells_ratio > 2.0 {
            1.1  // Boost for strong buying pressure
        } else if mint_features.buys_sells_ratio < 0.8 {
            0.8  // Reduce for selling pressure
        } else {
            1.0
        };
        
        // Adjust based on age (fresher = higher success rate)
        let age_factor = if mint_features.age_since_launch < 60 {
            1.15  // Fresh launch bonus
        } else if mint_features.age_since_launch < 300 {
            1.0
        } else {
            0.85  // Older launch penalty
        };
        
        (base_prob * ratio_factor * age_factor).clamp(0.1, 0.9)
    }
}

impl Default for TradeValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fee_estimate() {
        let fees = FeeEstimate::for_position(10.0, 150);  // $10 position, 1.5% slippage
        assert_eq!(fees.jito_tip_usd, 0.10);
        assert_eq!(fees.gas_fee_usd, 0.001);
        assert!((fees.slippage_usd - 0.15).abs() < 0.001);
        assert!((fees.total_usd - 0.251).abs() < 0.001);
    }
    
    #[test]
    fn test_fee_round_trip() {
        let fees = FeeEstimate::round_trip(10.0, 150);
        assert!((fees.total_usd - 0.502).abs() < 0.001);  // Double the single trip
    }
    
    #[test]
    fn test_min_profit_target() {
        let validator = TradeValidator::new();
        
        // With default multiplier 2.2
        let min_profit = validator.calculate_min_profit_target(0.50);
        assert!((min_profit - 1.1).abs() < 0.001);  // 0.50 × 2.2 = 1.1
        
        // Floor at $1.00
        let min_profit = validator.calculate_min_profit_target(0.30);
        assert_eq!(min_profit, 1.0);  // max(1.0, 0.30 × 2.2)
    }
    
    #[test]
    fn test_validation_pass() {
        let validator = TradeValidator::new();
        
        let mint = Pubkey::new_unique();
        let mint_features = MintFeatures {
            age_since_launch: 60,
            current_price: 0.001,
            vol_60s_sol: 25.0,  // Higher liquidity to keep impact below cap
            buyers_60s: 15,
            buys_sells_ratio: 2.5,
            curve_depth_proxy: 1_000_000,
            follow_through_score: 75,
            buyers_2s: 8,
            vol_5s_sol: 10.0,
            last_update: 0,
        };
        
        let result = validator.validate(
            mint,
            &mint_features,
            10.0,   // $10 position
            150,    // 1.5% slippage
            75,     // Good score
            None,
        );
        
        if let Err(e) = &result {
            eprintln!("Validation failed: {:?}", e);
        }
        assert!(result.is_ok(), "Expected validation to pass");
        let validated = result.unwrap();
        assert_eq!(validated.follow_through_score, 75);
        assert!(validated.estimated_fees_usd > 0.0);
    }
    
    #[test]
    fn test_validation_reasonable_fees() {
        let validator = TradeValidator::new();
        
        let mint = Pubkey::new_unique();
        let mint_features = MintFeatures {
            current_price: 0.001,
            vol_60s_sol: 50.0,
            buyers_60s: 25,
            buys_sells_ratio: 2.0,
            curve_depth_proxy: 10_000_000,
            ..Default::default()
        };
        
        // Reasonable parameters should pass
        let result = validator.validate(
            mint,
            &mint_features,
            10.0,
            150,  // 1.5% slippage
            75,
            None,
        );
        
        assert!(result.is_ok(), "Validation should pass with reasonable parameters");
    }
    
    #[test]
    fn test_validation_score_too_low() {
        let validator = TradeValidator::new();
        
        let mint = Pubkey::new_unique();
        let mint_features = MintFeatures {
            current_price: 0.001,
            vol_60s_sol: 50.0,
            buyers_60s: 25,  // Enough buyers
            buys_sells_ratio: 2.5,
            curve_depth_proxy: 10_000_000,
            ..Default::default()
        };
        
        let result = validator.validate(
            mint,
            &mint_features,
            10.0,
            150,
            40,     // Score too low (< 60)
            None,
        );
        
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(err_str.contains("Follow-through too low"), "Expected follow-through error, got: {}", err_str);
    }
    
    #[test]
    fn test_validation_suspicious_pattern() {
        let validator = TradeValidator::new();
        
        let mint = Pubkey::new_unique();
        
        // High volume, low buyers (suspicious)
        let mint_features = MintFeatures {
            current_price: 0.001,
            vol_60s_sol: 50.0,
            buyers_60s: 3,  // Only 3 buyers with 50 SOL volume!
            ..Default::default()
        };
        
        let result = validator.validate(
            mint,
            &mint_features,
            10.0,
            150,
            75,
            None,
        );
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Suspicious pattern"));
    }
    
    #[test]
    fn test_price_impact_estimation() {
        let validator = TradeValidator::new();
        
        // Small position, high liquidity = low impact
        let impact1 = validator.estimate_price_impact(5.0, 1_000_000, 50.0);
        assert!(impact1 < 20.0);
        
        // Large position, low liquidity = high impact
        let impact2 = validator.estimate_price_impact(50.0, 100_000, 5.0);
        assert!(impact2 > impact1);
    }
    
    #[test]
    fn test_validation_config() {
        let mut config = ValidationConfig::default();
        config.min_follow_through_score = 80;  // Higher threshold
        
        let validator = TradeValidator::with_config(config);
        
        let mint = Pubkey::new_unique();
        let mint_features = MintFeatures {
            current_price: 0.001,
            vol_60s_sol: 20.0,
            ..Default::default()
        };
        
        // Score 75 would normally pass, but not with threshold=80
        let result = validator.validate(mint, &mint_features, 10.0, 150, 75, None);
        assert!(result.is_err());
    }
}
