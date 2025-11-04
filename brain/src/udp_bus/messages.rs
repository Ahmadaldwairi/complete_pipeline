//! 📡 UDP Message Definitions for Brain ↔ Executor Communication
//! 
//! Fixed-size binary packets optimized for localhost UDP transmission.
//! All structs are #[repr(C)] for predictable memory layout and zero-copy serialization.

use anyhow::Result;
use crate::udp_bus::PositionUpdate;

/// 📦 TradeDecision - Brain → Executor (Port 45110)
/// 
/// 52-byte packet containing a validated trade decision ready for immediate execution.
/// The executor receives this and builds+sends the transaction without additional logic.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TradeDecision {
    /// Message type identifier (1 = TRADE_DECISION)
    pub msg_type: u8,
    
    /// Protocol version (currently 1)
    pub protocol_version: u8,
    
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
    
    /// Simple checksum: XOR of all data bytes (for data integrity validation)
    pub checksum: u8,
    
    /// Retry count for progressive slippage widening (0 = first attempt, max 3)
    pub retry_count: u8,
    
    /// Entry strategy type: 0=Rank, 1=Momentum, 2=CopyTrade, 3=LateOpportunity
    pub entry_type: u8,
    
    /// Padding to align to 52 bytes
    pub _padding: [u8; 1],
}

impl TradeDecision {
    /// Total packet size in bytes
    pub const SIZE: usize = 52;
    
    /// Message type constant
    pub const MSG_TYPE: u8 = 1;
    
    /// Current protocol version
    pub const PROTOCOL_VERSION: u8 = 1;
    
    /// Calculate checksum for data integrity (XOR of all data bytes)
    fn calculate_checksum(msg_type: u8, protocol_version: u8, mint: &[u8; 32], side: u8, 
                          size_lamports: u64, slippage_bps: u16, confidence: u8, retry_count: u8, entry_type: u8) -> u8 {
        let mut checksum = msg_type ^ protocol_version ^ side ^ confidence ^ retry_count ^ entry_type;
        for &byte in mint {
            checksum ^= byte;
        }
        for &byte in &size_lamports.to_le_bytes() {
            checksum ^= byte;
        }
        for &byte in &slippage_bps.to_le_bytes() {
            checksum ^= byte;
        }
        checksum
    }
    
    /// Verify checksum matches expected value
    pub fn verify_checksum(&self) -> bool {
        let expected = Self::calculate_checksum(
            self.msg_type,
            self.protocol_version,
            &self.mint,
            self.side,
            self.size_lamports,
            self.slippage_bps,
            self.confidence,
            self.retry_count,
            self.entry_type,
        );
        self.checksum == expected
    }
    
    /// Comprehensive validation for executor v1 compatibility
    /// 
    /// Validates all fields, protocol version, and data integrity.
    pub fn validate_v1_format(&self) -> Result<()> {
        // Protocol version check
        if self.protocol_version != Self::PROTOCOL_VERSION {
            anyhow::bail!(
                "Invalid protocol version: {} (expected {})",
                self.protocol_version,
                Self::PROTOCOL_VERSION
            );
        }
        
        // Message type check
        if self.msg_type != Self::MSG_TYPE {
            anyhow::bail!(
                "Invalid message type: {} (expected {})",
                self.msg_type,
                Self::MSG_TYPE
            );
        }
        
        // Trade side validation (0=BUY, 1=SELL)
        if self.side > 1 {
            anyhow::bail!("Invalid trade side: {} (must be 0 or 1)", self.side);
        }
        
        // Size validation
        if self.size_lamports == 0 {
            anyhow::bail!("Invalid trade size: 0 lamports");
        }
        
        // Slippage validation (max 100% = 10000 bps)
        if self.slippage_bps > 10000 {
            anyhow::bail!("Invalid slippage: {}bps (max 10000)", self.slippage_bps);
        }
        
        // Confidence validation (0-100)
        if self.confidence > 100 {
            anyhow::bail!("Invalid confidence: {} (max 100)", self.confidence);
        }
        
        // Checksum validation
        if !self.verify_checksum() {
            anyhow::bail!("Checksum validation failed - message corrupted");
        }
        
        Ok(())
    }
    
    /// Create a new BUY decision
    pub fn new_buy(mint: [u8; 32], size_lamports: u64, slippage_bps: u16, confidence: u8, entry_type: u8) -> Self {
        let retry_count = 0; // BUYs don't use retry logic
        let checksum = Self::calculate_checksum(
            Self::MSG_TYPE,
            Self::PROTOCOL_VERSION,
            &mint,
            0, // BUY
            size_lamports,
            slippage_bps,
            confidence,
            retry_count,
            entry_type,
        );
        
        Self {
            msg_type: Self::MSG_TYPE,
            protocol_version: Self::PROTOCOL_VERSION,
            mint,
            side: 0, // BUY
            size_lamports,
            slippage_bps,
            confidence,
            checksum,
            retry_count,
            entry_type,
            _padding: [0; 1],
        }
    }
    
