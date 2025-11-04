/// Advice Bus - Non-blocking live intelligence from external collectors
/// 
/// Architecture: Collectors (24/7) → UDP messages → This module → ActivePosition updates
/// Performance: < 50µs per check, zero blocking, falls back gracefully if bus silent
use std::net::UdpSocket;
use std::time::Duration;
use anyhow::{Result, Context};
use log::{debug, warn, info};

/// Unified message type for routing
#[derive(Debug)]
pub enum MessageType {
    TradeDecision(TradeDecision),
    Advisory(Advisory),
}

/// TradeDecision from Brain (52 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TradeDecision {
    pub msg_type: u8,           // 1 = TRADE_DECISION
    pub mint: [u8; 32],         // Token mint address
    pub side: u8,               // 0 = BUY, 1 = SELL
    pub size_lamports: u64,     // Trade size in lamports
    pub slippage_bps: u16,      // Slippage tolerance in basis points
    pub confidence: u8,         // Confidence score 0-100
    pub retry_count: u8,        // Retry count for progressive slippage (SELL only)
    pub entry_type: u8,         // Entry strategy: 0=Rank, 1=Momentum, 2=CopyTrade, 3=LateOpportunity
    pub _padding: [u8; 3],      // Padding to 52 bytes
}

impl TradeDecision {
    pub const SIZE: usize = 52;
    pub const MSG_TYPE: u8 = 1;
    
    /// Deserialize from 52-byte UDP packet
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 52 {
            anyhow::bail!("TradeDecision message too short: {} bytes", buf.len());
        }
        
        // FIXED: Brain protocol has: [msg_type][protocol_version][mint 32 bytes][side][size 8][slippage 2][conf]
        // Byte layout: 0=msg_type, 1=protocol_version, 2-34=mint, 34=side, 35-43=size, 43-45=slippage, 45=conf
        let msg_type = buf[0];
        let protocol_version = buf[1];  // Added - was missing!
        let mut mint = [0u8; 32];
        mint.copy_from_slice(&buf[2..34]);  // Fixed: was 1..33, now 2..34
        let side = buf[34];                  // Fixed: was 33, now 34
        let size_lamports = u64::from_le_bytes([
            buf[35], buf[36], buf[37], buf[38],  // Fixed: was 34-41, now 35-43
            buf[39], buf[40], buf[41], buf[42],
        ]);
        let slippage_bps = u16::from_le_bytes([buf[43], buf[44]]);  // Fixed: was 42-43, now 43-44
        let confidence = buf[45];            // Fixed: was 44, now 45
        let retry_count = buf[47];           // Retry count for progressive slippage
        let entry_type = buf[48];            // Entry strategy type
        
        Ok(TradeDecision {
            msg_type,
            mint,
            side,
            size_lamports,
            slippage_bps,
            confidence,
            retry_count,
            entry_type,
            _padding: [0; 3],
        })
    }
    
    pub fn is_buy(&self) -> bool {
        self.side == 0
    }
    
    pub fn is_sell(&self) -> bool {
        self.side == 1
    }
    
    /// Get human-readable entry strategy name
    pub fn entry_strategy_name(&self) -> &'static str {
        match self.entry_type {
            0 => "New Mint (Rank)",
            1 => "Momentum",
            2 => "Copy Trade",
            3 => "Late Opportunity",
            _ => "Unknown",
        }
    }
}

