pub mod scoring;
pub mod validation;
pub mod triggers;
pub mod guardrails;
pub mod logging;
pub mod position_tracker;
pub mod position_sizer;
pub mod early_scorer;  // 7-signal scoring for 1M+ MC detection

// Re-export main types for convenience
pub use scoring::FollowThroughScorer;
pub use validation::{TradeValidator, ValidatedTrade, ValidationError};
pub use triggers::TriggerEngine;
pub use guardrails::Guardrails;
pub use logging::{DecisionLogger, DecisionLogEntry, TriggerType};
pub use position_tracker::{PositionTracker, ActivePosition, ExitReason};
pub use position_sizer::{PositionSizer, PositionSizerConfig, SizingStrategy};
pub use early_scorer::{EarlyScorer, EarlyScore, EarlyScorerConfig};

// Type aliases for easier use in main.rs
pub type Scorer = FollowThroughScorer;
pub type Validator = TradeValidator;
pub type ValidationResult = Result<ValidatedTrade, ValidationError>;