    /// Create a new SELL decision with retry count for progressive slippage
    pub fn new_sell(mint: [u8; 32], size_lamports: u64, slippage_bps: u16, confidence: u8, retry_count: u8, entry_type: u8) -> Self {
        let checksum = Self::calculate_checksum(
            Self::MSG_TYPE,
            Self::PROTOCOL_VERSION,
            &mint,
            1, // SELL
            size_lamports,
            slippage_bps,
            confidence,
            retry_count,
            entry_type,
        );
        
        Self {
            msg_type: Self::MSG_TYPE,
            protocol_version: Self::PROTOCOL_VERSION,
            mint,
            side: 1, // SELL
            size_lamports,
            slippage_bps,
            confidence,
            checksum,
            retry_count,
            entry_type,
            _padding: [0; 1],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = self.msg_type;
        buf[1] = self.protocol_version;
        buf[2..34].copy_from_slice(&self.mint);
        buf[34] = self.side;
        buf[35..43].copy_from_slice(&self.size_lamports.to_le_bytes());
        buf[43..45].copy_from_slice(&self.slippage_bps.to_le_bytes());
        buf[45] = self.confidence;
        buf[46] = self.checksum;
        buf[47] = self.retry_count;
        buf[48] = self.entry_type;
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
        
        let protocol_version = buf[1];
        if protocol_version != Self::PROTOCOL_VERSION {
            anyhow::bail!("Unsupported protocol version: {}, expected {}", protocol_version, Self::PROTOCOL_VERSION);
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&buf[2..34]);
        
        let side = buf[34];
        let size_lamports = u64::from_le_bytes(buf[35..43].try_into()?);
        let slippage_bps = u16::from_le_bytes(buf[43..45].try_into()?);
        let confidence = buf[45];
        let checksum = buf[46];
        let retry_count = buf[47];
        let entry_type = buf[48];
        
        let decision = Self {
            msg_type: Self::MSG_TYPE,
            protocol_version,
            mint,
            side,
            size_lamports,
            slippage_bps,
            confidence,
            checksum,
            retry_count,
            entry_type,
            _padding: [0; 1],
        };
        
        // Verify checksum for data integrity
        if !decision.verify_checksum() {
            anyhow::bail!("Checksum mismatch: packet may be corrupted");
        }
        
        Ok(decision)
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

/// ✅ ExecutionConfirmation - Executor → Brain (Port 45115)
/// 
/// 64-byte packet confirming a trade was successfully executed.
/// Brain uses this to add/remove positions from tracker only after actual execution.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ExecutionConfirmation {
    /// Message type identifier (2 = EXECUTION_CONFIRMATION)
    pub msg_type: u8,
    
    /// Protocol version (currently 1)
    pub protocol_version: u8,
    
    /// Token mint address (32 bytes, Solana Pubkey)
    pub mint: [u8; 32],
    
    /// Trade side: 0 = BUY, 1 = SELL
    pub side: u8,
    
    /// Actual executed size in lamports
    pub executed_size_lamports: u64,
    
    /// Actual executed price (SOL per token, scaled by 1e9)
    pub executed_price_scaled: u64,
    
    /// Transaction signature (first 32 bytes for tracking)
    pub tx_signature: [u8; 32],
    
    /// Unix timestamp of execution
    pub timestamp: u64,
    
    /// Success flag: 1 = success, 0 = failed
    pub success: u8,
    
    /// Padding to align to 128 bytes
    pub _padding: [u8; 7],
}

impl ExecutionConfirmation {
    /// Total packet size in bytes
    pub const SIZE: usize = 128;
    
    /// Message type constant
    pub const MSG_TYPE: u8 = 2;
    
    /// Current protocol version
    pub const PROTOCOL_VERSION: u8 = 1;
    
    /// Create a new execution confirmation for successful trade
    pub fn new_success(
        mint: [u8; 32],
        side: u8,
        executed_size_lamports: u64,
        executed_price_sol: f64,
        tx_signature: [u8; 32],
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            msg_type: Self::MSG_TYPE,
            protocol_version: Self::PROTOCOL_VERSION,
            mint,
            side,
            executed_size_lamports,
            executed_price_scaled: (executed_price_sol * 1e9) as u64,
            tx_signature,
            timestamp,
            success: 1,
            _padding: [0; 7],
        }
    }
    
    /// Create a new execution confirmation for failed trade
    pub fn new_failure(mint: [u8; 32], side: u8) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            msg_type: Self::MSG_TYPE,
            protocol_version: Self::PROTOCOL_VERSION,
            mint,
            side,
            executed_size_lamports: 0,
            executed_price_scaled: 0,
            tx_signature: [0; 32],
            timestamp,
            success: 0,
            _padding: [0; 7],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = self.msg_type;
        buf[1] = self.protocol_version;
        buf[2..34].copy_from_slice(&self.mint);
        buf[34] = self.side;
        buf[35..43].copy_from_slice(&self.executed_size_lamports.to_le_bytes());
        buf[43..51].copy_from_slice(&self.executed_price_scaled.to_le_bytes());
        buf[51..83].copy_from_slice(&self.tx_signature);
        buf[83..91].copy_from_slice(&self.timestamp.to_le_bytes());
        buf[91] = self.success;
        buf
    }
    
    /// Deserialize from UDP packet bytes
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < Self::SIZE {
            anyhow::bail!("ExecutionConfirmation packet too short: {} bytes", buf.len());
        }
        
        if buf[0] != Self::MSG_TYPE {
            anyhow::bail!("Invalid message type: expected {}, got {}", Self::MSG_TYPE, buf[0]);
        }
        
        let protocol_version = buf[1];
        if protocol_version != Self::PROTOCOL_VERSION {
            anyhow::bail!("Unsupported protocol version: {}, expected {}", protocol_version, Self::PROTOCOL_VERSION);
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&buf[2..34]);
        
        let side = buf[34];
        let executed_size_lamports = u64::from_le_bytes(buf[35..43].try_into()?);
        let executed_price_scaled = u64::from_le_bytes(buf[43..51].try_into()?);
        
        let mut tx_signature = [0u8; 32];
        tx_signature.copy_from_slice(&buf[51..83]);
        
        let timestamp = u64::from_le_bytes(buf[83..91].try_into()?);
        let success = buf[91];
        
