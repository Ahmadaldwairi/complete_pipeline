//! 📡 UDP Bus - Communication layer for Brain service
//! 
//! Handles all UDP-based messaging:
//! - Decision Bus (port 45110): Brain → Executor
//! - Advice Bus (port 45100): Collectors → Brain

pub mod messages;
pub mod sender;
pub mod receiver;
pub mod deduplicator;
pub mod tx_confirmed_context;
pub mod exit_advice;
pub mod position_update;

pub use messages::{
    TradeDecision, AdviceMessage, 
    LateOpportunityAdvice, CopyTradeAdvice,
    MomentumOpportunityAdvice, RankOpportunityAdvice,
    ExecutionConfirmation, TxConfirmed,
};
pub use sender::DecisionBusSender;
pub use receiver::AdviceBusReceiver;
pub use tx_confirmed_context::TxConfirmedContext;
pub use exit_advice::ExitAdvice;
pub use position_update::PositionUpdate;
pub use deduplicator::{MessageDeduplicator, DeduplicationStats};
