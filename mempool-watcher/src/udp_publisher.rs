use crate::heat_calculator::{HeatIndex, HotSignal};
use anyhow::Result;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::net::UdpSocket;

/// UDP publisher - sends heat updates and hot signals
pub struct UdpPublisher {
    brain_socket: UdpSocket,
    executor_socket: UdpSocket,
    brain_addr: String,
    executor_addr: String,
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
        executor_port: u16,
    ) -> Result<Self> {
        // Create separate sockets for Brain and Executor
        let brain_socket = UdpSocket::bind(format!("{}:0", bind_address))?;
        brain_socket.set_nonblocking(true)?;

        let executor_socket = UdpSocket::bind(format!("{}:0", bind_address))?;
        executor_socket.set_nonblocking(true)?;

        let brain_addr = format!("{}:{}", bind_address, brain_port);
        let executor_addr = format!("{}:{}", bind_address, executor_port);

        Ok(Self {
            brain_socket,
            executor_socket,
            brain_addr,
            executor_addr,
        })
    }

    /// Send heat index to Brain for decision context
    pub fn send_heat_to_brain(&self, heat: &HeatIndex) -> Result<()> {
        let message = MempoolHeatMessage {
            heat_score: heat.score,
            tx_rate: heat.tx_rate,
            whale_activity: heat.whale_activity,
            bot_density: heat.bot_density,
            timestamp: heat.timestamp,
        };

        let serialized = bincode::serialize(&message)?;
        
        match self.brain_socket.send_to(&serialized, &self.brain_addr) {
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

    /// Send hot signal to Executor for immediate action
    pub fn send_hot_signal_to_executor(&self, signal: &HotSignal) -> Result<()> {
        let message = HotSignalMessage {
            mint: signal.mint.clone(),
            whale_wallet: signal.whale_wallet.clone(),
            amount_sol: signal.amount_sol,
            action: signal.action.clone(),
            urgency: signal.urgency,
            timestamp: signal.timestamp,
        };

        let serialized = bincode::serialize(&message)?;
        
        match self.executor_socket.send_to(&serialized, &self.executor_addr) {
            Ok(bytes) => {
                debug!("🔥 Sent hot signal to Executor: {} bytes (urgency: {})", 
                       bytes, signal.urgency);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to send hot signal to Executor: {}", e);
                Err(e.into())
            }
        }
    }

    /// Batch send hot signals (avoid overwhelming executor)
    pub fn send_hot_signals_batch(&self, signals: &[HotSignal]) -> Result<usize> {
        let mut sent_count = 0;

        for signal in signals {
            if self.send_hot_signal_to_executor(signal).is_ok() {
                sent_count += 1;
            }
        }

        Ok(sent_count)
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