        Ok(Self {
            msg_type: Self::MSG_TYPE,
            protocol_version,
            mint,
            side,
            executed_size_lamports,
            executed_price_scaled,
            tx_signature,
            timestamp,
            success,
            _padding: [0; 7],
        })
    }
    
    /// Check if this is a BUY confirmation
    pub fn is_buy(&self) -> bool {
        self.side == 0
    }
    
    /// Check if this is a SELL confirmation
    pub fn is_sell(&self) -> bool {
        self.side == 1
    }
    
    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        self.success == 1
    }
    
    /// Get executed price in SOL per token
    pub fn executed_price_sol(&self) -> f64 {
        self.executed_price_scaled as f64 / 1e9
    }
    
    /// Get executed size in SOL
    pub fn executed_size_sol(&self) -> f64 {
        self.executed_size_lamports as f64 / 1_000_000_000.0
    }
    
    /// Get mint address as bs58 string
    pub fn mint_bs58(&self) -> String {
        bs58::encode(&self.mint).into_string()
    }
    
    /// Get transaction signature as bs58 string (shortened for logging)
    pub fn tx_signature_short(&self) -> String {
        let sig_bs58 = bs58::encode(&self.tx_signature).into_string();
        if sig_bs58.len() > 12 {
            sig_bs58[..12].to_string()
        } else {
            sig_bs58
        }
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
        let decision = TradeDecision::new_buy(mint, 1_000_000_000, 150, 95, 2); // entry_type=2 (CopyTrade)
        
        let bytes = decision.to_bytes();
        assert_eq!(bytes.len(), TradeDecision::SIZE);
        
        let decoded = TradeDecision::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.mint, mint);
        assert_eq!(decoded.side, 0);
        assert_eq!(decoded.size_lamports, 1_000_000_000);
        assert_eq!(decoded.slippage_bps, 150);
        assert_eq!(decoded.confidence, 95);
        assert_eq!(decoded.entry_type, 2);
        assert!(decoded.is_buy());
    }
    
    #[test]
    fn test_execution_confirmation_serialization() {
        let mint = [2u8; 32];
        let tx_sig = [3u8; 32];
        let confirmation = ExecutionConfirmation::new_success(
            mint,
            0, // BUY
            1_000_000_000,
            0.00001, // Price in SOL
            tx_sig,
        );
        
        assert_eq!(confirmation.msg_type, ExecutionConfirmation::MSG_TYPE);
        assert!(confirmation.is_buy());
        assert!(confirmation.is_success());
        assert_eq!(confirmation.executed_size_sol(), 1.0);
        
        let bytes = confirmation.to_bytes();
        assert_eq!(bytes.len(), ExecutionConfirmation::SIZE);
        
        let decoded = ExecutionConfirmation::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.mint, mint);
        assert_eq!(decoded.side, 0);
        assert_eq!(decoded.executed_size_lamports, 1_000_000_000);
        assert!(decoded.is_success());
    }
    
    #[test]
    fn test_execution_confirmation_failure() {
        let mint = [4u8; 32];
        let confirmation = ExecutionConfirmation::new_failure(mint, 1); // SELL failure
        
        assert!(confirmation.is_sell());
        assert!(!confirmation.is_success());
        assert_eq!(confirmation.executed_size_lamports, 0);
    }
    
    #[test]
    fn test_v1_format_validation() {
        let mint = [1u8; 32];
        
        // Valid decision should pass
        let valid_decision = TradeDecision::new_buy(mint, 1_000_000_000, 150, 95);
        assert!(valid_decision.validate_v1_format().is_ok(), "Valid decision should pass");
        
        // Test invalid protocol version
        let mut invalid_decision = valid_decision;
        invalid_decision.protocol_version = 99;
        assert!(invalid_decision.validate_v1_format().is_err(), "Invalid protocol version should fail");
        
        // Test invalid message type
        let mut invalid_decision = valid_decision;
        invalid_decision.msg_type = 99;
        assert!(invalid_decision.validate_v1_format().is_err(), "Invalid message type should fail");
        
        // Test invalid trade side
        let mut invalid_decision = valid_decision;
        invalid_decision.side = 99;
        assert!(invalid_decision.validate_v1_format().is_err(), "Invalid trade side should fail");
        
        // Test zero size
        let mut invalid_decision = valid_decision;
        invalid_decision.size_lamports = 0;
        assert!(invalid_decision.validate_v1_format().is_err(), "Zero size should fail");
        
        // Test excessive slippage
        let mut invalid_decision = valid_decision;
        invalid_decision.slippage_bps = 20000;
        assert!(invalid_decision.validate_v1_format().is_err(), "Excessive slippage should fail");
        
        // Test invalid confidence
        let mut invalid_decision = valid_decision;
        invalid_decision.confidence = 150;
        assert!(invalid_decision.validate_v1_format().is_err(), "Invalid confidence should fail");
        
        // Test checksum corruption
        let mut invalid_decision = valid_decision;
        invalid_decision.checksum = 0;
        assert!(invalid_decision.validate_v1_format().is_err(), "Corrupted checksum should fail");
    }
    
    #[test]
    fn test_checksum_validation() {
        let mint = [1u8; 32];
        let decision = TradeDecision::new_buy(mint, 1_000_000_000, 150, 95);
        
        // Valid checksum should pass
        assert!(decision.verify_checksum(), "Valid checksum should pass");
        
        // Modified checksum should fail
        let mut corrupted = decision;
        corrupted.checksum = !decision.checksum;
        assert!(!corrupted.verify_checksum(), "Corrupted checksum should fail");
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
    LateOpportunity = 12,   // Mature launch opportunity (Path D)
    CopyTrade = 13,         // Copy a profitable wallet (Path C)
    SolPriceUpdate = 14,    // SOL price update
    RankOpportunity = 15,   // Top-ranked new launch (Path A)
    MomentumOpportunity = 16, // High momentum token (Path B)
    MempoolHeat = 17,       // Mempool heat index from watcher
    TradeSubmitted = 18,    // Transaction submitted (not yet confirmed)
    TradeConfirmed = 19,    // Transaction confirmed on-chain
    TradeFailed = 20,       // Transaction failed or timed out
    MomentumDetected = 21,  // Confirmed tx momentum detected (from data-mining)
    VolumeSpike = 22,       // Volume spike detected (from data-mining)
    WalletActivity = 23,    // Alpha wallet activity detected (from data-mining)
    ExitAck = 24,           // ✅ Executor acknowledges SELL command received (prevents spam)
    TxConfirmed = 26,       // ✅ Mempool-watcher confirms transaction on-chain (SOURCE OF TRUTH)
    EnterAck = 27,          // ✅ NEW: Executor acknowledges BUY command received (provides feedback)
    TradeClosed = 28,       // ✅ Executor signals trade fully finalized (audit trail)
    WindowMetrics = 29,     // ✅ Real-time market metrics (volume, buyers, price change, alpha activity)
    PositionUpdate = 32,    // ✅ NEW: Mempool-watcher sends real-time P&L updates
}

