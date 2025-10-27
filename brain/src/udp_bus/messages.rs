//! 📡 UDP Message Definitions for Brain ↔ Executor Communication
//! 
//! Fixed-size binary packets optimized for localhost UDP transmission.
//! All structs are #[repr(C)] for predictable memory layout and zero-copy serialization.

use anyhow::{Result, Context};

/// 📦 TradeDecision - Brain → Executor (Port 45110)
/// 
/// 52-byte packet containing a validated trade decision ready for immediate execution.
/// The executor receives this and builds+sends the transaction without additional logic.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TradeDecision {
    /// Message type identifier (1 = TRADE_DECISION)
    pub msg_type: u8,
    
    /// Token mint address (32 bytes, Solana Pubkey)
    pub mint: [u8; 32],
    
    /// Trade side: 0 = BUY, 1 = SELL
    pub side: u8,
    
    /// Trade size in lamports (u64, 1 SOL = 1_000_000_000 lamports)
    pub size_lamports: u64,
    
    /// Slippage tolerance in basis points (u16, e.g., 150 = 1.5%)
    pub slippage_bps: u16,
    
    /// Confidence score 0-100 (computed by Brain based on tier/signals)
    pub confidence: u8,
    
    /// Padding to align to 52 bytes
    pub _padding: [u8; 5],
}

impl TradeDecision {
    /// Total packet size in bytes
    pub const SIZE: usize = 52;
    
    /// Message type constant
    pub const MSG_TYPE: u8 = 1;
    
    /// Create a new BUY decision
    pub fn new_buy(mint: [u8; 32], size_lamports: u64, slippage_bps: u16, confidence: u8) -> Self {
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            side: 0, // BUY
            size_lamports,
            slippage_bps,
            confidence,
            _padding: [0; 5],
        }
    }
    
    /// Create a new SELL decision
    pub fn new_sell(mint: [u8; 32], size_lamports: u64, slippage_bps: u16, confidence: u8) -> Self {
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            side: 1, // SELL
            size_lamports,
            slippage_bps,
            confidence,
            _padding: [0; 5],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = self.msg_type;
        buf[1..33].copy_from_slice(&self.mint);
        buf[33] = self.side;
        buf[34..42].copy_from_slice(&self.size_lamports.to_le_bytes());
        buf[42..44].copy_from_slice(&self.slippage_bps.to_le_bytes());
        buf[44] = self.confidence;
        // Padding already zeros
        buf
    }
    
    /// Deserialize from UDP packet bytes
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            anyhow::bail!("TradeDecision packet too short: {} bytes", buf.len());
        }
        
        if buf[0] != Self::MSG_TYPE {
            anyhow::bail!("Invalid message type: expected {}, got {}", Self::MSG_TYPE, buf[0]);
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&buf[1..33]);
        
        let side = buf[33];
        let size_lamports = u64::from_le_bytes(buf[34..42].try_into()?);
        let slippage_bps = u16::from_le_bytes(buf[42..44].try_into()?);
        let confidence = buf[44];
        
        Ok(Self {
            msg_type: Self::MSG_TYPE,
            mint,
            side,
            size_lamports,
            slippage_bps,
            confidence,
            _padding: [0; 5],
        })
    }
    
    /// Check if this is a BUY decision
    pub fn is_buy(&self) -> bool {
        self.side == 0
    }
    
    /// Check if this is a SELL decision
    pub fn is_sell(&self) -> bool {
        self.side == 1
    }
    
    /// Get mint address as hex string (first 12 chars for logging)
    pub fn mint_short(&self) -> String {
        bs58::encode(&self.mint).into_string()[..12].to_string()
    }
    
    /// Get trade size in SOL (for logging)
    pub fn size_sol(&self) -> f64 {
        self.size_lamports as f64 / 1_000_000_000.0
    }
}

