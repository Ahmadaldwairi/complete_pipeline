/// UDP Advisory Sender - Send advisories to execution bot
/// 
/// Sends various advisory types via UDP to help execution bot make better decisions.
/// Advisory types:
/// - Type 1: ExtendHold - Hold position longer than normal
/// - Type 2: WidenExit - Increase exit slippage tolerance
/// - Type 3: LateOpportunity - New token with strong momentum
/// - Type 4: CopyTrade - Alpha wallet activity detected
/// - Type 5: SolPriceUpdate - SOL price update from oracle

pub mod batched_sender;

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::{Context, Result};
use tracing::{debug, info};

pub use batched_sender::{spawn_batched_sender, BatchedAdvisorySender, BatchedBrainSignalSender, UdpMessage};

/// Default target for execution bot UDP listener
const DEFAULT_ADVICE_HOST: &str = "127.0.0.1";
const DEFAULT_ADVICE_PORT: u16 = 45100;

/// Advisory packet size (fixed 64 bytes)
const ADVISORY_SIZE: usize = 64;

/// Advisory message types
/// IMPORTANT: These must match Brain's AdviceMessageType enum exactly!
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdviceType {
    ExtendHold = 10,         // Brain expects 10
    WidenExit = 11,          // Brain expects 11
    LateOpportunity = 12,    // Brain expects 12 - Path D
    CopyTrade = 13,          // Brain expects 13 - Path C
    SolPriceUpdate = 14,     // Brain expects 14
    RankOpportunity = 15,    // Path A: Top-ranked new launch
    MomentumOpportunity = 16, // Path B: High momentum token
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
    /// Send CopyTrade advisory (Type 13)
    /// 
    /// Sent when a tracked alpha wallet creates or buys a token.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `wallet_b58` - Wallet address (base58, 32 bytes)
    /// * `side` - 0=BUY, 1=SELL
    /// * `size_sol` - Trade size in SOL
    /// * `wallet_tier` - Wallet tier (0=Discovery, 1=C, 2=B, 3=A)
    /// * `confidence` - Confidence score 0-100
    pub fn send_copy_trade(
        &self, 
        mint_b58: &str, 
        wallet_b58: &str, 
        side: u8,
        size_sol: f32,
        wallet_tier: u8,
        confidence: u8
    ) -> Result<()> {
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
        
        // Brain expects 80 bytes: [type(1) | wallet(32) | mint(32) | side(1) | size(4) | tier(1) | conf(1) | padding(8)]
        let mut msg = vec![0u8; 80];
        msg[0] = AdviceType::CopyTrade as u8;
        msg[1..33].copy_from_slice(&wallet_bytes);
        msg[33..65].copy_from_slice(&mint_bytes);
        msg[65] = side;
        msg[66..70].copy_from_slice(&size_sol.to_le_bytes());
        msg[70] = wallet_tier;
        msg[71] = confidence.clamp(0, 100);
        // bytes 72-79 are padding (already zero)
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 CopyTrade: {}... from {}... | side: {} | size: {:.2} SOL | tier: {} | confidence: {}",
            &mint_b58[..12], &wallet_b58[..12], side, size_sol, wallet_tier, confidence
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
    
    /// Send RankOpportunity advisory (Type 6)
    /// 
    /// Sent when a new token launch ranks in top N by tier.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `rank` - Token's rank (1-255, lower is better)
    /// * `score` - Follow-through score 0-100
    pub fn send_rank_opportunity(&self, mint_b58: &str, rank: u8, score: u8) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::RankOpportunity as u8;
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33] = rank;
        msg[34] = score.clamp(0, 100);
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 RankOpportunity: {}... | rank: {} | score: {}",
            &mint_b58[..12], rank, score
        );
        
        Ok(())
    }
    
    /// Send MomentumOpportunity advisory (Type 7)
    /// 
    /// Sent when a token shows high recent activity (volume + buyers).
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `vol_5s_sol` - Volume in last 5 seconds (SOL, scaled x100 for transmission)
    /// * `buyers_2s` - Unique buyers in last 2 seconds
    /// * `score` - Momentum score 0-100
    pub fn send_momentum_opportunity(&self, mint_b58: &str, vol_5s_sol: f64, buyers_2s: u32, score: u8) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let mut msg = vec![0u8; ADVISORY_SIZE];
        msg[0] = AdviceType::MomentumOpportunity as u8;
        msg[1..33].copy_from_slice(&mint_bytes);
        
        // Encode vol_5s_sol (scale by 100, store as u16)
        let vol_scaled = (vol_5s_sol * 100.0).clamp(0.0, 65535.0) as u16;
        msg[33..35].copy_from_slice(&vol_scaled.to_le_bytes());
        
        // Encode buyers_2s (u16 is enough)
        let buyers = buyers_2s.clamp(0, 65535) as u16;
        msg[35..37].copy_from_slice(&buyers.to_le_bytes());
        
        msg[37] = score.clamp(0, 100);
        
        self.send_advice(&msg)?;
        
        debug!(
            "📤 MomentumOpportunity: {}... | vol_5s: {:.2} SOL | buyers_2s: {} | score: {}",
            &mint_b58[..12], vol_5s_sol, buyers_2s, score
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

/// BrainSignalSender - Send real-time signals to Brain bot
/// 
/// Sends momentum, volume spike, and wallet activity signals from confirmed
/// transaction analysis to help Brain make better entry/exit decisions.
/// 
/// Signal types:
/// - Type 21: MomentumDetected - High buying momentum detected
/// - Type 22: VolumeSpike - Sudden volume increase detected
/// - Type 23: WalletActivity - Alpha wallet activity detected
#[derive(Clone)]
pub struct BrainSignalSender {
    socket: Arc<UdpSocket>,
    target_addr: String,
}

impl BrainSignalSender {
    /// Create a new brain signal sender
    /// 
    /// # Arguments
    /// * `host` - Target host (e.g., "127.0.0.1")
    /// * `port` - Target port (e.g., 45120 for brain)
    pub fn new(host: &str, port: u16) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket for brain signals")?;
        
        // Set non-blocking to avoid delays if brain is offline
        socket.set_nonblocking(true)
            .context("Failed to set non-blocking mode")?;
        
        let target_addr = format!("{}:{}", host, port);
        
        info!("✅ BrainSignalSender initialized → {}", target_addr);
        
        Ok(Self {
            socket: Arc::new(socket),
            target_addr,
        })
    }
    
    /// Send a raw signal packet (internal helper)
    fn send_signal(&self, packet: &[u8]) -> Result<()> {
        match self.socket.send_to(packet, &self.target_addr) {
            Ok(_) => Ok(()),
            // Gracefully handle if brain is offline
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
    
    /// Send MomentumDetected signal (Type 21)
    /// 
    /// Sent when high buying momentum is detected in confirmed transactions.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `buys_in_last_500ms` - Number of buy transactions in last 500ms
    /// * `volume_sol` - SOL volume in the window
    /// * `unique_buyers` - Number of unique buyers
    /// * `confidence` - Confidence score 0-100
    pub fn send_momentum_detected(
        &self,
        mint_b58: &str,
        buys_in_last_500ms: u16,
        volume_sol: f32,
        unique_buyers: u16,
        confidence: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | buys(2) | volume(4) | buyers(2) | conf(1) | timestamp(8) | padding(14)]
        let mut msg = vec![0u8; 64];
        msg[0] = 21; // MomentumDetected type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..35].copy_from_slice(&buys_in_last_500ms.to_le_bytes());
        msg[35..39].copy_from_slice(&volume_sol.to_le_bytes());
        msg[39..41].copy_from_slice(&unique_buyers.to_le_bytes());
        msg[41] = confidence.clamp(0, 100);
        msg[42..50].copy_from_slice(&timestamp_ns.to_le_bytes());
        // bytes 50-63 are padding
        
        self.send_signal(&msg)?;
        
        debug!(
            "📊 MomentumDetected: {}... | buys: {}, vol: {:.2} SOL, buyers: {}, conf: {}",
            &mint_b58[..12], buys_in_last_500ms, volume_sol, unique_buyers, confidence
        );
        
        Ok(())
    }
    
    /// Send VolumeSpike signal (Type 22)
    /// 
    /// Sent when a sudden volume increase is detected.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `total_sol` - Total SOL volume in spike
    /// * `tx_count` - Number of transactions
    /// * `time_window_ms` - Time window in milliseconds
    /// * `confidence` - Confidence score 0-100
    pub fn send_volume_spike(
        &self,
        mint_b58: &str,
        total_sol: f32,
        tx_count: u16,
        time_window_ms: u16,
        confidence: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | total_sol(4) | tx_count(2) | window(2) | conf(1) | timestamp(8) | padding(14)]
        let mut msg = vec![0u8; 64];
        msg[0] = 22; // VolumeSpike type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..37].copy_from_slice(&total_sol.to_le_bytes());
        msg[37..39].copy_from_slice(&tx_count.to_le_bytes());
        msg[39..41].copy_from_slice(&time_window_ms.to_le_bytes());
        msg[41] = confidence.clamp(0, 100);
        msg[42..50].copy_from_slice(&timestamp_ns.to_le_bytes());
        // bytes 50-63 are padding
        
        self.send_signal(&msg)?;
        
        debug!(
            "📈 VolumeSpike: {}... | {:.2} SOL in {}ms, {} txs, conf: {}",
            &mint_b58[..12], total_sol, time_window_ms, tx_count, confidence
        );
        
        Ok(())
    }
    
    /// Send WalletActivity signal (Type 23)
    /// 
    /// Sent when an alpha wallet activity is detected.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `wallet_b58` - Wallet address (base58, 32 bytes)
    /// * `action` - 0=buy, 1=sell
    /// * `size_sol` - Trade size in SOL
    /// * `wallet_tier` - Wallet tier (0=Discovery, 1=C, 2=B, 3=A)
    /// * `confidence` - Confidence score 0-100
    pub fn send_wallet_activity(
        &self,
        mint_b58: &str,
        wallet_b58: &str,
        action: u8,
        size_sol: f32,
        wallet_tier: u8,
        confidence: u8,
    ) -> Result<()> {
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
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | wallet(32) | action(1) | size(4) | tier(1) | conf(1) | timestamp(8)]
        let mut msg = vec![0u8; 80];
        msg[0] = 23; // WalletActivity type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..65].copy_from_slice(&wallet_bytes);
        msg[65] = action;
        msg[66..70].copy_from_slice(&size_sol.to_le_bytes());
        msg[70] = wallet_tier;
        msg[71] = confidence.clamp(0, 100);
        msg[72..80].copy_from_slice(&timestamp_ns.to_le_bytes());
        
        self.send_signal(&msg)?;
        
        debug!(
            "👤 WalletActivity: {}... | {}... | {} | {:.2} SOL | tier: {}, conf: {}",
            &mint_b58[..12], &wallet_b58[..12],
            if action == 0 { "BUY" } else { "SELL" },
            size_sol, wallet_tier, confidence
        );
        
        Ok(())
    }
    
    /// Send WindowMetrics signal (Type 29) - Real-time market metrics
    /// 
    /// Sent when token shows significant activity for intelligent exit timing.
    /// 
    /// # Arguments
    /// * `mint_b58` - Token mint address (base58, 32 bytes)
    /// * `volume_sol_1s` - SOL volume in last 1 second
    /// * `unique_buyers_1s` - Unique buyers in last 1 second
    /// * `price_change_bps_2s` - Price change over 2s in basis points
    /// * `alpha_wallet_hits_10s` - Alpha wallet buys in last 10 seconds
    pub fn send_window_metrics(
        &self,
        mint_b58: &str,
        volume_sol_1s: f64,
        unique_buyers_1s: u16,
        price_change_bps_2s: i16,
        alpha_wallet_hits_10s: u8,
    ) -> Result<()> {
        let mint_bytes = bs58::decode(mint_b58).into_vec()
            .context("Invalid mint base58")?;
        
        if mint_bytes.len() != 32 {
            anyhow::bail!("Mint must be 32 bytes, got {}", mint_bytes.len());
        }
        
        // Scale volume by 1000 to fit in u32
        let volume_scaled = (volume_sol_1s * 1000.0).min(u32::MAX as f64) as u32;
        
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        // Packet: [type(1) | mint(32) | volume(4) | buyers(2) | price_change(2) | alpha(1) | timestamp(8) | padding(13)]
        let mut msg = vec![0u8; 64];
        msg[0] = 29; // WindowMetrics type
        msg[1..33].copy_from_slice(&mint_bytes);
        msg[33..37].copy_from_slice(&volume_scaled.to_le_bytes());
        msg[37..39].copy_from_slice(&unique_buyers_1s.to_le_bytes());
        msg[39..41].copy_from_slice(&price_change_bps_2s.to_le_bytes());
        msg[41] = alpha_wallet_hits_10s;
        msg[42..50].copy_from_slice(&timestamp_ns.to_le_bytes());
        // msg[50..64] is padding (already zeros)
        
        self.send_signal(&msg)?;
        
        debug!(
            "📊 WindowMetrics: {}... | vol_1s: {:.2} SOL, buyers_1s: {}, Δprice_2s: {}bps, alpha_10s: {}",
            &mint_b58[..12], volume_sol_1s, unique_buyers_1s, price_change_bps_2s, alpha_wallet_hits_10s
        );
        
        Ok(())
    }
}