impl AdviceMessageType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            10 => Some(Self::ExtendHold),
            11 => Some(Self::WidenExit),
            12 => Some(Self::LateOpportunity),
            13 => Some(Self::CopyTrade),
            14 => Some(Self::SolPriceUpdate),
            15 => Some(Self::RankOpportunity),
            16 => Some(Self::MomentumOpportunity),
            17 => Some(Self::MempoolHeat),
            18 => Some(Self::TradeSubmitted),
            19 => Some(Self::TradeConfirmed),
            20 => Some(Self::TradeFailed),
            21 => Some(Self::MomentumDetected),
            22 => Some(Self::VolumeSpike),
            23 => Some(Self::WalletActivity),
            24 => Some(Self::ExitAck),
            26 => Some(Self::TxConfirmed),
            27 => Some(Self::EnterAck),
            28 => Some(Self::TradeClosed),
            29 => Some(Self::WindowMetrics),
            32 => Some(Self::PositionUpdate),
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

/// MomentumOpportunity advice - explosive short-term activity (Path B)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct MomentumOpportunityAdvice {
    pub msg_type: u8,           // 16
    pub mint: [u8; 32],         // Token mint
    pub vol_5s_scaled: u16,     // 5s volume * 100 (e.g., 250 = 2.50 SOL)
    pub buyers_2s: u16,         // Unique buyers in 2s
    pub score: u8,              // Momentum score 0-100
    pub _padding: [u8; 12],
}

impl MomentumOpportunityAdvice {
    pub const SIZE: usize = 64;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            vol_5s_scaled: u16::from_le_bytes([bytes[33], bytes[34]]),
            buyers_2s: u16::from_le_bytes([bytes[35], bytes[36]]),
            score: bytes[37],
            _padding: [0u8; 12],
        })
    }
    
    /// Get actual 5s volume in SOL (scaled back from u16)
    pub fn vol_5s_sol(&self) -> f64 {
        self.vol_5s_scaled as f64 / 100.0
    }
}

/// RankOpportunity advice - top-ranked new launch (Path A)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct RankOpportunityAdvice {
    pub msg_type: u8,           // 15
    pub mint: [u8; 32],         // Token mint
    pub rank: u8,               // Rank position (1-10, 1 = best)
    pub score: u8,              // Overall quality score 0-100
    pub _padding: [u8; 29],
}

impl RankOpportunityAdvice {
    pub const SIZE: usize = 64;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            rank: bytes[33],
            score: bytes[34],
            _padding: [0u8; 29],
        })
    }
}

/// Mempool heat index from mempool-watcher
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct MempoolHeatAdvice {
    pub msg_type: u8,           // 17
    pub heat_score: u8,         // Overall heat score (0-100)
    pub tx_rate: u16,           // Transactions per second * 100 (to fit in u16)
    pub whale_activity: u16,    // Whale SOL activity * 100
    pub bot_density: u16,       // Bot density * 10000
    pub timestamp: u64,         // Unix timestamp
    pub _padding: [u8; 6],
}

impl MempoolHeatAdvice {
    pub const SIZE: usize = 24;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        Some(Self {
            msg_type: bytes[0],
            heat_score: bytes[1],
            tx_rate: u16::from_le_bytes([bytes[2], bytes[3]]),
            whale_activity: u16::from_le_bytes([bytes[4], bytes[5]]),
            bot_density: u16::from_le_bytes([bytes[6], bytes[7]]),
            timestamp: u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], 
                                           bytes[12], bytes[13], bytes[14], bytes[15]]),
            _padding: [0u8; 6],
        })
    }
}

/// TradeSubmitted - Executor → Brain (transaction submitted, not yet confirmed)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct TradeSubmittedAdvice {
    pub msg_type: u8,              // 18
    pub mint: [u8; 32],            // Token mint
    pub signature: [u8; 64],       // Transaction signature
    pub side: u8,                  // 0=BUY, 1=SELL
    pub submitted_ts_ns: u64,      // Timestamp (nanoseconds)
    pub expected_tokens: u64,      // Expected token amount (raw)
    pub expected_sol_lamports: u64, // Expected SOL amount
    pub expected_slip_bps: u16,    // Expected slippage (basis points)
    pub submitted_via: u8,         // 0=TPU, 1=RPC
    pub _padding: [u8; 5],
}