/// Fixed-size advisory messages (64 bytes for cache-line alignment)
/// Sent from external collectors (launch collector, wallet tracker)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub enum Advisory {
    /// Extend hold time - "volume/buyers still coming, don't exit yet"
    ExtendHold {
        mint: [u8; 32],      // Token address
        extra_secs: u16,     // Additional seconds to hold (5-30)
        confidence: u8,      // Confidence score 0-100
        _padding: [u8; 29],  // Pad to 64 bytes
    },
    
    /// Widen exit slippage - "exit urgently, accept higher slippage"
    WidenExit {
        mint: [u8; 32],      // Token address
        sell_slip_bps: u16,  // Slippage in basis points (e.g., 2500 = 25%)
        ttl_ms: u16,         // Time-to-live in milliseconds (auto-reset after)
        confidence: u8,      // Confidence score 0-100
        _padding: [u8; 27],  // Pad to 64 bytes
    },
    
    /// Late opportunity - "token from 2 days ago heating up, consider entry"
    LateOpportunity {
        mint: [u8; 32],      // Token address
        horizon_sec: u16,    // Expected opportunity window (30-300s)
        score: u8,           // Opportunity score 0-100
        _padding: [u8; 29],  // Pad to 64 bytes
    },
    
    /// Copy trade - "tracked alpha wallet just bought"
    /// TODO: Future enhancement - add trade_size_sol field for filtering small buys
    /// Would require protocol change: trade_size_sol: f64 (8 bytes), adjust _padding: [u8; 23]
    CopyTrade {
        mint: [u8; 32],      // Token address
        wallet: [u8; 32],    // Alpha wallet address (truncated to fit)
        confidence: u8,      // Confidence score 0-100
        _padding: [u8; 31],  // Pad to 64 bytes (32+32+1+31=96, close enough)
    },
    
    /// SOL Price Update - "current SOL/USD price from reliable source"
    /// Broadcast every 20s by copytrader bot to avoid API failures during trades
    SolPriceUpdate {
        price_cents: u32,    // Price in cents (e.g., 18283 = $182.83)
        timestamp_secs: u32, // Unix timestamp
        source: u8,          // 1=Helius, 2=Jupiter, 3=Fallback
        _padding: [u8; 55],  // Pad to 64 bytes
    },
    
    /// TASK 12: Emergency exit - "high-confidence wallet dumped large position, exit NOW"
    EmergencyExit {
        mint: [u8; 32],      // Token address
        wallet: [u8; 32],    // Alpha wallet that sold (for logging)
        sell_amount_sol: u32, // Amount sold in SOL * 1000 (e.g., 15500 = 15.5 SOL)
        wallet_win_rate: u8, // Wallet's historical win rate (for validation)
        confidence: u8,      // Confidence score 0-100 (should be 80-95)
        _padding: [u8; 27],  // Pad to 96 bytes
    },
    
    /// TradeSubmitted - "transaction submitted to network, awaiting confirmation"
    TradeSubmitted {
        mint: [u8; 32],         // Token address
        signature: [u8; 64],    // Transaction signature
        side: u8,               // 0=BUY, 1=SELL
        submitted_ts_ns: u64,   // Timestamp (nanoseconds)
        expected_tokens: u64,   // Expected token amount
        expected_sol: u64,      // Expected SOL amount (lamports)
        expected_slip_bps: u16, // Expected slippage (basis points)
        submitted_via: u8,      // 0=TPU, 1=RPC
        _padding: [u8; 5],      // Pad to 192 bytes
    },
    
    /// TradeConfirmed - "transaction confirmed on-chain with actual values"
    TradeConfirmed {
        mint: [u8; 32],         // Token address
        signature: [u8; 64],    // Transaction signature
        side: u8,               // 0=BUY, 1=SELL
        confirmed_ts_ns: u64,   // Timestamp (nanoseconds)
        actual_tokens: u64,     // Actual token amount received
        actual_sol: u64,        // Actual SOL amount (lamports)
        total_fees: u64,        // Total fees paid (lamports)
        compute_units: u32,     // Compute units used
        fast_confirm: u8,       // 1=mempool-based, 0=finalized
        tx_status: u8,          // 0=confirmed, 1=finalized
        _padding: [u8; 6],      // Pad to 208 bytes
    },
    
    /// TradeFailed - "transaction failed or timed out"
    TradeFailed {
        mint: [u8; 32],         // Token address
        signature: [u8; 64],    // Transaction signature (if available)
        side: u8,               // 0=BUY, 1=SELL
        failed_ts_ns: u64,      // Timestamp (nanoseconds)
        reason_code: u8,        // 1=timeout, 2=slippage, 3=instruction_error, 4=blockhash, 5=other
        has_signature: u8,      // 1=sig available, 0=failed before submission
        reason_str: [u8; 64],   // Human-readable reason
        _padding: [u8; 6],      // Pad to 176 bytes
    },
}

