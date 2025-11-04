//! 🔮 Pyth Oracle SOL/USD Price Subscriber (HTTP RPC)
//! 
//! Polls Pyth price oracle account via Solana HTTP RPC and broadcasts
//! real-time SOL/USD price updates via UDP to Brain (45100) and Executor (45110).
//! 
//! This uses simple HTTP polling instead of gRPC subscriptions.

use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::net::UdpSocket;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use tracing::{error, info, warn};

/// Pyth SOL/USD Price Feed Account (Mainnet)
const PYTH_SOL_USD_FEED: &str = "H6ARHf6YoNAfHp2rGQTqSXRfxiAqoFvkVZoxMdVpZGgr";

/// Poll interval (2 seconds for sub-second latency)
const POLL_INTERVAL_SECS: u64 = 2;

/// Broadcast interval (20 seconds as per requirements)
const BROADCAST_INTERVAL_SECS: u64 = 20;

/// UDP ports for broadcasting price updates
const BRAIN_UDP_PORT: u16 = 45100;
const EXECUTOR_UDP_PORT: u16 = 45110;

/// Message type for SolPriceUpdate (matching brain/src/udp_bus/messages.rs)
const SOL_PRICE_UPDATE_MSG_TYPE: u8 = 14;

/// Pyth price source identifier
const PYTH_SOURCE: u8 = 1;

pub struct PythSubscriberRpc {
    rpc_client: RpcClient,
    udp_socket: UdpSocket,
    pyth_feed_pubkey: Pubkey,
    brain_addr: String,
    executor_addr: String,
}

impl PythSubscriberRpc {
    /// Create new Pyth subscriber with RPC polling
    pub fn new(rpc_endpoint: String) -> Result<Self> {
        // Non-blocking UDP socket for broadcasting
        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket for Pyth price broadcast")?;
        udp_socket
            .set_nonblocking(true)
            .context("Failed to set UDP socket to non-blocking")?;

        let pyth_feed_pubkey =
            Pubkey::from_str(PYTH_SOL_USD_FEED).context("Invalid Pyth feed pubkey")?;

        let rpc_client = RpcClient::new(rpc_endpoint);

        Ok(Self {
            rpc_client,
            udp_socket,
            pyth_feed_pubkey,
            brain_addr: format!("127.0.0.1:{}", BRAIN_UDP_PORT),
            executor_addr: format!("127.0.0.1:{}", EXECUTOR_UDP_PORT),
        })
    }

