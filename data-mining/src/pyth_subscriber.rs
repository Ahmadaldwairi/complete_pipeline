//! 🔮 Pyth Oracle SOL/USD Price Subscriber
//! 
//! Subscribes to Pyth price oracle account via Yellowstone gRPC and broadcasts
//! real-time SOL/USD price updates via UDP to Brain (45100) and Executor (45110).
//! 
//! This eliminates HTTP dependency for price data and provides sub-second updates.

use anyhow::{Context, Result};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{interval, Duration};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterAccounts,
};

/// Pyth SOL/USD Price Feed Account
/// Mainnet: H6ARHf6YoNHUXJU8HSwZbE5E8XhHMR8N3dJYBYQy3Cz (SOL/USD)
const PYTH_SOL_USD_FEED: &str = "H6ARHf6YoNAfHp2rGQTqSXRfxiAqoFvkVZoxMdVpZGgr";

/// Broadcast interval (20 seconds as per requirements)
const BROADCAST_INTERVAL_SECS: u64 = 20;

/// UDP ports for broadcasting price updates
const BRAIN_UDP_PORT: u16 = 45100;
const EXECUTOR_UDP_PORT: u16 = 45110;

/// Message type for SolPriceUpdate (matching brain/src/udp_bus/messages.rs)
const SOL_PRICE_UPDATE_MSG_TYPE: u8 = 14;

/// Pyth price source identifier
const PYTH_SOURCE: u8 = 1;

pub struct PythSubscriber {
    grpc_endpoint: String,
    udp_socket: UdpSocket,
    pyth_feed_pubkey: Pubkey,
    brain_addr: String,
    executor_addr: String,
}

impl PythSubscriber {
    /// Create new Pyth subscriber
    pub fn new(grpc_endpoint: String) -> Result<Self> {
        // Non-blocking UDP socket for broadcasting
        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket for Pyth price broadcast")?;
        udp_socket.set_nonblocking(true)
            .context("Failed to set UDP socket to non-blocking")?;

        let pyth_feed_pubkey = Pubkey::from_str(PYTH_SOL_USD_FEED)
            .context("Invalid Pyth feed pubkey")?;

        Ok(Self {
            grpc_endpoint,
            udp_socket,
            pyth_feed_pubkey,
            brain_addr: format!("127.0.0.1:{}", BRAIN_UDP_PORT),
            executor_addr: format!("127.0.0.1:{}", EXECUTOR_UDP_PORT),
        })
    }