impl Advisory {
    /// Deserialize from 64-byte UDP packet
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() < 64 {
            anyhow::bail!("Advisory message too short: {} bytes", buf.len());
        }
        
        let msg_type = buf[0];
        match msg_type {
            1 => {
                // ExtendHold
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let extra_secs = u16::from_le_bytes([buf[33], buf[34]]);
                let confidence = buf[35];
                
                Ok(Advisory::ExtendHold {
                    mint,
                    extra_secs,
                    confidence,
                    _padding: [0; 29],
                })
            }
            2 => {
                // WidenExit
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let sell_slip_bps = u16::from_le_bytes([buf[33], buf[34]]);
                let ttl_ms = u16::from_le_bytes([buf[35], buf[36]]);
                let confidence = buf[37];
                
                Ok(Advisory::WidenExit {
                    mint,
                    sell_slip_bps,
                    ttl_ms,
                    confidence,
                    _padding: [0; 27],
                })
            }
            3 => {
                // LateOpportunity
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let horizon_sec = u16::from_le_bytes([buf[33], buf[34]]);
                let score = buf[35];
                
                Ok(Advisory::LateOpportunity {
                    mint,
                    horizon_sec,
                    score,
                    _padding: [0; 29],
                })
            }
            4 => {
                // CopyTrade
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let mut wallet = [0u8; 32];
                wallet.copy_from_slice(&buf[33..65]);
                let confidence = buf[65];
                
                Ok(Advisory::CopyTrade {
                    mint,
                    wallet,
                    confidence,
                    _padding: [0; 31],
                })
            }
            5 => {
                // SolPriceUpdate
                let price_cents = u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]);
                let timestamp_secs = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]);
                let source = buf[9];
                
                Ok(Advisory::SolPriceUpdate {
                    price_cents,
                    timestamp_secs,
                    source,
                    _padding: [0; 55],
                })
            }
            6 => {
                // EmergencyExit (TASK 12) - Changed from type 5 to 6
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let mut wallet = [0u8; 32];
                wallet.copy_from_slice(&buf[33..65]);
                let sell_amount_sol = u32::from_le_bytes([buf[65], buf[66], buf[67], buf[68]]);
                let wallet_win_rate = buf[69];
                let confidence = buf[70];
                
                Ok(Advisory::EmergencyExit {
                    mint,
                    wallet,
                    sell_amount_sol,
                    wallet_win_rate,
                    confidence,
                    _padding: [0; 27],
                })
            }
            18 => {
                // TradeSubmitted
                if buf.len() < 192 {
                    anyhow::bail!("TradeSubmitted message too short: {} bytes", buf.len());
                }
                
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let mut signature = [0u8; 64];
                signature.copy_from_slice(&buf[33..97]);
                let side = buf[97];
                let submitted_ts_ns = u64::from_le_bytes([buf[98], buf[99], buf[100], buf[101],
                                                          buf[102], buf[103], buf[104], buf[105]]);
                let expected_tokens = u64::from_le_bytes([buf[106], buf[107], buf[108], buf[109],
                                                          buf[110], buf[111], buf[112], buf[113]]);
                let expected_sol = u64::from_le_bytes([buf[114], buf[115], buf[116], buf[117],
                                                       buf[118], buf[119], buf[120], buf[121]]);
                let expected_slip_bps = u16::from_le_bytes([buf[122], buf[123]]);
                let submitted_via = buf[124];
                
                Ok(Advisory::TradeSubmitted {
                    mint,
                    signature,
                    side,
                    submitted_ts_ns,
                    expected_tokens,
                    expected_sol,
                    expected_slip_bps,
                    submitted_via,
                    _padding: [0; 5],
                })
            }
            19 => {
                // TradeConfirmed
                if buf.len() < 208 {
                    anyhow::bail!("TradeConfirmed message too short: {} bytes", buf.len());
                }
                
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let mut signature = [0u8; 64];
                signature.copy_from_slice(&buf[33..97]);
                let side = buf[97];
                let confirmed_ts_ns = u64::from_le_bytes([buf[98], buf[99], buf[100], buf[101],
                                                          buf[102], buf[103], buf[104], buf[105]]);
                let actual_tokens = u64::from_le_bytes([buf[106], buf[107], buf[108], buf[109],
                                                        buf[110], buf[111], buf[112], buf[113]]);
                let actual_sol = u64::from_le_bytes([buf[114], buf[115], buf[116], buf[117],
                                                     buf[118], buf[119], buf[120], buf[121]]);
                let total_fees = u64::from_le_bytes([buf[122], buf[123], buf[124], buf[125],
                                                     buf[126], buf[127], buf[128], buf[129]]);
                let compute_units = u32::from_le_bytes([buf[130], buf[131], buf[132], buf[133]]);
                let fast_confirm = buf[134];
                let tx_status = buf[135];
                
                Ok(Advisory::TradeConfirmed {
                    mint,
                    signature,
                    side,
                    confirmed_ts_ns,
                    actual_tokens,
                    actual_sol,
                    total_fees,
                    compute_units,
                    fast_confirm,
                    tx_status,
                    _padding: [0; 6],
                })
            }
            20 => {
                // TradeFailed
                if buf.len() < 176 {
                    anyhow::bail!("TradeFailed message too short: {} bytes", buf.len());
                }
                
                let mut mint = [0u8; 32];
                mint.copy_from_slice(&buf[1..33]);
                let mut signature = [0u8; 64];
                signature.copy_from_slice(&buf[33..97]);
                let side = buf[97];
                let failed_ts_ns = u64::from_le_bytes([buf[98], buf[99], buf[100], buf[101],
                                                       buf[102], buf[103], buf[104], buf[105]]);
                let reason_code = buf[106];
                let has_signature = buf[107];
                let mut reason_str = [0u8; 64];
                reason_str.copy_from_slice(&buf[108..172]);
                
                Ok(Advisory::TradeFailed {
                    mint,
                    signature,
                    side,
                    failed_ts_ns,
                    reason_code,
                    has_signature,
                    reason_str,
                    _padding: [0; 6],
                })
            }
            _ => anyhow::bail!("Unknown advisory type: {}", msg_type),
        }
    }
    
    /// Serialize to UDP packet (size varies by message type)
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Advisory::ExtendHold { mint, extra_secs, confidence, .. } => {
                let mut buf = vec![0u8; 96];
                buf[0] = 1; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..35].copy_from_slice(&extra_secs.to_le_bytes());
                buf[35] = *confidence;
                buf
            }
            Advisory::WidenExit { mint, sell_slip_bps, ttl_ms, confidence, .. } => {
                let mut buf = vec![0u8; 96];
                buf[0] = 2; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..35].copy_from_slice(&sell_slip_bps.to_le_bytes());
                buf[35..37].copy_from_slice(&ttl_ms.to_le_bytes());
                buf[37] = *confidence;
                buf
            }
            Advisory::LateOpportunity { mint, horizon_sec, score, .. } => {
                let mut buf = vec![0u8; 96];
                buf[0] = 3; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..35].copy_from_slice(&horizon_sec.to_le_bytes());
                buf[35] = *score;
                buf
            }
            Advisory::CopyTrade { mint, wallet, confidence, .. } => {
                let mut buf = vec![0u8; 96];
                buf[0] = 4; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..65].copy_from_slice(wallet);
                buf[65] = *confidence;
                buf
            }
            Advisory::SolPriceUpdate { price_cents, timestamp_secs, source, .. } => {
                let mut buf = vec![0u8; 96];
                buf[0] = 5; // Type
                buf[1..5].copy_from_slice(&price_cents.to_le_bytes());
                buf[5..9].copy_from_slice(&timestamp_secs.to_le_bytes());
                buf[9] = *source;
                buf
            }
            Advisory::EmergencyExit { mint, wallet, sell_amount_sol, wallet_win_rate, confidence, .. } => {
                let mut buf = vec![0u8; 96];
                buf[0] = 6; // Type (TASK 12) - Changed from 5 to 6
                buf[1..33].copy_from_slice(mint);
                buf[33..65].copy_from_slice(wallet);
                buf[65..69].copy_from_slice(&sell_amount_sol.to_le_bytes());
                buf[69] = *wallet_win_rate;
                buf[70] = *confidence;
                buf
            }
            Advisory::TradeSubmitted { mint, signature, side, submitted_ts_ns, expected_tokens, 
                                      expected_sol, expected_slip_bps, submitted_via, .. } => {
                let mut buf = vec![0u8; 192];
                buf[0] = 18; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..97].copy_from_slice(signature);
                buf[97] = *side;
                buf[98..106].copy_from_slice(&submitted_ts_ns.to_le_bytes());
                buf[106..114].copy_from_slice(&expected_tokens.to_le_bytes());
                buf[114..122].copy_from_slice(&expected_sol.to_le_bytes());
                buf[122..124].copy_from_slice(&expected_slip_bps.to_le_bytes());
                buf[124] = *submitted_via;
                buf
            }
            Advisory::TradeConfirmed { mint, signature, side, confirmed_ts_ns, actual_tokens,
                                      actual_sol, total_fees, compute_units, fast_confirm, tx_status, .. } => {
                let mut buf = vec![0u8; 208];
                buf[0] = 19; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..97].copy_from_slice(signature);
                buf[97] = *side;
                buf[98..106].copy_from_slice(&confirmed_ts_ns.to_le_bytes());
                buf[106..114].copy_from_slice(&actual_tokens.to_le_bytes());
                buf[114..122].copy_from_slice(&actual_sol.to_le_bytes());
                buf[122..130].copy_from_slice(&total_fees.to_le_bytes());
                buf[130..134].copy_from_slice(&compute_units.to_le_bytes());
                buf[134] = *fast_confirm;
                buf[135] = *tx_status;
                buf
            }
            Advisory::TradeFailed { mint, signature, side, failed_ts_ns, reason_code, 
                                   has_signature, reason_str, .. } => {
                let mut buf = vec![0u8; 176];
                buf[0] = 20; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..97].copy_from_slice(signature);
                buf[97] = *side;
                buf[98..106].copy_from_slice(&failed_ts_ns.to_le_bytes());
                buf[106] = *reason_code;
                buf[107] = *has_signature;
                buf[108..172].copy_from_slice(reason_str);
                buf
            }
        }
    }
    
    /// Get token mint as string
    pub fn mint_str(&self) -> String {
        let mint = match self {
            Advisory::ExtendHold { mint, .. } => mint,
            Advisory::WidenExit { mint, .. } => mint,
            Advisory::LateOpportunity { mint, .. } => mint,
            Advisory::CopyTrade { mint, .. } => mint,
            Advisory::SolPriceUpdate { .. } => return String::from("N/A"), // No mint for price updates
            Advisory::EmergencyExit { mint, .. } => mint,
            Advisory::TradeSubmitted { mint, .. } => mint,
            Advisory::TradeConfirmed { mint, .. } => mint,
            Advisory::TradeFailed { mint, .. } => mint,
        };
        bs58::encode(mint).into_string()
    }
    
    /// Get confidence/score (0-100)
    pub fn confidence(&self) -> u8 {
        match self {
            Advisory::ExtendHold { confidence, .. } => *confidence,
            Advisory::WidenExit { confidence, .. } => *confidence,
            Advisory::LateOpportunity { score, .. } => *score,
            Advisory::CopyTrade { confidence, .. } => *confidence,
            Advisory::SolPriceUpdate { .. } => 100, // Price updates always max confidence
            Advisory::EmergencyExit { confidence, .. } => *confidence,
            Advisory::TradeSubmitted { .. } => 100, // Always process submitted notifications
            Advisory::TradeConfirmed { .. } => 100, // Always process confirmations
            Advisory::TradeFailed { .. } => 100, // Always process failures
        }
    }
}

