//! 💰 Position Sizer - Calculate optimal position sizes based on risk management
//!
//! Implements portfolio-level risk management with dynamic position sizing.
//! Considers confidence score, available capital, concurrent positions, and risk limits.

use log::{info, debug};

/// Position sizing strategy
#[derive(Debug, Clone)]
pub enum SizingStrategy {
    /// Fixed size regardless of confidence
    Fixed { size_sol: f64 },
    
    /// Scale linearly with confidence (min_size at 0%, max_size at 100%)
    ConfidenceScaled { 
        min_size_sol: f64, 
        max_size_sol: f64 
    },
    
    /// Kelly Criterion-inspired (size = confidence * max_risk)
    KellyCriterion { 
        base_size_sol: f64,
        max_risk_pct: f64,  // Max % of portfolio per trade
    },
    
    /// Tiered sizing based on confidence ranges
    Tiered {
        low_size_sol: f64,    // 0-50% confidence
        mid_size_sol: f64,    // 50-75% confidence
        high_size_sol: f64,   // 75-100% confidence
    },
}

/// Position sizer configuration
#[derive(Debug, Clone)]
pub struct PositionSizerConfig {
    /// Sizing strategy to use
    pub strategy: SizingStrategy,
    
    /// Maximum position size in SOL (absolute cap)
    pub max_position_sol: f64,
    
    /// Minimum position size in SOL
    pub min_position_sol: f64,
    
    /// Total portfolio size in SOL
    pub portfolio_sol: f64,
    
    /// Max % of portfolio per single position
    pub max_position_pct: f64,
    
    /// Risk per trade as % of portfolio
    pub risk_per_trade_pct: f64,
    
    /// Reduce size when near position limit
    pub scale_down_near_limit: bool,
}

impl Default for PositionSizerConfig {
    fn default() -> Self {
        Self {
            strategy: SizingStrategy::ConfidenceScaled {
                min_size_sol: 0.05,
                max_size_sol: 0.2,
            },
            max_position_sol: 0.5,
            min_position_sol: 0.05,
            portfolio_sol: 10.0,
            max_position_pct: 5.0,  // 5% max per position
            risk_per_trade_pct: 2.0, // 2% risk per trade
            scale_down_near_limit: true,
        }
    }
}

/// Position sizer - calculates optimal position sizes
pub struct PositionSizer {
    config: PositionSizerConfig,
}

impl PositionSizer {
    /// Create new position sizer with config
    pub fn new(config: PositionSizerConfig) -> Self {
        info!("💰 Position Sizer initialized:");
        info!("   Strategy: {:?}", config.strategy);
        info!("   Portfolio: {} SOL", config.portfolio_sol);
        info!("   Max position: {} SOL ({:.1}%)", 
              config.max_position_sol, config.max_position_pct);
        info!("   Risk per trade: {:.1}%", config.risk_per_trade_pct);
        
        Self { config }
    }
    
    /// Calculate position size based on confidence and portfolio state
    /// 
    /// # Arguments
    /// * `confidence` - Confidence score (0-100)
    /// * `active_positions` - Number of currently active positions
    /// * `max_positions` - Maximum allowed concurrent positions
    /// * `total_exposure_sol` - Total SOL currently in active positions
    /// 
    /// # Returns
    /// Position size in SOL
    pub fn calculate_size(
        &self,
        confidence: u8,
        active_positions: usize,
        max_positions: usize,
        total_exposure_sol: f64,
    ) -> f64 {
        // 1. Calculate base size from strategy
        let base_size = self.calculate_base_size(confidence);
        
        // 2. Apply portfolio heat limit
        let remaining_capacity = self.config.portfolio_sol - total_exposure_sol;
        let heat_adjusted = base_size.min(remaining_capacity * 0.8); // Leave 20% buffer
        
        // 3. Apply position limit scaling
        let limit_adjusted = if self.config.scale_down_near_limit && max_positions > 0 {
            let utilization = active_positions as f64 / max_positions as f64;
            if utilization >= 0.8 {
                // Reduce size by 50% when 80%+ full
                heat_adjusted * 0.5
            } else if utilization >= 0.6 {
                // Reduce size by 25% when 60%+ full
                heat_adjusted * 0.75
            } else {
                heat_adjusted
            }
        } else {
            heat_adjusted
        };
        
        // 4. Apply absolute limits
        let final_size = limit_adjusted
            .max(self.config.min_position_sol)
            .min(self.config.max_position_sol)
            .min(self.config.portfolio_sol * self.config.max_position_pct / 100.0);
        
        debug!("Position sizing: conf={}, active={}/{}, exposure={:.2} SOL",
               confidence, active_positions, max_positions, total_exposure_sol);
        debug!("  base={:.3}, heat_adj={:.3}, limit_adj={:.3}, final={:.3} SOL",
               base_size, heat_adjusted, limit_adjusted, final_size);
        
        final_size
    }
    
    /// Calculate base size from strategy before adjustments
    fn calculate_base_size(&self, confidence: u8) -> f64 {
        let confidence_f64 = (confidence as f64 / 100.0).clamp(0.0, 1.0);
        
        match &self.config.strategy {
            SizingStrategy::Fixed { size_sol } => *size_sol,
            
            SizingStrategy::ConfidenceScaled { min_size_sol, max_size_sol } => {
                // Linear interpolation: size = min + (max - min) * confidence
                min_size_sol + (max_size_sol - min_size_sol) * confidence_f64
            }
            
            SizingStrategy::KellyCriterion { base_size_sol, max_risk_pct } => {
                // Kelly fraction: f = (bp - q) / b
                // Simplified: size = base * confidence * max_risk
                base_size_sol * confidence_f64 * (max_risk_pct / 100.0)
            }
            
            SizingStrategy::Tiered { low_size_sol, mid_size_sol, high_size_sol } => {
                if confidence < 50 {
                    *low_size_sol
                } else if confidence < 75 {
                    *mid_size_sol
                } else {
                    *high_size_sol
                }
            }
        }
    }
    
