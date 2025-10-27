/// UDP Advisory Sender - Send advisories to execution bot
/// 
/// Sends various advisory types via UDP to help execution bot make better decisions.
/// Advisory types:
/// - Type 1: ExtendHold - Hold position longer than normal
/// - Type 2: WidenExit - Increase exit slippage tolerance
/// - Type 3: LateOpportunity - New token with strong momentum
/// - Type 4: CopyTrade - Alpha wallet activity detected
/// - Type 5: SolPriceUpdate - SOL price update from oracle

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use tracing::{debug, info};

/// Default target for execution bot UDP listener
const DEFAULT_ADVICE_HOST: &str = "127.0.0.1";
const DEFAULT_ADVICE_PORT: u16 = 45100;

/// Advisory packet size (fixed 64 bytes)
const ADVISORY_SIZE: usize = 64;

/// Advisory message types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdviceType {
    ExtendHold = 1,
    WidenExit = 2,
    LateOpportunity = 3,
    CopyTrade = 4,
    SolPriceUpdate = 5,
}

/// UDP Advisory Sender (cloneable via Arc)
#[derive(Clone)]
pub struct AdvisorySender {
    socket: Arc<UdpSocket>,
    target_addr: String,
}

impl AdvisorySender {
    /// Create a new advisory sender
    /// 
    /// # Arguments
    /// * `host` - Target host (e.g., "127.0.0.1")
    /// * `port` - Target port (e.g., 45100)
    pub fn new(host: &str, port: u16) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket")?;
        
        // Set non-blocking to avoid delays if execution bot is offline
        socket.set_nonblocking(true)
            .context("Failed to set non-blocking mode")?;
        
        let target_addr = format!("{}:{}", host, port);
        
        info!("✅ AdvisorySender initialized → {}", target_addr);
        
        Ok(Self {
            socket: Arc::new(socket),
            target_addr,
        })
    }
    
    /// Create with default host/port
    pub fn new_default() -> Result<Self> {
        Self::new(DEFAULT_ADVICE_HOST, DEFAULT_ADVICE_PORT)
    }
    
    /// Send a raw advisory packet (internal helper)
    fn send_advice(&self, packet: &[u8]) -> Result<()> {
        match self.socket.send_to(packet, &self.target_addr) {
            Ok(_) => Ok(()),
            // Gracefully handle if execution bot is offline
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Send LateOpportunity advisory (Type 3)
    /// 
    /// Sent when a new token launch shows strong early momentum.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `horizon_sec` - Time horizon in seconds (how long opportunity is valid)
    /// * `score` - Opportunity score 0-100 (higher = stronger signal)
    pub fn send_late_opportunity(&self, mint_b58: &str, horizon_sec: u16, score: u8) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::LateOpportunity as u8;
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..35].copy_from_slice(&horizon_sec.to_le_bytes());
        msg[35] = score.clamp(0, 100);
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 LateOpportunity: {}... | horizon: {}s | score: {}",
            &mint_b58[..12], horizon_sec, score
        );
        
        Ok(())
    }
    
    /// Send CopyTrade advisory (Type 4)
    /// 
    /// Sent when a tracked alpha wallet creates or buys a token.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `wallet_b58` - Alpha wallet address (base58, 32 bytes)
    /// * `confidence` - Confidence score 0-100 (higher = more reliable wallet)
    pub fn send_copy_trade(&self, mint_b58: &str, wallet_b58: &str, confidence: u8) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        let wallet_bytes = bs58::decode(wallet_b58).into_vec()
            .context("Invalid wallet base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        if wallet_bytes.len() != 32 {
            anyhow::bail!("Wallet must be 32 bytes, got {}", wallet_bytes.len());
        }
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::CopyTrade as u8;
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..64].copy_from_slice(&wallet_bytes[0..31]); // Truncate wallet to 31 bytes
        msg[63] = confidence.clamp(0, 100);
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 CopyTrade: {}... from {}... | confidence: {}",
            &mint_b58[..12], &wallet_b58[..12], confidence
        );
        
        Ok(())
    }
    
    /// Send ExtendHold advisory (Type 1)
    /// 
    /// Sent when tracked wallet buys more of a token we already hold.
    /// Suggests holding position longer than normal exit strategy.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `extra_secs` - Extra seconds to hold beyond normal exit
    /// * `confidence` - Confidence score 0-100
    pub fn send_extend_hold(&self, mint_b58: &str, extra_secs: u16, confidence: u8) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::ExtendHold as u8;
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..35].copy_from_slice(&extra_secs.to_le_bytes());
        msg[35] = confidence.clamp(0, 100);
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 ExtendHold: {}... | extra: {}s | confidence: {}",
            &mint_b58[..12], extra_secs, confidence
        );
        
        Ok(())
    }
    
    /// Send WidenExit advisory (Type 2)
    /// 
    /// Sent when tracked wallet sells a token. Suggests exiting position
    /// with wider slippage tolerance for faster execution.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `slip_bps` - Max slippage in basis points (e.g., 500 = 5%)
    /// * `ttl_ms` - Time-to-live in milliseconds (urgency)
    /// * `confidence` - Confidence score 0-100
    pub fn send_widen_exit(&self, mint_b58: &str, slip_bps: u32, ttl_ms: u32, confidence: u8) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::WidenExit as u8;
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..37].copy_from_slice(&slip_bps.to_le_bytes());
        msg[37..41].copy_from_slice(&ttl_ms.to_le_bytes());
        msg[41] = confidence.clamp(0, 100);
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 WidenExit: {}... | slip: {}bps | ttl: {}ms | confidence: {}",
            &mint_b58[..12], slip_bps, ttl_ms, confidence
        );
        
        Ok(())
    }
    
    /// Send SolPriceUpdate advisory (Type 5)
    /// 
    /// Updates execution bot with current SOL price for USD calculations.
    /// 
    /// # Arguments
    /// * `price_usd` - SOL price in USD (e.g., 182.83)
    /// * `source` - Price source: 1=Helius, 2=Jupiter, 3=Other
    pub fn send_sol_price_update(&self, price_usd: f64, source: u8) -> Result<()> {
        let price_cents = (price_usd * 100.0) as u32;
        let timestamp_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::SolPriceUpdate as u8;
        msg[1..5].copy_from_slice(&price_cents.to_le_bytes());
        msg[5..9].copy_from_slice(&timestamp_secs.to_le_bytes());
        msg[9] = source;
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 SOL Price: ${:.2} from {}",
            price_usd,
            match source {
                1 => "Helius",
                2 => "Jupiter",
                _ => "Unknown",
            }
        );
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_advisory_sender_creation() {
        let sender = AdvisorySender::new("127.0.0.1", 45100);
        assert!(sender.is_ok());
    }
    
    #[test]
    fn test_clone() {
        let sender = AdvisorySender::new("127.0.0.1", 45100).unwrap();
        let cloned = sender.clone();
        assert_eq!(sender.target_addr, cloned.target_addr);
    }
    
    #[test]
    fn test_packet_sizes() {
        // Ensure all advisory types fit in 64 bytes
        assert_eq!(ADVISORY_SIZE, 64);
        
        // Test mint address encoding
        let test_mint = "So11111111111111111111111111111111111111112"; // SOL mint
        let mint_bytes = bs58::decode(test_mint).into_vec().unwrap();
        assert_eq!(mint_bytes.len(), 32);
    }
}