/// Non-blocking UDP listener for advice bus
pub struct AdviceBusListener {
    socket: UdpSocket,
    enabled: bool,
    min_confidence: u8,
}

impl AdviceBusListener {
    /// Create new listener on specified port
    pub fn new(port: u16, min_confidence: u8) -> Result<Self> {
        let addr = format!("127.0.0.1:{}", port);
        let socket = UdpSocket::bind(&addr)
            .context(format!("Failed to bind UDP socket to {}", addr))?;
        
        // CRITICAL: Flush any stale messages from previous sessions BEFORE setting non-blocking
        // UDP packets can persist in kernel buffers across process restarts
        // Use very short timeout so we don't block if buffer is empty
        socket.set_read_timeout(Some(Duration::from_millis(1)))
            .context("Failed to set initial read timeout")?;
        
        let mut flush_buffer = [0u8; 4096];
        let mut flushed_count = 0;
        loop {
            match socket.recv(&mut flush_buffer) {
                Ok(_) => {
                    flushed_count += 1;
                    if flushed_count > 100 {
                        break; // Safety limit
                    }
                }
                Err(_) => break, // Timeout = buffer empty
            }
        }
        if flushed_count > 0 {
            warn!("🧹 Flushed {} stale messages from UDP buffer on startup", flushed_count);
        }
        
        // NOW set non-blocking mode for normal operation (critical for hot path)
        socket.set_nonblocking(true)
            .context("Failed to set socket non-blocking")?;
        
        // Set small read timeout as backup
        socket.set_read_timeout(Some(Duration::from_micros(10)))
            .context("Failed to set read timeout")?;
        
        debug!("Advice Bus listening on {}", addr);
        
        Ok(Self {
            socket,
            enabled: true,
            min_confidence,
        })
    }
    
