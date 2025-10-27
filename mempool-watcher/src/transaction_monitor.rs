//! 🎯 Transaction Monitor - WebSocket Mempool Watcher
//! 
//! Monitors Solana mempool via WebSocket for Pump.fun transactions
//! Extracts hot signals (whale buys, volume spikes) and publishes to executor

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;

const PUMP_FUN_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const RAYDIUM_PROGRAM: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

/// Raw transaction data from WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawTransaction {
    pub signature: String,
    pub slot: u64,
    pub timestamp: i64,
    pub accounts: Vec<String>,
    pub data: Vec<u8>,
    pub program_id: String,
}

/// Transaction monitor - watches mempool for new transactions
pub struct TransactionMonitor {
    ws_url: String,
    tx_sender: mpsc::UnboundedSender<RawTransaction>,
}

impl TransactionMonitor {
    pub fn new(ws_url: String) -> (Self, mpsc::UnboundedReceiver<RawTransaction>) {
        let (tx_sender, tx_receiver) = mpsc::unbounded_channel();
        
        (
            Self {
                ws_url,
                tx_sender,
            },
            tx_receiver,
        )
    }

    /// Start monitoring mempool via WebSocket
    pub async fn start_monitoring(&self) -> Result<()> {
        info!("🎯 Starting mempool monitoring...");
        info!("🌊 WebSocket: {}", self.ws_url);

        loop {
            match self.connect_and_monitor().await {
                Ok(_) => {
                    warn!("WebSocket connection closed cleanly, reconnecting...");
                }
                Err(e) => {
                    error!("❌ WebSocket error: {} - Reconnecting in 5s...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// Connect to WebSocket and monitor transactions
    async fn connect_and_monitor(&self) -> Result<()> {
        // Connect to WebSocket
        let (ws_stream, _) = connect_async(&self.ws_url)
            .await
            .context("Failed to connect to WebSocket")?;

        info!("✅ Connected to Solana WebSocket");

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to Pump.fun program logs
        let subscribe_msg = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "logsSubscribe",
            "params": [
                {
                    "mentions": [PUMP_FUN_PROGRAM]
                },
                {
                    "commitment": "confirmed"
                }
            ]
        });

        write
            .send(Message::Text(subscribe_msg.to_string()))
            .await
            .context("Failed to send subscription")?;

        info!("📡 Subscribed to Pump.fun program logs");

        // Also subscribe to Raydium (optional)
        let subscribe_raydium = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "logsSubscribe",
            "params": [
                {
                    "mentions": [RAYDIUM_PROGRAM]
                },
                {
                    "commitment": "confirmed"
                }
            ]
        });

        write
            .send(Message::Text(subscribe_raydium.to_string()))
            .await
            .context("Failed to send Raydium subscription")?;

        info!("📡 Subscribed to Raydium program logs");

        // Process incoming messages
        while let Some(message) = read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.process_message(&text).await {
                        debug!("Failed to process message: {}", e);
                    }
                }
                Ok(Message::Ping(data)) => {
                    if let Err(e) = write.send(Message::Pong(data)).await {
                        error!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket closed by server");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Process incoming WebSocket message
    async fn process_message(&self, text: &str) -> Result<()> {
        let value: serde_json::Value = serde_json::from_str(text)?;

        // Check if it's a notification (not a subscription confirmation)
        if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
            if method == "logsNotification" {
                if let Some(params) = value.get("params") {
                    if let Some(result) = params.get("result") {
                        self.handle_log_notification(result).await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle log notification from WebSocket
    async fn handle_log_notification(&self, result: &serde_json::Value) -> Result<()> {
        // Extract signature
        let signature = result
            .get("value")
            .and_then(|v| v.get("signature"))
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        // Extract logs
        let logs = result
            .get("value")
            .and_then(|v| v.get("logs"))
            .and_then(|l| l.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Check if it's a Pump.fun buy/sell
        let is_pump_buy = logs.iter().any(|log| {
            log.contains("Instruction: Buy") || log.contains("buy")
        });

        let is_pump_sell = logs.iter().any(|log| {
            log.contains("Instruction: Sell") || log.contains("sell")
        });

        if is_pump_buy || is_pump_sell {
            let action = if is_pump_buy { "BUY" } else { "SELL" };
            debug!("🔥 Detected Pump.fun {}: {}", action, &signature[..12]);

            // Create raw transaction (simplified - in production, fetch full tx)
            let raw_tx = RawTransaction {
                signature: signature.to_string(),
                slot: result
                    .get("context")
                    .and_then(|c| c.get("slot"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0),
                timestamp: chrono::Utc::now().timestamp(),
                accounts: vec![], // Would be populated from full tx
                data: vec![],     // Would be populated from full tx
                program_id: PUMP_FUN_PROGRAM.to_string(),
            };

            // Send to processing channel
            if let Err(e) = self.tx_sender.send(raw_tx) {
                error!("Failed to send transaction to channel: {}", e);
            }
        }

        Ok(())
    }

    /// Subscribe to specific program accounts (alternative method)
    pub async fn subscribe_to_account_updates(&self) -> Result<()> {
        // TODO: Implement account subscription for more granular monitoring
        warn!("⚠️  Account subscription not yet implemented");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_creation() {
        let (monitor, _rx) = TransactionMonitor::new(
            "wss://api.mainnet-beta.solana.com".to_string(),
        );
        
        assert_eq!(monitor.ws_url, "wss://api.mainnet-beta.solana.com");
    }

    #[test]
    fn test_pump_program_constant() {
        assert_eq!(PUMP_FUN_PROGRAM, "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
    }
}
