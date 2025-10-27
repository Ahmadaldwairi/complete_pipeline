/// Advice Sender - Send UDP advisories to execution bot
/// 
/// This module provides functions to send advisory messages to the execution bot
/// via UDP on port 45100. Used by pump_collector and wallet_tracker to notify
/// execution bot of trading opportunities.
/// 
/// Advisory Types:
/// - Type 1: ExtendHold - Extend soft stop loss
/// - Type 2: WidenExit - Increase exit slippage for urgent exits
/// - Type 3: LateOpportunity - Hot token detected (from pump_collector)
/// - Type 4: CopyTrade - Alpha wallet bought (from wallet_tracker)
/// - Type 5: SolPriceUpdate - SOL/USD price broadcast (from copytrader)

use std::net::UdpSocket;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use log::{info, warn, debug};

/// Target host:port for execution bot UDP listener
const ADVICE_HOST: &str = "127.0.0.1:45100";

/// Advisory message types
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum AdviceType {
    ExtendHold = 1,
    WidenExit = 2,
    LateOpportunity = 3,
    CopyTrade = 4,
    SolPriceUpdate = 5,
}

/// Send raw UDP advisory packet to execution bot
/// Non-blocking: if port unavailable, fails silently
fn send_advice(packet: &[u8]) -> std::io::Result<()> {
    let sock = UdpSocket::bind("127.0.0.1:0")?;
    sock.set_write_timeout(Some(Duration::from_millis(10)))?;
    sock.send_to(packet, ADVICE_HOST)?;
    Ok(())
}

/// Send LateOpportunity advisory (Type 3)
/// Called by pump_collector when hot token detected
/// 
/// Args:
///   mint_b58: Token mint address (base58)
///   horizon_sec: Expected opportunity window in seconds (30-300)
///   score: Opportunity score 0-100 (based on volume, buys, ratio)
/// 
/// Example:
///   send_late_opportunity("7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU", 60, 85);
pub fn send_late_opportunity(mint_b58: &str, horizon_sec: u16, score: u8) {
    match bs58::decode(mint_b58).into_vec() {
        Ok(mint_bytes) if mint_bytes.len() == 32 => {
            let mut msg = vec![0u8; 64];
            msg[0] = AdviceType::LateOpportunity as u8;
            msg[1..33].copy_from_slice(&mint_bytes);
            msg[33..35].copy_from_slice(&horizon_sec.to_le_bytes());
            msg[35] = score.clamp(0, 100);
            // Bytes 36-63 already zero (padding)
            
            match send_advice(&msg) {
                Ok(_) => {
                    info!("📤 Sent LateOpportunity: {} | horizon: {}s | score: {}", 
                          &mint_b58[..8], horizon_sec, score);
                }
                Err(e) => {
                    debug!("⚠️  Failed to send LateOpportunity: {}", e);
                }
            }
        }
        _ => {
            warn!("⚠️  Invalid mint address for LateOpportunity: {}", mint_b58);
        }
    }
}

/// Send CopyTrade advisory (Type 4)
/// Called by wallet_tracker when tracked/discovered wallet buys
/// 
/// Args:
///   mint_b58: Token mint address (base58)
///   wallet_b58: Wallet address that bought (base58)
///   confidence: Confidence score 0-100 (based on wallet tier/WR)
/// 
/// Confidence tiers:
///   90-95: Tier A (WR≥60%, PnL≥100 SOL)
///   85-89: Tier B (WR≥55%, PnL≥40 SOL)
///   75-84: Tier C (WR≥50%, PnL≥15 SOL)
/// 
/// Example:
///   send_copy_trade("7xKXtg...", "9AB2cD...", 90);
pub fn send_copy_trade(mint_b58: &str, wallet_b58: &str, confidence: u8) {
    match (bs58::decode(mint_b58).into_vec(), bs58::decode(wallet_b58).into_vec()) {
        (Ok(mint_bytes), Ok(wallet_bytes)) if mint_bytes.len() == 32 && wallet_bytes.len() == 32 => {
            let mut msg = vec![0u8; 96];
            msg[0] = AdviceType::CopyTrade as u8;
            msg[1..33].copy_from_slice(&mint_bytes);
            msg[33..65].copy_from_slice(&wallet_bytes);
            msg[65] = confidence.clamp(0, 100);
            // Bytes 66-95 already zero (padding)
            
            match send_advice(&msg) {
                Ok(_) => {
                    info!("📤 Sent CopyTrade: {} | wallet: {} | confidence: {}%", 
                          &mint_b58[..8], &wallet_b58[..8], confidence);
                }
                Err(e) => {
                    debug!("⚠️  Failed to send CopyTrade: {}", e);
                }
            }
        }
        _ => {
            warn!("⚠️  Invalid addresses for CopyTrade: mint={}, wallet={}", mint_b58, wallet_b58);
        }
    }
}