impl TradeSubmittedAdvice {
    pub const SIZE: usize = 192;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[33..97]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            signature,
            side: bytes[97],
            submitted_ts_ns: u64::from_le_bytes([bytes[98], bytes[99], bytes[100], bytes[101],
                                                  bytes[102], bytes[103], bytes[104], bytes[105]]),
            expected_tokens: u64::from_le_bytes([bytes[106], bytes[107], bytes[108], bytes[109],
                                                  bytes[110], bytes[111], bytes[112], bytes[113]]),
            expected_sol_lamports: u64::from_le_bytes([bytes[114], bytes[115], bytes[116], bytes[117],
                                                        bytes[118], bytes[119], bytes[120], bytes[121]]),
            expected_slip_bps: u16::from_le_bytes([bytes[122], bytes[123]]),
            submitted_via: bytes[124],
            _padding: [0u8; 5],
        })
    }
}

/// TradeConfirmed - Executor → Brain (transaction confirmed on-chain)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct TradeConfirmedAdvice {
    pub msg_type: u8,              // 19
    pub mint: [u8; 32],            // Token mint
    pub signature: [u8; 64],       // Transaction signature
    pub side: u8,                  // 0=BUY, 1=SELL
    pub confirmed_ts_ns: u64,      // Timestamp (nanoseconds)
    pub actual_tokens: u64,        // Actual token amount received
    pub actual_sol_lamports: u64,  // Actual SOL amount
    pub total_fees_lamports: u64,  // Total fees paid
    pub compute_units_used: u32,   // Compute units consumed
    pub fast_confirm: u8,          // 1=mempool-based fast confirm, 0=finalized
    pub tx_status: u8,             // 0=confirmed, 1=finalized
    pub _padding: [u8; 6],
}

impl TradeConfirmedAdvice {
    pub const SIZE: usize = 208;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[33..97]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            signature,
            side: bytes[97],
            confirmed_ts_ns: u64::from_le_bytes([bytes[98], bytes[99], bytes[100], bytes[101],
                                                  bytes[102], bytes[103], bytes[104], bytes[105]]),
            actual_tokens: u64::from_le_bytes([bytes[106], bytes[107], bytes[108], bytes[109],
                                                bytes[110], bytes[111], bytes[112], bytes[113]]),
            actual_sol_lamports: u64::from_le_bytes([bytes[114], bytes[115], bytes[116], bytes[117],
                                                      bytes[118], bytes[119], bytes[120], bytes[121]]),
            total_fees_lamports: u64::from_le_bytes([bytes[122], bytes[123], bytes[124], bytes[125],
                                                      bytes[126], bytes[127], bytes[128], bytes[129]]),
            compute_units_used: u32::from_le_bytes([bytes[130], bytes[131], bytes[132], bytes[133]]),
            fast_confirm: bytes[134],
            tx_status: bytes[135],
            _padding: [0u8; 6],
        })
    }
}

/// TradeFailed - Executor → Brain (transaction failed or timed out)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct TradeFailedAdvice {
    pub msg_type: u8,              // 20
    pub mint: [u8; 32],            // Token mint
    pub signature: [u8; 64],       // Transaction signature (if known)
    pub side: u8,                  // 0=BUY, 1=SELL
    pub failed_ts_ns: u64,         // Timestamp (nanoseconds)
    pub reason_code: u8,           // 1=timeout, 2=slippage, 3=instruction_error, 4=blockhash, 5=other
    pub has_signature: u8,         // 1=signature available, 0=failed before submission
    pub reason_str: [u8; 64],      // Human-readable reason (UTF-8, null-terminated)
    pub _padding: [u8; 6],
}

impl TradeFailedAdvice {
    pub const SIZE: usize = 176;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&bytes[33..97]);
        
        let mut reason_str = [0u8; 64];
        reason_str.copy_from_slice(&bytes[108..172]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            signature,
            side: bytes[97],
            failed_ts_ns: u64::from_le_bytes([bytes[98], bytes[99], bytes[100], bytes[101],
                                               bytes[102], bytes[103], bytes[104], bytes[105]]),
            reason_code: bytes[106],
            has_signature: bytes[107],
            reason_str,
            _padding: [0u8; 6],
        })
    }
    
    /// Get reason string as UTF-8 (stops at null terminator)
    pub fn reason_string(&self) -> String {
        let null_pos = self.reason_str.iter().position(|&b| b == 0).unwrap_or(64);
        String::from_utf8_lossy(&self.reason_str[..null_pos]).to_string()
    }
}

/// MomentumDetected - Data-Mining → Brain (confirmed tx momentum detected)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct MomentumDetectedAdvice {
    pub msg_type: u8,              // 21
    pub mint: [u8; 32],            // Token mint
    pub buys_in_last_500ms: u16,   // Number of buy transactions in last 500ms
    pub volume_sol: f32,           // SOL volume in the window
    pub unique_buyers: u16,        // Number of unique buyers
    pub confidence: u8,            // Confidence score 0-100
    pub timestamp_ns: u64,         // Timestamp (nanoseconds)
    pub _padding: [u8; 7],         // Padding to align to 64 bytes
}

impl MomentumDetectedAdvice {
    pub const SIZE: usize = 64;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            buys_in_last_500ms: u16::from_le_bytes([bytes[33], bytes[34]]),
            volume_sol: f32::from_le_bytes([bytes[35], bytes[36], bytes[37], bytes[38]]),
            unique_buyers: u16::from_le_bytes([bytes[39], bytes[40]]),
            confidence: bytes[41],
            timestamp_ns: u64::from_le_bytes([bytes[42], bytes[43], bytes[44], bytes[45],
                                               bytes[46], bytes[47], bytes[48], bytes[49]]),
            _padding: [0u8; 7],
        })
    }
}

