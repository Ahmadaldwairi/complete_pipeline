/// Advice Bus - Non-blocking live intelligence from external collectors
/// 
/// Architecture: Collectors (24/7) → UDP messages → This module → ActivePosition updates
/// Performance: < 50µs per check, zero blocking, falls back gracefully if bus silent
use std::net::UdpSocket;
use std::time::Duration;
use anyhow::{Result, Context};
use log::{debug, warn};

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
            _ => anyhow::bail!("Unknown advisory type: {}", msg_type),
        }
    }
    
    /// Serialize to 64-byte UDP packet (for testing/collectors)
    pub fn to_bytes(&self) -> [u8; 96] {
        let mut buf = [0u8; 96];
        
        match self {
            Advisory::ExtendHold { mint, extra_secs, confidence, .. } => {
                buf[0] = 1; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..35].copy_from_slice(&extra_secs.to_le_bytes());
                buf[35] = *confidence;
            }
            Advisory::WidenExit { mint, sell_slip_bps, ttl_ms, confidence, .. } => {
                buf[0] = 2; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..35].copy_from_slice(&sell_slip_bps.to_le_bytes());
                buf[35..37].copy_from_slice(&ttl_ms.to_le_bytes());
                buf[37] = *confidence;
            }
            Advisory::LateOpportunity { mint, horizon_sec, score, .. } => {
                buf[0] = 3; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..35].copy_from_slice(&horizon_sec.to_le_bytes());
                buf[35] = *score;
            }
            Advisory::CopyTrade { mint, wallet, confidence, .. } => {
                buf[0] = 4; // Type
                buf[1..33].copy_from_slice(mint);
                buf[33..65].copy_from_slice(wallet);
                buf[65] = *confidence;
            }
            Advisory::SolPriceUpdate { price_cents, timestamp_secs, source, .. } => {
                buf[0] = 5; // Type
                buf[1..5].copy_from_slice(&price_cents.to_le_bytes());
                buf[5..9].copy_from_slice(&timestamp_secs.to_le_bytes());
                buf[9] = *source;
            }
            Advisory::EmergencyExit { mint, wallet, sell_amount_sol, wallet_win_rate, confidence, .. } => {
                buf[0] = 6; // Type (TASK 12) - Changed from 5 to 6
                buf[1..33].copy_from_slice(mint);
                buf[33..65].copy_from_slice(wallet);
                buf[65..69].copy_from_slice(&sell_amount_sol.to_le_bytes());
                buf[69] = *wallet_win_rate;
                buf[70] = *confidence;
            }
        }
        
        buf
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
        
        // Set non-blocking mode (critical for hot path)
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
    
    /// Try to receive one advisory (non-blocking, < 50µs)
    pub fn try_recv(&self) -> Option<Advisory> {
        if !self.enabled {
            return None;
        }
        
        let mut buf = [0u8; 96];
        
        match self.socket.recv_from(&mut buf) {
            Ok((len, _addr)) => {
                if len < 64 {
                    warn!("Received undersized advisory: {} bytes", len);
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
