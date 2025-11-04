use crate::heat_calculator::{HeatIndex, HotSignal};
use crate::position_update::PositionUpdate;
use crate::exit_advice::ExitAdvice;
use anyhow::Result;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::net::UdpSocket;
use std::time::Duration;
use tokio::time::sleep;

/// UDP publisher - sends heat updates and hot signals
pub struct UdpPublisher {
    brain_socket: UdpSocket,
    brain_confirmation_socket: UdpSocket,
    brain_addr: String,
    brain_confirmation_addr: String,
}

/// Message sent to Brain (heat context for decisions)
#[derive(Debug, Serialize, Deserialize)]
pub struct MempoolHeatMessage {
    pub heat_score: u8,
    pub tx_rate: f64,
    pub whale_activity: f64,
    pub bot_density: f64,
    pub timestamp: u64,
}

/// Message sent to Executor (hot frontrunning opportunity)
#[derive(Debug, Serialize, Deserialize)]
pub struct HotSignalMessage {
    pub mint: String,
    pub whale_wallet: String,
    pub amount_sol: f64,
    pub action: String,
    pub urgency: u8,
    pub timestamp: u64,
}

impl UdpPublisher {
    pub fn new(
        bind_address: &str,
        brain_port: u16,
        brain_confirmation_port: u16,
    ) -> Result<Self> {
        // Create separate sockets for Brain (price updates) and Brain (confirmations/hot signals)
        let brain_socket = UdpSocket::bind(format!("{}:0", bind_address))?;
        brain_socket.set_nonblocking(true)?;

        let brain_confirmation_socket = UdpSocket::bind(format!("{}:0", bind_address))?;
        brain_confirmation_socket.set_nonblocking(true)?;

        let brain_addr = format!("{}:{}", bind_address, brain_port);
        let brain_confirmation_addr = format!("{}:{}", bind_address, brain_confirmation_port);

        Ok(Self {
            brain_socket,
            brain_confirmation_socket,
            brain_addr,
            brain_confirmation_addr,
        })
    }

    /// Send heat index to Brain for decision context
    pub fn send_heat_to_brain(&self, heat: &HeatIndex) -> Result<()> {
        // Build binary message (24 bytes total, matching MempoolHeatAdvice::SIZE)
        // Format: [msg_type(1), heat_score(1), tx_rate(2), whale_activity(2), 
        //          bot_density(2), timestamp(8), padding(6)]
        let mut buf = [0u8; 24];
        
        buf[0] = 17; // MempoolHeat message type
        buf[1] = heat.score;
        
        // Scale floats to fit in u16
        let tx_rate_scaled = (heat.tx_rate * 100.0).min(65535.0) as u16;
        let whale_activity_scaled = (heat.whale_activity * 100.0).min(65535.0) as u16;
        let bot_density_scaled = (heat.bot_density * 10000.0).min(65535.0) as u16;
        
        buf[2..4].copy_from_slice(&tx_rate_scaled.to_le_bytes());
        buf[4..6].copy_from_slice(&whale_activity_scaled.to_le_bytes());
        buf[6..8].copy_from_slice(&bot_density_scaled.to_le_bytes());
        buf[8..16].copy_from_slice(&heat.timestamp.to_le_bytes());
        // buf[16..22] remains zeros (padding)
        
        match self.brain_socket.send_to(&buf, &self.brain_addr) {
            Ok(bytes) => {
                debug!("📤 Sent heat to Brain: {} bytes (score: {})", bytes, heat.score);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to send heat to Brain: {}", e);
                Err(e.into())
            }
        }
    }

    /// Send hot signal to Brain for strategic decision-making (with anti-collision jitter)
    pub async fn send_hot_signal_to_brain(&self, signal: &HotSignal) -> Result<()> {
        // Add 1-3ms random jitter to prevent UDP burst collisions
        let jitter_ms = {
            use rand::Rng;
            rand::thread_rng().gen_range(1..=3)
        };
        sleep(Duration::from_millis(jitter_ms)).await;

        let message = HotSignalMessage {
            mint: signal.mint.clone(),
            whale_wallet: signal.whale_wallet.clone(),
            amount_sol: signal.amount_sol,
            action: signal.action.clone(),
            urgency: signal.urgency,
            timestamp: signal.timestamp,
        };

        let serialized = bincode::serialize(&message)?;
        
        match self.brain_confirmation_socket.send_to(&serialized, &self.brain_confirmation_addr) {
            Ok(bytes) => {
                debug!("🔥 Sent hot signal to Brain (45131): {} bytes (urgency: {}, jitter: {}ms)", 
                       bytes, signal.urgency, jitter_ms);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to send hot signal to Brain: {}", e);
                Err(e.into())
            }
        }
    }