    /// Start Pyth price polling with periodic broadcasts
    pub async fn run(&self) -> Result<()> {
        info!("🔮 Starting Pyth SOL/USD Price Subscriber (RPC Polling)");
        info!("   📡 Feed: {}", PYTH_SOL_USD_FEED);
        info!(
            "   🎯 Broadcast to: Brain ({}), Executor ({})",
            self.brain_addr, self.executor_addr
        );

        // Track latest price for change detection
        let mut latest_price: Option<f32> = None;
        let mut last_broadcast_price: Option<f32> = None;

        let mut poll_interval = interval(Duration::from_secs(POLL_INTERVAL_SECS));
        let mut broadcast_interval = interval(Duration::from_secs(BROADCAST_INTERVAL_SECS));

        loop {
            tokio::select! {
                // Poll RPC for price updates
                _ = poll_interval.tick() => {
                    match self.fetch_pyth_price().await {
                        Ok(price) => {
                            // Detect price change
                            let price_changed = latest_price.map_or(true, |old| (price - old).abs() > 0.01);
                            latest_price = Some(price);

                            if price_changed {
                                // Broadcast immediately on price change
                                if let Err(e) = self.broadcast_price(price) {
                                    warn!("Failed to broadcast price update: {}", e);
                                } else {
                                    info!("📊 SOL/USD: ${:.4} (Pyth)", price);
                                    last_broadcast_price = Some(price);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Failed to fetch Pyth price: {}", e);
                        }
                    }
                }

                // Periodic broadcast (every 20 seconds)
                _ = broadcast_interval.tick() => {
                    if let Some(price) = latest_price {
                        // Only broadcast if price exists and hasn't been broadcast recently
                        let should_broadcast = last_broadcast_price.map_or(true, |last| (price - last).abs() > 0.001);
                        
                        if should_broadcast {
                            if let Err(e) = self.broadcast_price(price) {
                                warn!("Failed to broadcast periodic price: {}", e);
                            } else {
                                info!("⏰ Periodic SOL/USD: ${:.4}", price);
                                last_broadcast_price = Some(price);
                            }
                        }
                    } else {
                        warn!("⏸️  No Pyth price available yet for periodic broadcast");
                    }
                }
            }
        }
    }

    /// Fetch Pyth price from RPC
    async fn fetch_pyth_price(&self) -> Result<f32> {
        let account = self
            .rpc_client
            .get_account(&self.pyth_feed_pubkey)
            .context("Failed to fetch Pyth account")?;

        self.parse_pyth_price(&account.data)
            .context("Failed to parse Pyth price from account data")
    }

    /// Parse Pyth price from account data
    /// Pyth price account structure (simplified):
    /// - Bytes 0-4: Magic number
    /// - Bytes 4-8: Version
    /// - Bytes 8-12: Type
    /// - Bytes 208-216: Price (i64)
    /// - Bytes 216-220: Confidence (u64)
    /// - Bytes 220-224: Exponent (i32)
    fn parse_pyth_price(&self, data: &[u8]) -> Option<f32> {
        if data.len() < 224 {
            return None;
        }

        // Read price (i64 at offset 208)
        let price_raw = i64::from_le_bytes(data[208..216].try_into().ok()?);

        // Read exponent (i32 at offset 220)
        let exponent = i32::from_le_bytes(data[220..224].try_into().ok()?);

        // Calculate actual price: price_raw * 10^exponent
        let price = price_raw as f64 * 10f64.powi(exponent);

        // Sanity check: SOL price should be between $1 and $10,000
        if price < 1.0 || price > 10_000.0 {
            warn!(
                "⚠️  Pyth price out of range: ${:.4} (raw={}, exp={})",
                price, price_raw, exponent
            );
            return None;
        }

        Some(price as f32)
    }

    /// Broadcast price update to Brain and Executor
    fn broadcast_price(&self, price: f32) -> Result<()> {
        let timestamp_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        // Message format: [msg_type(1), source(1), timestamp(8), price(4)]
        let mut msg = Vec::with_capacity(14);
        msg.push(SOL_PRICE_UPDATE_MSG_TYPE);
        msg.push(PYTH_SOURCE);
        msg.extend_from_slice(&timestamp_ns.to_le_bytes());
        msg.extend_from_slice(&price.to_le_bytes());

        // Send to Brain
        self.udp_socket
            .send_to(&msg, &self.brain_addr)
            .context("Failed to send price to Brain")?;

        // Send to Executor
        self.udp_socket
            .send_to(&msg, &self.executor_addr)
            .context("Failed to send price to Executor")?;

        Ok(())
    }
}

/// Spawn Pyth subscriber task in background with auto-retry
pub fn spawn_pyth_subscriber_rpc(rpc_endpoint: String) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🚀 Spawning Pyth RPC subscriber task");
        
        loop {
            match PythSubscriberRpc::new(rpc_endpoint.clone()) {
                Ok(subscriber) => {
                    info!("✅ Pyth RPC subscriber initialized");
                    
                    if let Err(e) = subscriber.run().await {
                        error!("❌ Pyth RPC subscriber error: {}", e);
                        error!("   Reconnecting in 5 seconds...");
                    }
                }
                Err(e) => {
                    error!("❌ Failed to create Pyth RPC subscriber: {}", e);
                }
            }
            
            // Wait before retry
            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("🔄 Retrying Pyth RPC subscription...");
        }
    })
}
