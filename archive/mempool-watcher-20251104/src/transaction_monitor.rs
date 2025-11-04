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

        let mut reconnect_delay = 2; // Start with 2 seconds
        const MAX_RECONNECT_DELAY: u64 = 60; // Cap at 60 seconds

        loop {
            match self.connect_and_monitor().await {
                Ok(_) => {
                    warn!("WebSocket connection closed cleanly, reconnecting...");
                    // Reset delay on clean disconnect
                    reconnect_delay = 2;
                }
                Err(e) => {
                    error!("❌ WebSocket error: {} - Reconnecting in {}s...", e, reconnect_delay);
                    
                    // Exponential backoff: 2s → 4s → 8s → 16s → 32s → 60s (capped)
                    tokio::time::sleep(tokio::time::Duration::from_secs(reconnect_delay)).await;
                    
                    reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
                    
                    info!("⚡ Attempting reconnection (next delay: {}s if failed)", reconnect_delay);
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
        let mut last_ping = tokio::time::Instant::now();
        const PING_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_secs(30);
        
        while let Some(message) = read.next().await {
            match message {
                Ok(Message::Text(text)) => {
                    if let Err(e) = self.process_message(&text).await {
                        debug!("Failed to process message: {}", e);
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("📶 Received ping, sending pong");
                    if let Err(e) = write.send(Message::Pong(data)).await {
                        error!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("📶 Received pong - connection alive");
                    last_ping = tokio::time::Instant::now();
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

            // Send periodic ping to keep connection alive
            if last_ping.elapsed() > PING_INTERVAL {
                if let Err(e) = write.send(Message::Ping(vec![])).await {
                    error!("Failed to send ping: {}", e);
                    break;
                }
                last_ping = tokio::time::Instant::now();
                debug!("📶 Sent ping to keep connection alive");
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

        debug!("📨 Log notification: {} | logs: {}", &signature[..12], logs.len());

        // Check if it's a Pump.fun buy/sell
        let is_pump_buy = logs.iter().any(|log| {
            log.contains("Instruction: Buy") || log.contains("buy")
        });

        let is_pump_sell = logs.iter().any(|log| {
            log.contains("Instruction: Sell") || log.contains("sell")
        });

        if is_pump_buy || is_pump_sell {
            let action = if is_pump_buy { "BUY" } else { "SELL" };
            info!("🔥 Detected Pump.fun {}: {} (fetching full tx...)", action, &signature[..12]);

            // Fetch full transaction data via RPC for proper processing
            match Self::fetch_transaction(signature).await {
                Ok(raw_tx) => {
                    info!("✅ Fetched full tx: {} | {} accounts, {} data bytes", 
                           &signature[..12], raw_tx.accounts.len(), raw_tx.data.len());
                    
                    // Send to processing channel
                    if let Err(e) = self.tx_sender.send(raw_tx) {
                        error!("Failed to send transaction to channel: {}", e);
                    } else {
                        debug!("📤 Sent tx to processing channel: {}", &signature[..12]);
                    }
                }
                Err(e) => {
                    warn!("⚠️  Failed to fetch transaction {}: {} - using fallback", &signature[..12], e);
                    // Fallback: send minimal transaction data
                    let raw_tx = RawTransaction {
                        signature: signature.to_string(),
                        slot: result.get("context").and_then(|c| c.get("slot")).and_then(|s| s.as_u64()).unwrap_or(0),
                        timestamp: chrono::Utc::now().timestamp(),
                        accounts: vec![],
                        data: vec![],
                        program_id: PUMP_FUN_PROGRAM.to_string(),
                    };
                    let _ = self.tx_sender.send(raw_tx);
                }
            }
        }

        Ok(())
    }
    
    /// Fetch full transaction data from RPC
    async fn fetch_transaction(signature: &str) -> Result<RawTransaction> {
        use solana_client::rpc_client::RpcClient;
        use solana_sdk::commitment_config::CommitmentConfig;
        use solana_transaction_status::UiTransactionEncoding;
        
        let rpc_url = std::env::var("SOLANA_RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
        
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        );
        
        let sig_parsed = signature.parse()
            .context("Failed to parse signature")?;
        
        let tx = rpc_client.get_transaction_with_config(
            &sig_parsed,
            solana_client::rpc_config::RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        ).context("Failed to fetch transaction")?;
        
        // Extract accounts and instruction data
        let mut accounts = Vec::new();
        let mut data = Vec::new();
        let mut program_id = PUMP_FUN_PROGRAM.to_string();
        
        if let solana_transaction_status::EncodedTransaction::Json(ui_tx) = tx.transaction.transaction {
            if let solana_transaction_status::UiMessage::Parsed(parsed) = ui_tx.message {
                // Extract account keys
                for key in parsed.account_keys {
                    accounts.push(key.pubkey);
                }
                
                // Extract instruction data from first instruction
                if let Some(first_ix) = parsed.instructions.first() {
                    if let solana_transaction_status::UiInstruction::Compiled(compiled) = first_ix {
                        if let Ok(decoded) = bs58::decode(&compiled.data).into_vec() {
                            data = decoded;
                        }
                        // Get program ID from program_id_index (u8)
                        let prog_idx = compiled.program_id_index as usize;
                        if let Some(prog_key) = accounts.get(prog_idx) {
                            program_id = prog_key.clone();
                        }
                    }
                }
            }
        }
        
        Ok(RawTransaction {
            signature: signature.to_string(),
            slot: tx.slot,
            timestamp: tx.block_time.unwrap_or(chrono::Utc::now().timestamp()),
            accounts,
            data,
            program_id,
        })
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