    /// Batch send hot signals (avoid overwhelming brain, with jitter)
    pub async fn send_hot_signals_batch(&self, signals: &[HotSignal]) -> Result<usize> {
        let mut sent_count = 0;

        for signal in signals {
            if self.send_hot_signal_to_brain(signal).await.is_ok() {
                sent_count += 1;
            }
        }

        Ok(sent_count)
    }
    
    /// Send position update to Brain for exit decision-making
    pub fn send_position_update(&self, update: &PositionUpdate) -> Result<()> {
        let bytes = update.to_bytes();
        
        // Copy packed fields to avoid unaligned references
        let pnl_usd = update.realized_pnl_usd;
        let pnl_pct = update.pnl_percent;
        
        match self.brain_confirmation_socket.send_to(&bytes, &self.brain_confirmation_addr) {
            Ok(sent) => {
                debug!("📤 Sent PositionUpdate to Brain: {} bytes | mint: {} | P&L: ${:.2} ({:.1}%)",
                    sent, &update.mint_str()[..8], pnl_usd, pnl_pct);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to send PositionUpdate to Brain: {}", e);
                Err(e.into())
            }
        }
    }
    
    /// Send ManualExitNotification to Executor and Brain
    pub fn send_manual_exit(&self, notification: &crate::manual_exit::ManualExitNotification) -> Result<()> {
        let bytes = notification.to_bytes();
        
        // Copy packed fields to avoid alignment issues
        let pnl_usd = notification.realized_pnl_usd;
        let pnl_pct = notification.pnl_percent;
        
        // Send to Executor (port 45134) - for Telegram notification
        let executor_addr = "127.0.0.1:45134";
        match self.brain_socket.send_to(&bytes, executor_addr) {
            Ok(_) => {
                debug!("📤 Sent ManualExitNotification to Executor: mint: {} | P&L: ${:.2} ({:.1}%)",
                    &notification.mint_str()[..8], pnl_usd, pnl_pct);
            }
            Err(e) => {
                error!("❌ Failed to send ManualExitNotification to Executor: {}", e);
            }
        }
        
        // Send to Brain (port 45135) - for position cleanup
        let brain_cleanup_addr = "127.0.0.1:45135";
        match self.brain_confirmation_socket.send_to(&bytes, brain_cleanup_addr) {
            Ok(_) => {
                debug!("📤 Sent ManualExitNotification to Brain: mint: {} | P&L: ${:.2} ({:.1}%)",
                    &notification.mint_str()[..8], pnl_usd, pnl_pct);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to send ManualExitNotification to Brain: {}", e);
                Err(e.into())
            }
        }
    }
    
    /// Send ExitAdvice to Brain (profit target or stop loss hit)
    pub fn send_exit_advice(&self, advice: &ExitAdvice) -> Result<()> {
        let bytes = advice.to_bytes();
        
        // Send to Brain (port 45115) - Brain's confirmation listener
        let brain_advice_addr = "127.0.0.1:45115";
        match self.brain_confirmation_socket.send_to(&bytes, brain_advice_addr) {
            Ok(_) => {
                debug!("📤 Sent ExitAdvice to Brain: {} | P&L: ${:.2} | confidence: {}",
                    advice.reason_str(), advice.realized_pnl_usd(), advice.confidence);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to send ExitAdvice to Brain: {}", e);
                Err(e.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_publisher_creation() {
        let publisher = UdpPublisher::new("127.0.0.1", 45120, 45130);
        assert!(publisher.is_ok());
    }

    #[test]
    fn test_message_serialization() {
        let heat_msg = MempoolHeatMessage {
            heat_score: 75,
            tx_rate: 5.5,
            whale_activity: 120.5,
            bot_density: 30.0,
            timestamp: 1234567890,
        };

        let serialized = bincode::serialize(&heat_msg);
        assert!(serialized.is_ok());

        let deserialized: Result<MempoolHeatMessage, _> = bincode::deserialize(&serialized.unwrap());
        assert!(deserialized.is_ok());
    }
}
