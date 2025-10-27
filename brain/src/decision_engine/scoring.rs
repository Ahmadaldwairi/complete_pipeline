//! 📊 Follow-Through Scoring Algorithm
//!
//! Computes 0-100 momentum score based on:
//! - 40% Buyer momentum (buyers_2s normalized)
//! - 40% Volume momentum (vol_5s_sol normalized)
//! - 20% Wallet quality (avg tier/confidence of recent buyers)
//!
//! Higher scores indicate stronger momentum and higher probability of follow-through.

use crate::feature_cache::{MintFeatures, WalletFeatures, WalletTier};
use log::debug;

/// Components of the follow-through score
#[derive(Debug, Clone)]
pub struct ScoreComponents {
    /// Buyer momentum score (0-100)
    pub buyer_score: u8,
    
    /// Volume momentum score (0-100)
    pub volume_score: u8,
    
    /// Wallet quality score (0-100)
    pub wallet_quality_score: u8,
    
    /// Combined follow-through score (0-100)
    pub total_score: u8,
    
    /// Raw metrics used in calculation
    pub buyers_2s: u32,
    pub vol_5s_sol: f64,
    pub avg_wallet_confidence: f64,
}

impl ScoreComponents {
    /// Create a breakdown string for logging
    pub fn breakdown(&self) -> String {
        format!(
            "FT={} (buyers={}, vol={}, quality={}) | raw: {}b, {:.1}SOL, conf={:.0}",
            self.total_score,
            self.buyer_score,
            self.volume_score,
            self.wallet_quality_score,
            self.buyers_2s,
            self.vol_5s_sol,
            self.avg_wallet_confidence
        )
    }
}

/// Follow-through scorer with configurable thresholds
pub struct FollowThroughScorer {
    /// Maximum expected buyers in 2s (for normalization)
    max_buyers_2s: u32,
    
    /// Maximum expected volume in 5s SOL (for normalization)
    max_vol_5s: f64,
    
    /// Weight for buyer momentum (default: 0.4)
    buyer_weight: f64,
    
    /// Weight for volume momentum (default: 0.4)
    volume_weight: f64,
    
    /// Weight for wallet quality (default: 0.2)
    quality_weight: f64,
}

impl Default for FollowThroughScorer {
    fn default() -> Self {
        Self {
            max_buyers_2s: 20,      // 20 unique buyers in 2s is very hot
            max_vol_5s: 50.0,       // 50 SOL in 5s is extremely high
            buyer_weight: 0.4,
            volume_weight: 0.4,
            quality_weight: 0.2,
        }
    }
}

impl FollowThroughScorer {
    /// Create new scorer with default settings
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create scorer with custom thresholds
    pub fn with_thresholds(max_buyers: u32, max_volume: f64) -> Self {
        Self {
            max_buyers_2s: max_buyers,
            max_vol_5s: max_volume,
            ..Self::default()
        }
    }
    
    /// Create scorer with custom weights
    pub fn with_weights(buyer_weight: f64, volume_weight: f64, quality_weight: f64) -> Self {
        // Ensure weights sum to 1.0
        let total = buyer_weight + volume_weight + quality_weight;
        Self {
            buyer_weight: buyer_weight / total,
            volume_weight: volume_weight / total,
            quality_weight: quality_weight / total,
            ..Self::default()
        }
    }
    
    /// Calculate follow-through score from mint features
    /// 
    /// This is the simplified version that uses pre-computed metrics from MintFeatures.
    /// For real-time scoring with wallet data, use `calculate_with_wallets()`.
    pub fn calculate(&self, mint_features: &MintFeatures) -> ScoreComponents {
        // Extract metrics from mint features
        let buyers_2s = mint_features.buyers_2s;
        let vol_5s_sol = mint_features.vol_5s_sol;
        
        // Use the follow_through_score from mint cache as wallet quality proxy
        // (since we don't have individual wallet data in this simplified version)
        let wallet_quality_proxy = mint_features.follow_through_score as f64 / 100.0;
        
        // Calculate individual component scores
        let buyer_score = self.score_buyers(buyers_2s);
        let volume_score = self.score_volume(vol_5s_sol);
        let wallet_quality_score = (wallet_quality_proxy * 100.0) as u8;
        
        // Compute weighted total
        let total_score = (
            (buyer_score as f64 * self.buyer_weight) +
            (volume_score as f64 * self.volume_weight) +
            (wallet_quality_score as f64 * self.quality_weight)
        ).round() as u8;
        
        ScoreComponents {
            buyer_score,
            volume_score,
            wallet_quality_score,
            total_score: total_score.min(100),
            buyers_2s,
            vol_5s_sol,
            avg_wallet_confidence: wallet_quality_proxy * 100.0,
        }
    }
    