/// VolumeSpike - Data-Mining → Brain (volume spike detected)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct VolumeSpikeAdvice {
    pub msg_type: u8,              // 22
    pub mint: [u8; 32],            // Token mint
    pub total_sol: f32,            // Total SOL volume in spike
    pub tx_count: u16,             // Number of transactions
    pub time_window_ms: u16,       // Time window in milliseconds
    pub confidence: u8,            // Confidence score 0-100
    pub timestamp_ns: u64,         // Timestamp (nanoseconds)
    pub _padding: [u8; 11],        // Padding to align to 64 bytes
}

impl VolumeSpikeAdvice {
    pub const SIZE: usize = 64;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            total_sol: f32::from_le_bytes([bytes[33], bytes[34], bytes[35], bytes[36]]),
            tx_count: u16::from_le_bytes([bytes[37], bytes[38]]),
            time_window_ms: u16::from_le_bytes([bytes[39], bytes[40]]),
            confidence: bytes[41],
            timestamp_ns: u64::from_le_bytes([bytes[42], bytes[43], bytes[44], bytes[45],
                                               bytes[46], bytes[47], bytes[48], bytes[49]]),
            _padding: [0u8; 11],
        })
    }
}

/// WalletActivity - Data-Mining → Brain (alpha wallet activity detected)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct WalletActivityAdvice {
    pub msg_type: u8,              // 23
    pub mint: [u8; 32],            // Token mint
    pub wallet: [u8; 32],          // Alpha wallet address
    pub action: u8,                // 0=buy, 1=sell
    pub size_sol: f32,             // Trade size in SOL
    pub wallet_tier: u8,           // Wallet tier (0=Discovery, 1=C, 2=B, 3=A)
    pub confidence: u8,            // Confidence score 0-100
    pub timestamp_ns: u64,         // Timestamp (nanoseconds)
    pub _padding: [u8; 12],        // Padding to align to 80 bytes
}

impl WalletActivityAdvice {
    pub const SIZE: usize = 80;
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        let mut wallet = [0u8; 32];
        wallet.copy_from_slice(&bytes[33..65]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            wallet,
            action: bytes[65],
            size_sol: f32::from_le_bytes([bytes[66], bytes[67], bytes[68], bytes[69]]),
            wallet_tier: bytes[70],
            confidence: bytes[71],
            timestamp_ns: u64::from_le_bytes([bytes[72], bytes[73], bytes[74], bytes[75],
                                               bytes[76], bytes[77], bytes[78], bytes[79]]),
            _padding: [0u8; 12],
        })
    }
}

/// ✅ NEW: ExitAck - Executor → Brain (acknowledges SELL command received)
/// 
/// Sent immediately when Executor receives a SELL TradeDecision, BEFORE building the tx.
/// This breaks the infinite SELL loop by telling Brain to stop resending.
/// 
/// Brain behavior after receiving ExitAck:
/// - Set position state to CLOSING
/// - Start 10-20s timeout timer
/// - Stop resending SELL signals for this trade_id
/// - Wait for ExitResult (success/failure) or timeout
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct ExitAck {
    pub msg_type: u8,              // 24
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // UUID of trade (first 16 bytes)
    pub timestamp_ns: u64,         // When ack was sent (nanoseconds)
    pub _padding: [u8; 7],         // Padding to align to 64 bytes
}

impl ExitAck {
    pub const SIZE: usize = 64;
    pub const MSG_TYPE: u8 = 24;
    
    /// Create new ExitAck from mint and trade_id
    pub fn new(mint: [u8; 32], trade_id: &str) -> Self {
        let mut trade_id_bytes = [0u8; 16];
        // Take first 16 bytes of UUID (sufficient for uniqueness)
        let uuid_bytes = trade_id.as_bytes();
        let copy_len = uuid_bytes.len().min(16);
        trade_id_bytes[..copy_len].copy_from_slice(&uuid_bytes[..copy_len]);
        
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            trade_id: trade_id_bytes,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 7],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.mint);
        bytes.extend_from_slice(&self.trade_id);
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        bytes
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&bytes[33..49]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            trade_id,
            timestamp_ns: u64::from_le_bytes([bytes[49], bytes[50], bytes[51], bytes[52],
                                               bytes[53], bytes[54], bytes[55], bytes[56]]),
            _padding: [0u8; 7],
        })
    }
}

/// ✅ EnterAck - Executor → Brain (Port 45115)
/// 
/// Sent immediately when Executor receives a BUY TradeDecision, BEFORE building the tx.
/// Provides feedback to Brain that BUY was accepted and is being processed.
/// Mirrors ExitAck for consistency.
/// 
/// Brain behavior after receiving EnterAck:
/// - Confirm that Executor received BUY command
/// - Optional: Log acknowledgment for monitoring
/// - Continue waiting for TxConfirmed (on-chain confirmation)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct EnterAck {
    pub msg_type: u8,              // 27
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // UUID of trade (first 16 bytes)
    pub timestamp_ns: u64,         // When ack was sent (nanoseconds)
    pub _padding: [u8; 7],         // Padding to align to 64 bytes
}

impl EnterAck {
    pub const SIZE: usize = 64;
    pub const MSG_TYPE: u8 = 27;
    