    /// Start Pyth price subscription with periodic broadcasts
    pub async fn run(&self) -> Result<()> {
        info!("🔮 Starting Pyth SOL/USD Price Subscriber");
        info!("   📡 Feed: {}", PYTH_SOL_USD_FEED);
        info!("   🎯 Broadcast to: Brain ({}), Executor ({})", 
            self.brain_addr, self.executor_addr);

        // Connect to gRPC
        let mut client = GeyserGrpcClient::build_from_shared(self.grpc_endpoint.clone())
            .context("Failed to build gRPC client")?
            .connect()
            .await
            .context("Failed to connect to gRPC")?;

        info!("✅ Connected to Yellowstone gRPC: {}", self.grpc_endpoint);

        // Create subscription request for Pyth account
        let mut accounts = HashMap::new();
        accounts.insert(
            "pyth_sol_usd".to_string(),
            SubscribeRequestFilterAccounts {
                account: vec![self.pyth_feed_pubkey.to_string()],
                owner: vec![],
                filters: vec![],
                nonempty_txn_signature: None,
            },
        );

        let subscribe_request = SubscribeRequest {
            accounts,
            slots: HashMap::new(),
            transactions: HashMap::new(),
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };

        // Subscribe to account updates
        let (_subscribe_tx, mut stream) = client
            .subscribe_with_request(Some(subscribe_request))
            .await
            .context("Failed to create subscription")?;

        info!("✅ Subscribed to Pyth SOL/USD feed");

        // Track latest price for periodic broadcasts
        let mut latest_price: Option<f32> = None;
        let mut broadcast_interval = interval(Duration::from_secs(BROADCAST_INTERVAL_SECS));

        loop {
            tokio::select! {
                // Process gRPC updates (price changes)
                Some(update_result) = stream.next() => {
                    match update_result {
                        Ok(update) => {
                            if let Some(update_oneof) = update.update_oneof {
                                if let UpdateOneof::Account(account_update) = update_oneof {
                                    // Parse Pyth price from account data
                                    if let Some(price) = self.parse_pyth_price(&account_update.account.unwrap().data) {
                                        latest_price = Some(price);
                                        
                                        // Broadcast immediately on price change
                                        if let Err(e) = self.broadcast_price(price) {
                                            warn!("Failed to broadcast price update: {}", e);
                                        } else {
                                            info!("📊 SOL/USD: ${:.4} (Pyth)", price);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            return Err(anyhow::anyhow!("gRPC stream error: {}", e));
                        }
                    }
                }
                
                // Periodic broadcast (every 20s) even if price hasn't changed
                _ = broadcast_interval.tick() => {
                    if let Some(price) = latest_price {
                        if let Err(e) = self.broadcast_price(price) {
                            warn!("Failed periodic broadcast: {}", e);
                        } else {
                            info!("🔄 Periodic SOL/USD broadcast: ${:.4}", price);
                        }
                    } else {
                        warn!("⏸️  No Pyth price available yet for periodic broadcast");
                    }
                }
            }
        }
    }

    /// Parse Pyth price from account data
    /// Pyth price format: https://docs.pyth.network/price-feeds/on-chain-price-feeds/solana
    fn parse_pyth_price(&self, data: &[u8]) -> Option<f32> {
        // Pyth V2 account layout (simplified):
        // - Bytes 0-4: Magic number
        // - Bytes 4-8: Version
        // - Bytes 8-12: Account type
        // - Bytes 208-216: Price (i64)
        // - Bytes 232-236: Exponent (i32)
        
        if data.len() < 240 {
            warn!("Pyth account data too short: {} bytes", data.len());
            return None;
        }

        // Extract price (i64 at offset 208)
        let price_i64 = i64::from_le_bytes([
            data[208], data[209], data[210], data[211],
            data[212], data[213], data[214], data[215],
        ]);

        // Extract exponent (i32 at offset 232)
        let exponent = i32::from_le_bytes([
            data[232], data[233], data[234], data[235],
        ]);

        // Calculate actual price: price * 10^exponent
        // Example: price=24523456, exp=-6 → 24.523456 USD
        let price_usd = (price_i64 as f64) * 10_f64.powi(exponent);

        if price_usd <= 0.0 || price_usd > 10000.0 {
            warn!("Invalid Pyth price: {} (raw={}, exp={})", price_usd, price_i64, exponent);
            return None;
        }

        Some(price_usd as f32)
    }

    /// Broadcast price update via UDP to Brain and Executor
    fn broadcast_price(&self, price_usd: f32) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Build SolPriceUpdate message (32 bytes)
        // Format: [msg_type(1), price(4), timestamp(8), source(1), padding(18)]
        let mut msg = [0u8; 32];
        msg[0] = SOL_PRICE_UPDATE_MSG_TYPE;
        msg[1..5].copy_from_slice(&price_usd.to_le_bytes());
        msg[5..13].copy_from_slice(&timestamp.to_le_bytes());
        msg[13] = PYTH_SOURCE;
        // Remaining 18 bytes are padding (already zeroed)

        // Broadcast to Brain
        self.udp_socket
            .send_to(&msg, &self.brain_addr)
            .context("Failed to send price to Brain")?;

        // Broadcast to Executor
        self.udp_socket
            .send_to(&msg, &self.executor_addr)
            .context("Failed to send price to Executor")?;

        Ok(())
    }
}

/// Spawn Pyth subscriber as background task
pub fn spawn_pyth_subscriber(grpc_endpoint: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🚀 Spawning Pyth subscriber task");
        
        loop {
            match PythSubscriber::new(grpc_endpoint.clone()) {
                Ok(subscriber) => {
                    info!("✅ Pyth subscriber initialized");
                    
                    if let Err(e) = subscriber.run().await {
                        error!("❌ Pyth subscriber error: {}", e);
                        error!("   Reconnecting in 5 seconds...");
                    }
                }
                Err(e) => {
                    error!("❌ Failed to create Pyth subscriber: {}", e);
                }
            }
            
            // Wait before retry
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("🔄 Retrying Pyth subscription...");
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pyth_price() {
        let subscriber = PythSubscriber::new("http://localhost:10000".to_string()).unwrap();
        
        // Mock Pyth account data with price=24523456, exponent=-6
        let mut data = vec![0u8; 240];
        
        // Price at offset 208
        let price_i64: i64 = 24_523_456;
        data[208..216].copy_from_slice(&price_i64.to_le_bytes());
        
        // Exponent at offset 232
        let exponent: i32 = -6;
        data[232..236].copy_from_slice(&exponent.to_le_bytes());
        
        let price = subscriber.parse_pyth_price(&data);
        assert!(price.is_some());
        
        let price_val = price.unwrap();
        assert!(price_val > 24.0 && price_val < 25.0);
        assert!((price_val - 24.523456).abs() < 0.001);
    }

    #[test]
    fn test_price_message_format() {
        let price_usd = 125.75_f32;
        let timestamp = 1234567890_u64;
        
        let mut msg = [0u8; 32];
        msg[0] = SOL_PRICE_UPDATE_MSG_TYPE;
        msg[1..5].copy_from_slice(&price_usd.to_le_bytes());
        msg[5..13].copy_from_slice(&timestamp.to_le_bytes());
        msg[13] = PYTH_SOURCE;
        
        // Verify parsing
        assert_eq!(msg[0], 14);
        assert_eq!(f32::from_le_bytes([msg[1], msg[2], msg[3], msg[4]]), price_usd);
        assert_eq!(u64::from_le_bytes([
            msg[5], msg[6], msg[7], msg[8],
            msg[9], msg[10], msg[11], msg[12]
        ]), timestamp);
        assert_eq!(msg[13], 1); // Pyth source
    }
}