    /// Calculate follow-through score with detailed wallet analysis
    /// 
    /// Takes recent buyer wallet addresses and looks up their quality metrics.
    /// This provides more accurate wallet quality scoring than the cache proxy.
    pub fn calculate_with_wallets(
        &self,
        mint_features: &MintFeatures,
        recent_wallets: &[WalletFeatures],
    ) -> ScoreComponents {
        // Extract metrics
        let buyers_2s = mint_features.buyers_2s;
        let vol_5s_sol = mint_features.vol_5s_sol;
        
        // Calculate buyer and volume scores
        let buyer_score = self.score_buyers(buyers_2s);
        let volume_score = self.score_volume(vol_5s_sol);
        
        // Calculate wallet quality from actual wallet features
        let wallet_quality_score = self.score_wallet_quality(recent_wallets);
        let avg_wallet_confidence = if !recent_wallets.is_empty() {
            recent_wallets.iter().map(|w| w.confidence as f64).sum::<f64>() / recent_wallets.len() as f64
        } else {
            50.0
        };
        
        // Compute weighted total
        let total_score = (
            (buyer_score as f64 * self.buyer_weight) +
            (volume_score as f64 * self.volume_weight) +
            (wallet_quality_score as f64 * self.quality_weight)
        ).round() as u8;
        
        debug!(
            "📊 Follow-through: total={} (buyers={}, vol={}, quality={}) | {}/{}b, {:.2}SOL",
            total_score, buyer_score, volume_score, wallet_quality_score,
            buyers_2s, self.max_buyers_2s, vol_5s_sol
        );
        
        ScoreComponents {
            buyer_score,
            volume_score,
            wallet_quality_score,
            total_score: total_score.min(100),
            buyers_2s,
            vol_5s_sol,
            avg_wallet_confidence,
        }
    }
    
    /// Score buyer momentum (0-100)
    /// 
    /// Uses sigmoid-like curve for smooth normalization:
    /// - 0-5 buyers: Linear scaling (0-50 points)
    /// - 5-20 buyers: Logarithmic scaling (50-100 points)
    fn score_buyers(&self, buyers_2s: u32) -> u8 {
        if buyers_2s == 0 {
            return 0;
        }
        
        // Linear scaling for low buyer counts (0-5 buyers → 0-50 points)
        if buyers_2s <= 5 {
            return ((buyers_2s as f64 / 5.0) * 50.0) as u8;
        }
        
        // Logarithmic scaling for higher counts (5-max → 50-100 points)
        let normalized = (buyers_2s as f64 / self.max_buyers_2s as f64).min(1.0);
        let log_score = (normalized.ln() + 1.0).max(0.0);  // ln(1)=0, ln(e)=1
        
        (50.0 + log_score * 50.0) as u8
    }
    
    /// Score volume momentum (0-100)
    /// 
    /// Uses square root curve for diminishing returns:
    /// - sqrt(vol/max) gives better scaling for volume
    /// - 0 SOL → 0 points
    /// - 8 SOL → ~63 points (threshold for Path B)
    /// - 25 SOL → 100 points
    fn score_volume(&self, vol_5s_sol: f64) -> u8 {
        if vol_5s_sol <= 0.0 {
            return 0;
        }
        
        // Square root normalization (diminishing returns)
        let normalized = (vol_5s_sol / self.max_vol_5s).min(1.0);
        let sqrt_score = normalized.sqrt();
        
        (sqrt_score * 100.0) as u8
    }
    
    /// Score wallet quality based on tier distribution (0-100)
    /// 
    /// Weighted by tier:
    /// - Tier A wallets: 95 points
    /// - Tier B wallets: 85 points
    /// - Tier C wallets: 75 points
    /// - Discovery: 50 points
    fn score_wallet_quality(&self, wallets: &[WalletFeatures]) -> u8 {
        if wallets.is_empty() {
            return 50;  // Neutral score if no wallet data
        }
        
        // Calculate weighted average based on tier scores
        let total_score: f64 = wallets.iter().map(|w| {
            match w.tier {
                WalletTier::A => 95.0,
                WalletTier::B => 85.0,
                WalletTier::C => 75.0,
                WalletTier::Discovery => w.bootstrap_score as f64,
            }
        }).sum();
        
        let avg_score = total_score / wallets.len() as f64;
        avg_score.round() as u8
    }
    