    /// Create new EnterAck from mint and trade_id
    pub fn new(mint: [u8; 32], trade_id: &str) -> Self {
        let mut trade_id_bytes = [0u8; 16];
        // Take first 16 bytes of UUID (sufficient for uniqueness)
        let uuid_bytes = trade_id.as_bytes();
        let copy_len = uuid_bytes.len().min(16);
        trade_id_bytes[..copy_len].copy_from_slice(&uuid_bytes[..copy_len]);
        
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            trade_id: trade_id_bytes,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 7],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.mint);
        bytes.extend_from_slice(&self.trade_id);
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        bytes
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&bytes[1..33]);
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&bytes[33..49]);
        
        Some(Self {
            msg_type: bytes[0],
            mint,
            trade_id,
            timestamp_ns: u64::from_le_bytes([bytes[49], bytes[50], bytes[51], bytes[52],
                                               bytes[53], bytes[54], bytes[55], bytes[56]]),
            _padding: [0u8; 7],
        })
    }
}

/// ✅ TxConfirmed - Mempool-watcher → Brain (Port 45115)
/// 
/// Sent when mempool-watcher confirms a transaction appeared in the on-chain confirmed stream.
/// This is the SOURCE OF TRUTH for transaction confirmation (not executor's local confirmation).
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct TxConfirmed {
    pub msg_type: u8,              // 26
    pub signature: [u8; 64],       // Transaction signature
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // UUID of trade (first 16 bytes)
    pub side: u8,                  // 0=BUY, 1=SELL
    pub status: u8,                // 0=SUCCESS, 1=FAILED
    pub timestamp_ns: u64,         // When confirmed (nanoseconds)
    pub _padding: [u8; 5],         // Padding to 128 bytes
}

impl TxConfirmed {
    pub const SIZE: usize = 128;
    pub const MSG_TYPE: u8 = 26;
    
    pub const STATUS_SUCCESS: u8 = 0;
    pub const STATUS_FAILED: u8 = 1;
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        
        if data[0] != Self::MSG_TYPE {
            return None;
        }
        
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&data[1..65]);
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[65..97]);
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[97..113]);
        
        let side = data[113];
        let status = data[114];
        
        let timestamp_ns = u64::from_le_bytes([
            data[115], data[116], data[117], data[118],
            data[119], data[120], data[121], data[122]
        ]);
        
        Some(Self {
            msg_type: data[0],
            signature,
            mint,
            trade_id,
            side,
            status,
            timestamp_ns,
            _padding: [0u8; 5],
        })
    }
    
    /// Check if transaction succeeded
    pub fn is_success(&self) -> bool {
        self.status == Self::STATUS_SUCCESS
    }
    
    /// Check if transaction failed
    pub fn is_failure(&self) -> bool {
        self.status == Self::STATUS_FAILED
    }
}

/// ✅ TradeClosed - Executor → Brain (Port 45115)
/// 
/// Sent when a trade reaches its final terminal state (confirmed/failed/timeout).
/// Provides a definitive finalization signal for audit trails and state reconciliation.
/// This is sent AFTER TxConfirmed is processed and all internal state updates are complete.
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct TradeClosed {
    pub msg_type: u8,              // 28
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // UUID of trade (first 16 bytes)
    pub side: u8,                  // 0=BUY, 1=SELL
    pub final_status: u8,          // 0=CONFIRMED, 1=FAILED, 2=TIMEOUT
    pub timestamp_ns: u64,         // When trade was closed (nanoseconds)
    pub _padding: [u8; 6],         // Padding to 64 bytes
}

impl TradeClosed {
    pub const SIZE: usize = 64;
    pub const MSG_TYPE: u8 = 28;
    
    pub const STATUS_CONFIRMED: u8 = 0;
    pub const STATUS_FAILED: u8 = 1;
    pub const STATUS_TIMEOUT: u8 = 2;
    
    /// Create new TradeClosed message
    pub fn new(mint: [u8; 32], trade_id: &str, side: u8, final_status: u8) -> Self {
        let mut trade_id_bytes = [0u8; 16];
        // Take first 16 bytes of UUID (sufficient for uniqueness)
        let uuid_bytes = trade_id.as_bytes();
        let copy_len = uuid_bytes.len().min(16);
        trade_id_bytes[..copy_len].copy_from_slice(&uuid_bytes[..copy_len]);
        
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            trade_id: trade_id_bytes,
            side,
            final_status,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 6],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.mint);
        bytes.extend_from_slice(&self.trade_id);
        bytes.push(self.side);
        bytes.push(self.final_status);
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        bytes
    }
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        
        if data[0] != Self::MSG_TYPE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[1..33]);
        
        let mut trade_id = [0u8; 16];
        trade_id.copy_from_slice(&data[33..49]);
        
        let side = data[49];
        let final_status = data[50];
        
        let timestamp_ns = u64::from_le_bytes([
            data[51], data[52], data[53], data[54],
            data[55], data[56], data[57], data[58]
        ]);
        
        Some(Self {
            msg_type: data[0],
            mint,
            trade_id,
            side,
            final_status,
            timestamp_ns,
            _padding: [0u8; 6],
        })
    }
}

/// ✅ WindowMetrics - Data-mining → Brain (Port 45120)
/// 
/// Real-time sliding window market metrics for intelligent exit timing.
/// Sent when token shows significant activity (3+ trades in 2s window).
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct WindowMetrics {
    pub msg_type: u8,              // 29
    pub mint: [u8; 32],            // Token mint
    pub volume_sol_1s: u32,        // SOL volume last 1s (scaled by 1000, e.g., 1500 = 1.5 SOL)
    pub unique_buyers_1s: u16,     // Unique buyers in last 1s
    pub price_change_bps_2s: i16,  // Price change over 2s in basis points (100 = 1%)
    pub alpha_wallet_hits_10s: u8, // Alpha wallet buys in last 10s
    pub timestamp_ns: u64,         // When metrics calculated (nanoseconds)
    pub _padding: [u8; 13],        // Padding to 64 bytes
}