/// 🔥 HeatPulse - Mempool Monitor → Brain (Future)
/// 
/// 64-byte packet containing real-time mempool heat signals for a specific token.
/// Allows Brain to override decision thresholds during high-momentum periods.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct HeatPulse {
    /// Message type identifier (6 = HEAT_PULSE)
    pub msg_type: u8,
    
    /// Token mint address (32 bytes)
    pub mint: [u8; 32],
    
    /// Aggregation window in milliseconds (e.g., 200, 500, 2000)
    pub window_ms: u16,
    
    /// Number of pending buy transactions detected
    pub pending_buys: u16,
    
    /// Total pending SOL volume in basis points (e.g., 235 = 2.35 SOL)
    pub pending_sol_bps: u32,
    
    /// Number of unique sender addresses
    pub uniq_senders: u8,
    
    /// Jito bundle detected flag (0 = no, 1 = yes)
    pub jito_seen: u8,
    
    /// Heat score 0-100 (computed by mempool monitor)
    pub score: u8,
    
    /// Time-to-live in milliseconds (validity window)
    pub ttl_ms: u16,
    
    /// Padding to align to 64 bytes
    pub _padding: [u8; 20],
}

impl HeatPulse {
    /// Total packet size in bytes
    pub const SIZE: usize = 64;
    
    /// Message type constant
    pub const MSG_TYPE: u8 = 6;
    
    /// Create a new HeatPulse
    pub fn new(
        mint: [u8; 32],
        window_ms: u16,
        pending_buys: u16,
        pending_sol_bps: u32,
        uniq_senders: u8,
        jito_seen: bool,
        score: u8,
        ttl_ms: u16,
    ) -> Self {
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            window_ms,
            pending_buys,
            pending_sol_bps,
            uniq_senders,
            jito_seen: if jito_seen { 1 } else { 0 },
            score,
            ttl_ms,
            _padding: [0; 20],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = self.msg_type;
        buf[1..33].copy_from_slice(&self.mint);
        buf[33..35].copy_from_slice(&self.window_ms.to_le_bytes());
        buf[35..37].copy_from_slice(&self.pending_buys.to_le_bytes());
        buf[37..41].copy_from_slice(&self.pending_sol_bps.to_le_bytes());
        buf[41] = self.uniq_senders;
        buf[42] = self.jito_seen;
        buf[43] = self.score;
        buf[44..46].copy_from_slice(&self.ttl_ms.to_le_bytes());
        // Padding already zeros
        buf
    }
    
    /// Deserialize from UDP packet bytes
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            anyhow::bail!("HeatPulse packet too short: {} bytes", buf.len());
        }
        
        if buf[0] != Self::MSG_TYPE {
            anyhow::bail!("Invalid message type: expected {}, got {}", Self::MSG_TYPE, buf[0]);
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&buf[1..33]);
        
        let window_ms = u16::from_le_bytes(buf[33..35].try_into()?);
        let pending_buys = u16::from_le_bytes(buf[35..37].try_into()?);
        let pending_sol_bps = u32::from_le_bytes(buf[37..41].try_into()?);
        let uniq_senders = buf[41];
        let jito_seen = buf[42];
        let score = buf[43];
        let ttl_ms = u16::from_le_bytes(buf[44..46].try_into()?);
        
        Ok(Self {
            msg_type: Self::MSG_TYPE,
            mint,
            window_ms,
            pending_buys,
            pending_sol_bps,
            uniq_senders,
            jito_seen,
            score,
            ttl_ms,
            _padding: [0; 20],
        })
    }
    
    /// Get pending SOL volume as float
    pub fn pending_sol(&self) -> f64 {
        self.pending_sol_bps as f64 / 100.0
    }
    
    /// Check if Jito bundle was detected
    pub fn has_jito(&self) -> bool {
        self.jito_seen == 1
    }
    
    /// Get mint address as hex string (first 12 chars for logging)
    pub fn mint_short(&self) -> String {
        bs58::encode(&self.mint).into_string()[..12].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_trade_decision_serialization() {
        let mint = [1u8; 32];
        let decision = TradeDecision::new_buy(mint, 1_000_000_000, 150, 95);
        
        let bytes = decision.to_bytes();
        assert_eq!(bytes.len(), TradeDecision::SIZE);
        
        let decoded = TradeDecision::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.mint, mint);
        assert_eq!(decoded.side, 0);
        assert_eq!(decoded.size_lamports, 1_000_000_000);
        assert_eq!(decoded.slippage_bps, 150);
        assert_eq!(decoded.confidence, 95);
        assert!(decoded.is_buy());
    }
    
    #[test]
    fn test_heat_pulse_serialization() {
        let mint = [2u8; 32];
        let pulse = HeatPulse::new(mint, 200, 5, 235, 4, true, 85, 2000);
        
        let bytes = pulse.to_bytes();
        assert_eq!(bytes.len(), HeatPulse::SIZE);
        
        let decoded = HeatPulse::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.mint, mint);
        assert_eq!(decoded.window_ms, 200);
        assert_eq!(decoded.pending_buys, 5);
        assert_eq!(decoded.pending_sol_bps, 235);
        assert_eq!(decoded.uniq_senders, 4);
        assert!(decoded.has_jito());
        assert_eq!(decoded.score, 85);
        assert_eq!(decoded.ttl_ms, 2000);
    }
}