    /// Get current portfolio utilization %
    pub fn get_portfolio_utilization(&self, total_exposure_sol: f64) -> f64 {
        (total_exposure_sol / self.config.portfolio_sol * 100.0).min(100.0)
    }
    
    /// Check if new position would exceed portfolio heat
    pub fn check_portfolio_heat(
        &self,
        total_exposure_sol: f64,
        new_position_sol: f64,
    ) -> Result<(), String> {
        let new_total = total_exposure_sol + new_position_sol;
        let utilization_pct = (new_total / self.config.portfolio_sol) * 100.0;
        
        if utilization_pct > 90.0 {
            return Err(format!(
                "Portfolio heat too high: {:.1}% (max 90%)",
                utilization_pct
            ));
        }
        
        Ok(())
    }
    
    /// Get recommended size for a given scenario (for testing/logging)
    pub fn get_recommended_size(&self, confidence: u8) -> f64 {
        self.calculate_size(confidence, 0, 10, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fixed_sizing() {
        let config = PositionSizerConfig {
            strategy: SizingStrategy::Fixed { size_sol: 0.1 },
            ..Default::default()
        };
        
        let sizer = PositionSizer::new(config);
        
        // Should be 0.1 regardless of confidence
        assert_eq!(sizer.calculate_size(20, 0, 10, 0.0), 0.1);
        assert_eq!(sizer.calculate_size(80, 0, 10, 0.0), 0.1);
    }
    
    #[test]
    fn test_confidence_scaled_sizing() {
        let config = PositionSizerConfig {
            strategy: SizingStrategy::ConfidenceScaled {
                min_size_sol: 0.05,
                max_size_sol: 0.2,
            },
            ..Default::default()
        };
        
        let sizer = PositionSizer::new(config);
        
        // 0% confidence -> min size
        assert_eq!(sizer.calculate_size(0, 0, 10, 0.0), 0.05);
        
        // 50% confidence -> midpoint
        let mid = sizer.calculate_size(50, 0, 10, 0.0);
        assert!((mid - 0.125).abs() < 0.001);
        
        // 100% confidence -> max size
        assert_eq!(sizer.calculate_size(100, 0, 10, 0.0), 0.2);
    }
    
    #[test]
    fn test_portfolio_heat_scaling() {
        let config = PositionSizerConfig {
            strategy: SizingStrategy::Fixed { size_sol: 2.0 },
            max_position_sol: 2.0,
            max_position_pct: 20.0,  // Allow up to 20% per position (2 SOL out of 10 SOL)
            portfolio_sol: 10.0,
            ..Default::default()
        };
        
        let sizer = PositionSizer::new(config);
        
        // With 9 SOL already exposed, remaining capacity is 1 SOL
        // Should cap at 1 SOL * 0.8 = 0.8 SOL (leaving 20% buffer)
        let size_high_heat = sizer.calculate_size(80, 0, 10, 9.0);
        assert!(size_high_heat < 1.0);
        assert!(size_high_heat <= 0.8);
        
        // With 0 SOL exposed, should allow full size (up to max)
        let size_no_heat = sizer.calculate_size(80, 0, 10, 0.0);
        assert_eq!(size_no_heat, 2.0);
    }
    
    #[test]
    fn test_position_limit_scaling() {
        let config = PositionSizerConfig {
            strategy: SizingStrategy::Fixed { size_sol: 0.2 },
            scale_down_near_limit: true,
            ..Default::default()
        };
        
        let sizer = PositionSizer::new(config);
        
        // At 2/10 positions (20%), should use full size
        let size_low = sizer.calculate_size(80, 2, 10, 0.0);
        assert_eq!(size_low, 0.2);
        
        // At 7/10 positions (70%), should reduce by 25%
        let size_mid = sizer.calculate_size(80, 7, 10, 0.0);
        assert!((size_mid - 0.15).abs() < 0.001);
        
        // At 9/10 positions (90%), should reduce by 50%
        let size_high = sizer.calculate_size(80, 9, 10, 0.0);
        assert_eq!(size_high, 0.1);
    }
    
    #[test]
    fn test_absolute_limits() {
        let config = PositionSizerConfig {
            strategy: SizingStrategy::Fixed { size_sol: 1.0 },
            max_position_sol: 0.5,
            min_position_sol: 0.05,
            portfolio_sol: 10.0,
            max_position_pct: 5.0,
            ..Default::default()
        };
        
        let sizer = PositionSizer::new(config.clone());
        
        // Should cap at max_position_sol (0.5)
        let size = sizer.calculate_size(100, 0, 10, 0.0);
        assert_eq!(size, 0.5);
        
        // Should also respect max_position_pct (5% of 10 SOL = 0.5)
        assert!(size <= config.portfolio_sol * config.max_position_pct / 100.0);
    }
    
    #[test]
    fn test_portfolio_heat_check() {
        let config = PositionSizerConfig {
            portfolio_sol: 10.0,
            ..Default::default()
        };
        
        let sizer = PositionSizer::new(config);
        
        // 5 SOL exposed + 4 SOL new = 90% utilization (OK)
        assert!(sizer.check_portfolio_heat(5.0, 4.0).is_ok());
        
        // 5 SOL exposed + 5.1 SOL new = 101% utilization (BLOCKED)
        assert!(sizer.check_portfolio_heat(5.0, 5.1).is_err());
    }
}
