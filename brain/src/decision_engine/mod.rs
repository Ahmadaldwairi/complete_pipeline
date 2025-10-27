pub mod scoring;
pub mod validation;
pub mod triggers;
pub mod guardrails;
pub mod logging;
pub mod position_tracker;
pub mod position_sizer;

// Re-export main types for convenience
pub use scoring::{FollowThroughScorer, ScoreComponents};
pub use validation::{TradeValidator, ValidatedTrade, ValidationConfig, ValidationError, FeeEstimate};
pub use triggers::{TriggerEngine, TriggerConfig, EntryTrigger};
pub use guardrails::{Guardrails, GuardrailConfig, GuardrailStats, TradeOutcome};
pub use logging::{DecisionLogger, DecisionLogEntry, DecisionLogBuilder, TriggerType};
pub use position_tracker::{PositionTracker, ActivePosition, ExitReason};
pub use position_sizer::{PositionSizer, PositionSizerConfig, SizingStrategy};

// Type aliases for easier use in main.rs
pub type Scorer = FollowThroughScorer;
pub type Validator = TradeValidator;
pub type ValidationResult = Result<ValidatedTrade, ValidationError>;