// ============================================================================
// Advice Bus Messages (Incoming from WalletTracker and LaunchTracker)
// ============================================================================

/// Message types for Advice Bus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdviceMessageType {
    ExtendHold = 10,        // Suggest extending hold period
    WidenExit = 11,         // Suggest widening exit spread
    LateOpportunity = 12,   // Mature launch opportunity
    CopyTrade = 13,         // Copy a profitable wallet
    SolPriceUpdate = 14,    // SOL price update
}

impl AdviceMessageType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            10 => Some(Self::ExtendHold),
            11 => Some(Self::WidenExit),
            12 => Some(Self::LateOpportunity),
            13 => Some(Self::CopyTrade),
            14 => Some(Self::SolPriceUpdate),
            _ => None,
        }
    }
}

/// ExtendHold advice - suggest holding position longer
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct ExtendHoldAdvice {
    pub msg_type: u8,           // 10
    pub mint: [u8; 32],         // Token mint
    pub current_hold_secs: u32, // Current hold duration
    pub suggested_hold_secs: u32, // Suggested hold duration
    pub reason_code: u8,        // Reason: 1=momentum, 2=whale_entry, 3=volume_surge
    pub confidence: u8,         // 0-100
    pub _padding: [u8; 6],
}

impl ExtendHoldAdvice {
    pub const SIZE: usize = 48;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            current_hold_secs: u32::from_le_bytes([bytes[33], bytes[34], bytes[35], bytes[36]]),
            suggested_hold_secs: u32::from_le_bytes([bytes[37], bytes[38], bytes[39], bytes[40]]),
            reason_code: bytes[41],
            confidence: bytes[42],
            _padding: [0u8; 6],
        })
    }
}

/// WidenExit advice - suggest widening exit spread
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct WidenExitAdvice {
    pub msg_type: u8,           // 11
    pub mint: [u8; 32],         // Token mint
    pub current_tp_bps: u16,    // Current take-profit (bps)
    pub suggested_tp_bps: u16,  // Suggested take-profit (bps)
    pub reason_code: u8,        // Reason: 1=strong_momentum, 2=low_resistance
    pub confidence: u8,         // 0-100
    pub _padding: [u8; 8],
}

impl WidenExitAdvice {
    pub const SIZE: usize = 48;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            current_tp_bps: u16::from_le_bytes([bytes[33], bytes[34]]),
            suggested_tp_bps: u16::from_le_bytes([bytes[35], bytes[36]]),
            reason_code: bytes[37],
            confidence: bytes[38],
            _padding: [0u8; 8],
        })
    }
}

/// LateOpportunity advice - mature launch opportunity
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct LateOpportunityAdvice {
    pub msg_type: u8,           // 12
    pub mint: [u8; 32],         // Token mint
    pub age_seconds: u64,       // Time since launch
    pub vol_60s_sol: f32,       // Volume last 60s
    pub buyers_60s: u32,        // Buyers last 60s
    pub follow_through_score: u8, // Computed score
    pub _padding: [u8; 6],
}

impl LateOpportunityAdvice {
    pub const SIZE: usize = 56;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            age_seconds: u64::from_le_bytes([
                bytes[33], bytes[34], bytes[35], bytes[36],
                bytes[37], bytes[38], bytes[39], bytes[40]
            ]),
            vol_60s_sol: f32::from_le_bytes([bytes[41], bytes[42], bytes[43], bytes[44]]),
            buyers_60s: u32::from_le_bytes([bytes[45], bytes[46], bytes[47], bytes[48]]),
            follow_through_score: bytes[49],
            _padding: [0u8; 6],
        })
    }
}

