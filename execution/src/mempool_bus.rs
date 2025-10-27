// ============================================================================
// MEMPOOL BUS - Receives Hot Signals from Mempool Watcher
// ============================================================================
// Port 45130: Mempool Watcher → Executor (hot frontrunning opportunities)

use serde::{Deserialize, Serialize};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use log::{debug, error, warn};

/// Hot signal from mempool watcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotSignalMessage {
    pub mint: String,
    pub whale_wallet: String,
    pub amount_sol: f64,
    pub action: String,
    pub urgency: u8,  // 0-100
    pub timestamp: u64,
}

/// Mempool bus listener (receives hot signals)
pub struct MempoolBusListener {
    socket: Arc<Mutex<UdpSocket>>,
    buffer_size: usize,
}

impl MempoolBusListener {
    /// Create new mempool bus listener on port 45130
    pub fn new(port: u16) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let socket = UdpSocket::bind(format!("0.0.0.0:{}", port))?;
        socket.set_nonblocking(true)?;
        
        Ok(Self {
            socket: Arc::new(Mutex::new(socket)),
            buffer_size: 8192,  // 8KB buffer
        })
    }
    
    /// Try to receive a hot signal (non-blocking)
    pub fn try_recv(&self) -> Option<HotSignalMessage> {
        let mut buffer = vec![0u8; self.buffer_size];
        
        let socket = self.socket.lock().unwrap();
        match socket.recv_from(&mut buffer) {
            Ok((size, _addr)) => {
                // Deserialize the message
                match bincode::deserialize::<HotSignalMessage>(&buffer[..size]) {
                    Ok(signal) => {
                        debug!("🔥 Received hot signal: {} (urgency: {}, SOL: {:.4})", 
                               &signal.mint[..8], signal.urgency, signal.amount_sol);
                        Some(signal)
                    }
                    Err(e) => {
                        error!("❌ Failed to deserialize hot signal: {}", e);
                        None
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available (expected in non-blocking mode)
                None
            }
            Err(e) => {
                warn!("⚠️  Mempool bus recv error: {}", e);
                None
            }
        }
    }
    
    /// Get socket address
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
        let socket = self.socket.lock().unwrap();
        socket.local_addr()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mempool_bus_creation() {
        // Should be able to create listener
        let listener = MempoolBusListener::new(0); // Use port 0 for OS-assigned port
        assert!(listener.is_ok());
    }
    
    #[test]
    fn test_try_recv_no_data() {
        let listener = MempoolBusListener::new(0).unwrap();
        // Should return None when no data available
        assert!(listener.try_recv().is_none());
    }
}