/// Send ExtendHold advisory (Type 1)
/// Called by wallet_tracker when volume surge detected on active position
/// 
/// Args:
///   mint_b58: Token mint address (base58)
///   extra_secs: Additional seconds to hold (5-60)
///   confidence: Confidence score 0-100 (based on volume/wallet activity)
/// 
/// Example:
///   send_extend_hold("7xKXtg...", 20, 85);
pub fn send_extend_hold(mint_b58: &str, extra_secs: u16, confidence: u8) {
    match bs58::decode(mint_b58).into_vec() {
        Ok(mint_bytes) if mint_bytes.len() == 32 => {
            let mut msg = vec![0u8; 64];
            msg[0] = AdviceType::ExtendHold as u8;
            msg[1..33].copy_from_slice(&mint_bytes);
            msg[33..35].copy_from_slice(&extra_secs.to_le_bytes());
            msg[35] = confidence.clamp(0, 100);
            // Bytes 36-63 already zero (padding)
            
            match send_advice(&msg) {
                Ok(_) => {
                    info!("📤 Sent ExtendHold: {} | +{}s | confidence: {}%", 
                          &mint_b58[..8], extra_secs, confidence);
                }
                Err(e) => {
                    debug!("⚠️  Failed to send ExtendHold: {}", e);
                }
            }
        }
        _ => {
            warn!("⚠️  Invalid mint address for ExtendHold: {}", mint_b58);
        }
    }
}

/// Send WidenExit advisory (Type 2)
/// Called by wallet_tracker when high-confidence wallet dumps
/// 
/// Args:
///   mint_b58: Token mint address (base58)
///   sell_slip_bps: Slippage in basis points (e.g., 1000 = 10%)
///   ttl_ms: Time-to-live in milliseconds (how long to keep wider slippage)
///   confidence: Confidence score 0-100
/// 
/// Example:
///   send_widen_exit("7xKXtg...", 1000, 1500, 90);
pub fn send_widen_exit(mint_b58: &str, sell_slip_bps: u16, ttl_ms: u16, confidence: u8) {
    match bs58::decode(mint_b58).into_vec() {
        Ok(mint_bytes) if mint_bytes.len() == 32 => {
            let mut msg = vec![0u8; 64];
            msg[0] = AdviceType::WidenExit as u8;
            msg[1..33].copy_from_slice(&mint_bytes);
            msg[33..35].copy_from_slice(&sell_slip_bps.to_le_bytes());
            msg[35..37].copy_from_slice(&ttl_ms.to_le_bytes());
            msg[37] = confidence.clamp(0, 100);
            // Bytes 38-63 already zero (padding)
            
            match send_advice(&msg) {
                Ok(_) => {
                    info!("📤 Sent WidenExit: {} | slip: {}bps | ttl: {}ms | confidence: {}%", 
                          &mint_b58[..8], sell_slip_bps, ttl_ms, confidence);
                }
                Err(e) => {
                    debug!("⚠️  Failed to send WidenExit: {}", e);
                }
            }
        }
        _ => {
            warn!("⚠️  Invalid mint address for WidenExit: {}", mint_b58);
        }
    }
}

/// Send SolPriceUpdate advisory (Type 5)
/// Called by copytrader to broadcast current SOL/USD price
/// 
/// Args:
///   price_usd: SOL price in USD (e.g., 182.83)
///   source: 1=Helius, 2=Jupiter, 3=Fallback
/// 
/// Example:
///   send_sol_price_update(182.83, 1);
pub fn send_sol_price_update(price_usd: f64, source: u8) {
    let price_cents = (price_usd * 100.0) as u32;
    let timestamp_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    
    let mut msg = vec![0u8; 64];
    msg[0] = AdviceType::SolPriceUpdate as u8;
    msg[1..5].copy_from_slice(&price_cents.to_le_bytes());
    msg[5..9].copy_from_slice(&timestamp_secs.to_le_bytes());
    msg[9] = source.clamp(1, 3);
    // Bytes 10-63 already zero (padding)
    
    match send_advice(&msg) {
        Ok(_) => {
            let source_name = match source {
                1 => "Helius",
                2 => "Jupiter",
                _ => "Fallback",
            };
            info!("📤 Sent SOL Price: ${:.2} from {} (timestamp: {})", 
                  price_usd, source_name, timestamp_secs);
        }
        Err(e) => {
            debug!("⚠️  Failed to send SOL price: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_late_opportunity_encoding() {
        // Test with valid mint address
        let mint = "So11111111111111111111111111111111111111112";
        send_late_opportunity(mint, 60, 85);
        // Should log success (check manually)
    }

    #[test]
    fn test_copy_trade_encoding() {
        let mint = "So11111111111111111111111111111111111111112";
        let wallet = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
        send_copy_trade(mint, wallet, 90);
        // Should log success (check manually)
    }

    #[test]
    fn test_sol_price_update() {
        send_sol_price_update(182.83, 1);
        // Should log success (check manually)
    }
}