    /// Try to receive any message type (non-blocking)
    /// Returns either TradeDecision or Advisory based on message size
    pub fn try_recv_any(&self) -> Option<MessageType> {
        if !self.enabled {
            return None;
        }
        
        let mut buf = [0u8; 96];
        
        match self.socket.recv_from(&mut buf) {
            Ok((len, _addr)) => {
                if len < 52 {
                    warn!("Received undersized message: {} bytes (need >= 52)", len);
                    return None;
                }
                
                // Route by message size
                if len == 52 {
                    // TradeDecision
                    match TradeDecision::from_bytes(&buf) {
                        Ok(decision) => Some(MessageType::TradeDecision(decision)),
                        Err(e) => {
                            warn!("Failed to parse TradeDecision: {}", e);
                            None
                        }
                    }
                } else {
                    // Advisory (64+ bytes)
                    match Advisory::from_bytes(&buf) {
                        Ok(advisory) => {
                            if advisory.confidence() >= self.min_confidence {
                                Some(MessageType::Advisory(advisory))
                            } else {
                                debug!("Rejected low-confidence advisory: {}", advisory.confidence());
                                None
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse advisory: {}", e);
                            None
                        }
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                None
            }
            Err(e) => {
                warn!("Socket error: {}", e);
                None
            }
        }
    }
    
    /// Try to receive one advisory (non-blocking, < 50µs)
    pub fn try_recv(&self) -> Option<Advisory> {
        if !self.enabled {
            return None;
        }
        
        let mut buf = [0u8; 96];
        
        match self.socket.recv_from(&mut buf) {
            Ok((len, _addr)) => {
                // Accept both TradeDecision (52 bytes) and Advisory (64+ bytes) formats
                if len < 52 {
                    warn!("Received undersized message: {} bytes (need >= 52)", len);
                    return None;
                }
                
                // If 52 bytes, it's likely a TradeDecision - skip Advisory parsing
                if len == 52 {
                    debug!("Received 52-byte message (likely TradeDecision), skipping Advisory parse");
                    return None;
                }
                
                match Advisory::from_bytes(&buf) {
                    Ok(advisory) => {
                        // Filter by confidence
                        if advisory.confidence() >= self.min_confidence {
                            debug!("Received advisory: {:?}", advisory);
                            Some(advisory)
                        } else {
                            debug!("Rejected low-confidence advisory: {}", advisory.confidence());
                            None
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse advisory: {}", e);
                        None
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available - this is expected and normal
                None
            }
            Err(e) => {
                warn!("Socket error: {}", e);
                None
            }
        }
    }
    
    /// Try to receive one TradeDecision from Brain (non-blocking)
    pub fn try_recv_trade_decision(&self) -> Option<TradeDecision> {
        if !self.enabled {
            return None;
        }
        
        let mut buf = [0u8; 96];
        
        match self.socket.recv_from(&mut buf) {
            Ok((len, _addr)) => {
                // TradeDecisions are exactly 52 bytes
                if len == 52 {
                    // DEBUG: Log raw bytes received BEFORE parsing
                    let mint_hex = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}", 
                                          buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9]);
                    info!("🔍 EXECUTOR RECEIVED RAW: len={}, side_byte[34]={}, mint_bytes[2..10]={}", 
                          len, buf[34], mint_hex);
                    
                    match TradeDecision::from_bytes(&buf) {
                        Ok(decision) => {
                            let mint_hex_parsed = format!("{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}", 
                                                         decision.mint[0], decision.mint[1], decision.mint[2], decision.mint[3],
                                                         decision.mint[4], decision.mint[5], decision.mint[6], decision.mint[7]);
                            info!("📥 RECEIVED TradeDecision: {} {} lamports (conf={}) | mint={}",
                                  if decision.is_buy() { "BUY" } else { "SELL" },
                                  decision.size_lamports,
                                  decision.confidence,
                                  mint_hex_parsed);
                            Some(decision)
                        }
                        Err(e) => {
                            warn!("Failed to parse TradeDecision: {}", e);
                            None
                        }
                    }
                } else {
                    // Not a TradeDecision, ignore (will be picked up by try_recv)
                    None
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                None
            }
            Err(e) => {
                warn!("Socket error: {}", e);
                None
            }
        }
    }
    
    /// Drain up to N advisories in one tick (prevents backpressure)
    pub fn drain(&self, max_per_tick: usize) -> Vec<Advisory> {
        let mut advisories = Vec::with_capacity(max_per_tick);
        
        for _ in 0..max_per_tick {
            match self.try_recv() {
                Some(advisory) => advisories.push(advisory),
                None => break,
            }
        }
        
        advisories
    }
    
    /// Disable listener (for testing/emergency)
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_advisory_roundtrip() {
        let mint = [1u8; 32];
        let original = Advisory::ExtendHold {
            mint,
            extra_secs: 15,
            confidence: 85,
            _padding: [0; 29],
        };
        
        let bytes = original.to_bytes();
        let decoded = Advisory::from_bytes(&bytes).unwrap();
        
        if let Advisory::ExtendHold { extra_secs, confidence, .. } = decoded {
            assert_eq!(extra_secs, 15);
            assert_eq!(confidence, 85);
        } else {
            panic!("Wrong advisory type decoded");
        }
    }
}
