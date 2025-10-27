//! 📡 UDP Bus - Communication layer for Brain service
//! 
//! Handles all UDP-based messaging:
//! - Decision Bus (port 45110): Brain → Executor
//! - Advice Bus (port 45100): Collectors → Brain

pub mod messages;
pub mod sender;
pub mod receiver;

pub use messages::{TradeDecision, HeatPulse, AdviceMessage, LateOpportunityAdvice, CopyTradeAdvice};
pub use sender::{DecisionBusSender, DecisionBatchSender};
pub use receiver::{AdviceBusReceiver, ReceiverStats};