impl WindowMetrics {
    pub const SIZE: usize = 64;
    pub const MSG_TYPE: u8 = 29;
    
    /// Create new WindowMetrics message
    pub fn new(
        mint: [u8; 32],
        volume_sol_1s: f64,
        unique_buyers_1s: u16,
        price_change_bps_2s: i16,
        alpha_wallet_hits_10s: u8,
    ) -> Self {
        // Scale volume by 1000 to fit in u32 (max ~4.29M SOL)
        let volume_scaled = (volume_sol_1s * 1000.0).min(u32::MAX as f64) as u32;
        
        Self {
            msg_type: Self::MSG_TYPE,
            mint,
            volume_sol_1s: volume_scaled,
            unique_buyers_1s,
            price_change_bps_2s,
            alpha_wallet_hits_10s,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 13],
        }
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.mint);
        bytes.extend_from_slice(&self.volume_sol_1s.to_le_bytes());
        bytes.extend_from_slice(&self.unique_buyers_1s.to_le_bytes());
        bytes.extend_from_slice(&self.price_change_bps_2s.to_le_bytes());
        bytes.push(self.alpha_wallet_hits_10s);
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        bytes
    }
    
    /// Parse from UDP bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        
        if data[0] != Self::MSG_TYPE {
            return None;
        }
        
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&data[1..33]);
        
        let volume_sol_1s = u32::from_le_bytes([data[33], data[34], data[35], data[36]]);
        let unique_buyers_1s = u16::from_le_bytes([data[37], data[38]]);
        let price_change_bps_2s = i16::from_le_bytes([data[39], data[40]]);
        let alpha_wallet_hits_10s = data[41];
        
        let timestamp_ns = u64::from_le_bytes([
            data[42], data[43], data[44], data[45],
            data[46], data[47], data[48], data[49]
        ]);
        
        Some(Self {
            msg_type: data[0],
            mint,
            volume_sol_1s,
            unique_buyers_1s,
            price_change_bps_2s,
            alpha_wallet_hits_10s,
            timestamp_ns,
            _padding: [0u8; 13],
        })
    }
    
    /// Get actual volume in SOL (unscale)
    pub fn volume_sol(&self) -> f64 {
        self.volume_sol_1s as f64 / 1000.0
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
    MomentumOpportunity(MomentumOpportunityAdvice),
    RankOpportunity(RankOpportunityAdvice),
    MempoolHeat(MempoolHeatAdvice),
    TradeSubmitted(TradeSubmittedAdvice),
    TradeConfirmed(TradeConfirmedAdvice),
    TradeFailed(TradeFailedAdvice),
    MomentumDetected(MomentumDetectedAdvice),
    VolumeSpike(VolumeSpikeAdvice),
    WalletActivity(WalletActivityAdvice),
    ExitAck(ExitAck),  // ✅ Executor acknowledges SELL received
    TxConfirmed(TxConfirmed),  // ✅ Mempool-watcher confirms tx on-chain
    EnterAck(EnterAck),  // ✅ NEW: Executor acknowledges BUY received
    TradeClosed(TradeClosed),  // ✅ Executor signals trade finalized
    WindowMetrics(WindowMetrics),  // ✅ Real-time market metrics from data-mining
    PositionUpdate(PositionUpdate),  // ✅ NEW: Mempool-watcher sends real-time P&L updates
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
            AdviceMessageType::MomentumOpportunity => {
                MomentumOpportunityAdvice::from_bytes(bytes).map(Self::MomentumOpportunity)
            }
            AdviceMessageType::RankOpportunity => {
                RankOpportunityAdvice::from_bytes(bytes).map(Self::RankOpportunity)
            }
            AdviceMessageType::MempoolHeat => {
                MempoolHeatAdvice::from_bytes(bytes).map(Self::MempoolHeat)
            }
            AdviceMessageType::TradeSubmitted => {
                TradeSubmittedAdvice::from_bytes(bytes).map(Self::TradeSubmitted)
            }
            AdviceMessageType::TradeConfirmed => {
                TradeConfirmedAdvice::from_bytes(bytes).map(Self::TradeConfirmed)
            }
            AdviceMessageType::TradeFailed => {
                TradeFailedAdvice::from_bytes(bytes).map(Self::TradeFailed)
            }
            AdviceMessageType::MomentumDetected => {
                MomentumDetectedAdvice::from_bytes(bytes).map(Self::MomentumDetected)
            }
            AdviceMessageType::VolumeSpike => {
                VolumeSpikeAdvice::from_bytes(bytes).map(Self::VolumeSpike)
            }
            AdviceMessageType::WalletActivity => {
                WalletActivityAdvice::from_bytes(bytes).map(Self::WalletActivity)
            }
            AdviceMessageType::ExitAck => {
                ExitAck::from_bytes(bytes).map(Self::ExitAck)
            }
            AdviceMessageType::TxConfirmed => {
                TxConfirmed::from_bytes(bytes).map(Self::TxConfirmed)
            }
            AdviceMessageType::EnterAck => {
                EnterAck::from_bytes(bytes).map(Self::EnterAck)
            }
            AdviceMessageType::TradeClosed => {
                TradeClosed::from_bytes(bytes).map(Self::TradeClosed)
            }
            AdviceMessageType::WindowMetrics => {
                WindowMetrics::from_bytes(bytes).map(Self::WindowMetrics)
            }
            AdviceMessageType::PositionUpdate => {
                PositionUpdate::from_bytes(bytes).ok().map(Self::PositionUpdate)
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
