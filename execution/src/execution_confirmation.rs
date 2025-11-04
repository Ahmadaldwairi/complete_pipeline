//! 📡 Execution Confirmation Message - Executor → Brain (Port 45115)
//!
//! Copied from brain/src/udp_bus/messages.rs for executor use.
//! This message confirms trade execution back to the brain for position tracking.

use anyhow::{Result, Context};

/// ✅ ExecutionConfirmation - Executor → Brain (Port 45115)
/// 
/// 128-byte packet confirming a trade was successfully executed.
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
}

/// ✅ NEW: EnterAck - Executor → Brain (Port 45115)
/// 
/// Sent immediately when Executor receives a BUY command, BEFORE building the tx.
/// Provides feedback to Brain that BUY was accepted and is being processed.
/// Mirrors ExitAck for consistency.
/// 
/// Message type 27
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
}

/// ✅ NEW: ExitAck - Executor → Brain (Port 45115)
/// 
/// Sent immediately when Executor receives a SELL command, BEFORE building the tx.
/// This breaks the infinite SELL loop by telling Brain to stop resending.
/// 
/// Message type 24 to match brain's AdviceMessageType::ExitAck
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
}

/// ✅ NEW: WatchSignature - Executor → Mempool-watcher (Port 45100)
/// 
/// Sent immediately after submitting a transaction to register it for confirmation tracking.
/// Mempool-watcher will watch the Yellowstone gRPC stream for this signature.
/// 
/// Message type 25 (new message type for watcher)
#[derive(Debug, Clone)]
#[repr(C, packed)]
pub struct WatchSignature {
    pub msg_type: u8,              // 25
    pub signature: [u8; 64],       // Transaction signature
    pub mint: [u8; 32],            // Token mint
    pub trade_id: [u8; 16],        // UUID of trade (first 16 bytes)
    pub side: u8,                  // 0=BUY, 1=SELL
    pub timestamp_ns: u64,         // When registered (nanoseconds)
    pub _padding: [u8; 6],         // Padding to align to 128 bytes
}

impl WatchSignature {
    pub const SIZE: usize = 128;
    pub const MSG_TYPE: u8 = 25;
    
    /// Create new WatchSignature
    pub fn new(signature: &str, mint: [u8; 32], trade_id: &str, side: u8) -> Result<Self> {
        // Decode base58 signature to bytes
        let sig_bytes = bs58::decode(signature)
            .into_vec()
            .context("Failed to decode signature")?;
        
        if sig_bytes.len() != 64 {
            anyhow::bail!("Invalid signature length: {} (expected 64)", sig_bytes.len());
        }
        
        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&sig_bytes);
        
        let mut trade_id_bytes = [0u8; 16];
        let uuid_bytes = trade_id.as_bytes();
        let copy_len = uuid_bytes.len().min(16);
        trade_id_bytes[..copy_len].copy_from_slice(&uuid_bytes[..copy_len]);
        
        Ok(Self {
            msg_type: Self::MSG_TYPE,
            signature: sig_array,
            mint,
            trade_id: trade_id_bytes,
            side,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            _padding: [0u8; 6],
        })
    }
    
    /// Serialize to bytes for UDP transmission
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(Self::SIZE);
        bytes.push(self.msg_type);
        bytes.extend_from_slice(&self.signature);
        bytes.extend_from_slice(&self.mint);
        bytes.extend_from_slice(&self.trade_id);
        bytes.push(self.side);
        bytes.extend_from_slice(&self.timestamp_ns.to_le_bytes());
        bytes.extend_from_slice(&self._padding);
        bytes
    }
}