    /// Quick check if score meets minimum threshold for entry
    pub fn meets_threshold(&self, score: u8, threshold: u8) -> bool {
        score >= threshold
    }
    
    /// Get recommended position size multiplier based on score
    /// 
    /// Returns multiplier for base position size:
    /// - Score 0-40: 0.5x (low confidence, reduce size)
    /// - Score 40-60: 0.75x
    /// - Score 60-80: 1.0x (normal size)
    /// - Score 80-90: 1.25x
    /// - Score 90-100: 1.5x (high confidence, increase size)
    pub fn position_size_multiplier(&self, score: u8) -> f64 {
        match score {
            0..=39 => 0.5,
            40..=59 => 0.75,
            60..=79 => 1.0,
            80..=89 => 1.25,
            90..=100 => 1.5,
            _ => 1.0,
        }
    }
    
    /// Estimate probability of follow-through based on historical data
    /// 
    /// This is a calibrated mapping from score to success probability.
    /// Actual values should be tuned based on backtesting results.
    pub fn estimate_success_probability(&self, score: u8) -> f64 {
        // Sigmoid curve calibrated to realistic probabilities
        // score=50 → ~30% success
        // score=70 → ~55% success
        // score=85 → ~75% success
        let x = (score as f64 - 50.0) / 15.0;  // Centered at 50, scale=15
        let sigmoid = 1.0 / (1.0 + (-x).exp());
        
        // Map sigmoid [0,1] to probability range [10%, 90%]
        0.1 + sigmoid * 0.8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_buyer_scoring() {
        let scorer = FollowThroughScorer::new();
        
        // Zero buyers
        assert_eq!(scorer.score_buyers(0), 0);
        
        // Low counts (linear)
        assert_eq!(scorer.score_buyers(1), 10);  // 1/5 * 50 = 10
        assert_eq!(scorer.score_buyers(3), 30);  // 3/5 * 50 = 30
        assert_eq!(scorer.score_buyers(5), 50);  // 5/5 * 50 = 50
        
        // Higher counts (logarithmic)
        let score_10 = scorer.score_buyers(10);
        let score_20 = scorer.score_buyers(20);
        assert!(score_10 > 50 && score_10 < 100);
        assert_eq!(score_20, 100);  // Max at threshold
    }
    
    #[test]
    fn test_volume_scoring() {
        let scorer = FollowThroughScorer::new();
        
        // Zero volume
        assert_eq!(scorer.score_volume(0.0), 0);
        
        // Various volumes (sqrt scaling)
        // 5 SOL: sqrt(5/50) = sqrt(0.1) ≈ 0.316 → 31 points
        assert!(scorer.score_volume(5.0) >= 30 && scorer.score_volume(5.0) <= 35);
        
        // 8 SOL: sqrt(8/50) = sqrt(0.16) = 0.4 → 40 points (Path B threshold)
        assert!(scorer.score_volume(8.0) >= 38 && scorer.score_volume(8.0) <= 42);
        
        // 25 SOL: sqrt(25/50) = sqrt(0.5) ≈ 0.707 → 70 points
        assert!(scorer.score_volume(25.0) >= 68 && scorer.score_volume(25.0) <= 72);
        
        // 50 SOL: sqrt(50/50) = 1.0 → 100 points (max)
        assert_eq!(scorer.score_volume(50.0), 100);
    }
    
    #[test]
    fn test_wallet_quality_scoring() {
        let scorer = FollowThroughScorer::new();
        
        // Empty wallets
        assert_eq!(scorer.score_wallet_quality(&[]), 50);
        
        // Single tier A wallet
        let wallet_a = WalletFeatures {
            tier: WalletTier::A,
            ..Default::default()
        };
        assert_eq!(scorer.score_wallet_quality(&[wallet_a.clone()]), 95);
        
        // Mixed tiers
        let wallet_b = WalletFeatures {
            tier: WalletTier::B,
            ..Default::default()
        };
        let wallet_c = WalletFeatures {
            tier: WalletTier::C,
            ..Default::default()
        };
        
        // Average: (95 + 85 + 75) / 3 = 85
        let mixed = vec![wallet_a, wallet_b, wallet_c];
        assert_eq!(scorer.score_wallet_quality(&mixed), 85);
    }
    
    #[test]
    fn test_full_scoring() {
        let scorer = FollowThroughScorer::new();
        
        // Create mint features with good momentum
        let mint = MintFeatures {
            buyers_2s: 8,           // >5, so uses log scaling: normalized = 8/20 = 0.4, ln(0.4) ≈ -0.916
            vol_5s_sol: 15.0,       // sqrt(15/50) = sqrt(0.3) ≈ 0.548 → 54 points
            follow_through_score: 70,  // 70 wallet quality proxy
            ..Default::default()
        };
        
        let components = scorer.calculate(&mint);
        
        // Buyer score for 8: ln(0.4) + 1 = 0.084, score = 50 + 0.084*50 = 54
        // Total: 0.4*54 + 0.4*54 + 0.2*70 = 21.6 + 21.6 + 14 = 57.2 ≈ 57
        assert!(components.total_score >= 52 && components.total_score <= 62);
        assert!(components.buyer_score >= 50);  // 8 buyers (log scale)
        assert!(components.volume_score >= 50);  // 15 SOL is solid
    }
    
    #[test]
    fn test_scoring_with_wallets() {
        let scorer = FollowThroughScorer::new();
        
        let mint = MintFeatures {
            buyers_2s: 10,       // Should score ~70
            vol_5s_sol: 20.0,    // Should score ~63
            ..Default::default()
        };
        
        // Create quality wallet features
        let wallets = vec![
            WalletFeatures {
                tier: WalletTier::A,
                confidence: 93,
                ..Default::default()
            },
            WalletFeatures {
                tier: WalletTier::B,
                confidence: 87,
                ..Default::default()
            },
        ];
        
        let components = scorer.calculate_with_wallets(&mint, &wallets);
        
        // Expected: 0.4*70 + 0.4*63 + 0.2*90 = 28 + 25.2 + 18 = 71.2 ≈ 71
        assert!(components.total_score >= 68 && components.total_score <= 75);
        assert!(components.wallet_quality_score >= 85);  // (95+85)/2 = 90
        assert!(components.avg_wallet_confidence >= 85.0 && components.avg_wallet_confidence <= 92.0);
    }
    
    #[test]
    fn test_position_size_multiplier() {
        let scorer = FollowThroughScorer::new();
        
        assert_eq!(scorer.position_size_multiplier(30), 0.5);
        assert_eq!(scorer.position_size_multiplier(50), 0.75);
        assert_eq!(scorer.position_size_multiplier(70), 1.0);
        assert_eq!(scorer.position_size_multiplier(85), 1.25);
        assert_eq!(scorer.position_size_multiplier(95), 1.5);
    }
    
    #[test]
    fn test_success_probability() {
        let scorer = FollowThroughScorer::new();
        
        // Low score → low probability
        let prob_30 = scorer.estimate_success_probability(30);
        assert!(prob_30 >= 0.1 && prob_30 < 0.35);
        
        // Medium score → medium probability
        // score=60: x=(60-50)/15 = 0.667, sigmoid ≈ 0.66, prob ≈ 0.1 + 0.66*0.8 = 0.628
        let prob_60 = scorer.estimate_success_probability(60);
        assert!(prob_60 >= 0.55 && prob_60 <= 0.70);
        
        // High score → high probability
        let prob_85 = scorer.estimate_success_probability(85);
        assert!(prob_85 >= 0.75 && prob_85 <= 0.90);
    }
    
    #[test]
    fn test_meets_threshold() {
        let scorer = FollowThroughScorer::new();
        
        assert!(scorer.meets_threshold(70, 60));
        assert!(!scorer.meets_threshold(55, 60));
        assert!(scorer.meets_threshold(60, 60));  // Exactly at threshold
    }
    
    #[test]
    fn test_custom_weights() {
        let scorer = FollowThroughScorer::with_weights(0.5, 0.3, 0.2);
        
        // Weights should sum to 1.0 (normalized internally)
        assert!((scorer.buyer_weight - 0.5).abs() < 0.001);
        assert!((scorer.volume_weight - 0.3).abs() < 0.001);
        assert!((scorer.quality_weight - 0.2).abs() < 0.001);
    }
    
    #[test]
    fn test_score_breakdown() {
        let components = ScoreComponents {
            buyer_score: 75,
            volume_score: 85,
            wallet_quality_score: 80,
            total_score: 80,
            buyers_2s: 12,
            vol_5s_sol: 18.5,
            avg_wallet_confidence: 87.0,
        };
        
        let breakdown = components.breakdown();
        assert!(breakdown.contains("FT=80"));
        assert!(breakdown.contains("12b"));
        assert!(breakdown.contains("18.5SOL"));
    }
}