/// CopyTrade advice - follow a profitable wallet
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct CopyTradeAdvice {
    pub msg_type: u8,           // 13
    pub wallet: [u8; 32],       // Wallet address
    pub mint: [u8; 32],         // Token mint
    pub side: u8,               // 0=BUY, 1=SELL
    pub size_sol: f32,          // Trade size in SOL
    pub wallet_tier: u8,        // Wallet tier (0=Discovery, 1=C, 2=B, 3=A)
    pub wallet_confidence: u8,  // 0-100
    pub _padding: [u8; 6],
}

impl CopyTradeAdvice {
    pub const SIZE: usize = 80;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut wallet = [0u8; 32];
        wallet.copy_from_slice(&bytes[1..33]);
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[33..65]);
        
        Some(Self {
            msg_type: bytes[0],
            wallet,
            mint,
            side: bytes[65],
            size_sol: f32::from_le_bytes([bytes[66], bytes[67], bytes[68], bytes[69]]),
            wallet_tier: bytes[70],
            wallet_confidence: bytes[71],
            _padding: [0u8; 6],
        })
    }
}

/// SolPriceUpdate - SOL price update for position sizing
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct SolPriceUpdate {
    pub msg_type: u8,           // 14
    pub price_usd: f32,         // SOL price in USD
    pub timestamp: u64,         // Unix timestamp
    pub source: u8,             // Price source: 1=Pyth, 2=Jupiter, 3=Birdeye
    pub _padding: [u8; 18],
}

impl SolPriceUpdate {
    pub const SIZE: usize = 32;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        Some(Self {
            msg_type: bytes[0],
            price_usd: f32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]),
            timestamp: u64::from_le_bytes([
                bytes[5], bytes[6], bytes[7], bytes[8],
                bytes[9], bytes[10], bytes[11], bytes[12]
            ]),
            source: bytes[13],
            _padding: [0u8; 18],
        })
    }
}

/// Unified advice message enum
#[derive(Debug, Clone)]
pub enum AdviceMessage {
    ExtendHold(ExtendHoldAdvice),
    WidenExit(WidenExitAdvice),
    LateOpportunity(LateOpportunityAdvice),
    CopyTrade(CopyTradeAdvice),
    SolPriceUpdate(SolPriceUpdate),
}

impl AdviceMessage {
    /// Parse advice message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        
        let msg_type = AdviceMessageType::from_u8(bytes[0])?;
        
        match msg_type {
            AdviceMessageType::ExtendHold => {
                ExtendHoldAdvice::from_bytes(bytes).map(Self::ExtendHold)
            }
            AdviceMessageType::WidenExit => {
                WidenExitAdvice::from_bytes(bytes).map(Self::WidenExit)
            }
            AdviceMessageType::LateOpportunity => {
                LateOpportunityAdvice::from_bytes(bytes).map(Self::LateOpportunity)
            }
            AdviceMessageType::CopyTrade => {
                CopyTradeAdvice::from_bytes(bytes).map(Self::CopyTrade)
            }
            AdviceMessageType::SolPriceUpdate => {
                SolPriceUpdate::from_bytes(bytes).map(Self::SolPriceUpdate)
            }
        }
    }
}

#[cfg(test)]
mod advice_tests {
    use super::*;
    
    #[test]
    fn test_advice_message_type_conversion() {
        assert_eq!(AdviceMessageType::from_u8(10), Some(AdviceMessageType::ExtendHold));
        assert_eq!(AdviceMessageType::from_u8(13), Some(AdviceMessageType::CopyTrade));
        assert_eq!(AdviceMessageType::from_u8(99), None);
    }
    
    #[test]
    fn test_extend_hold_size() {
        assert_eq!(ExtendHoldAdvice::SIZE, 48);
    }
    
    #[test]
    fn test_copy_trade_size() {
        assert_eq!(CopyTradeAdvice::SIZE, 80);
    }
    
    #[test]
    fn test_sol_price_update_size() {
        assert_eq!(SolPriceUpdate::SIZE, 32);
    }
}
