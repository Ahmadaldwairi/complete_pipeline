//! 🔮 Pyth Oracle SOL/USD Price via HTTP API
//! 
//! Fetches Pyth price via Hermes HTTP API and broadcasts
//! real-time SOL/USD price updates via UDP to Brain (45100) and Executor (45110).
//!
//! Features:
//! - Exponential backoff retry for network resilience
//! - Confidence interval filtering for price quality
//! - SQLite logging for analytics and debugging
//! - WebSocket-ready architecture for future upgrades

use anyhow::{Context, Result};
use serde::Deserialize;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::{error, info, warn, debug};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Pyth Hermes API endpoint
const PYTH_HERMES_API: &str = "https://hermes.pyth.network/v2/updates/price/latest";

/// Pyth SOL/USD Price Feed ID
const SOL_USD_FEED_ID: &str = "0xef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";

/// Poll interval (5 seconds)
const POLL_INTERVAL_SECS: u64 = 5;

/// Maximum confidence interval (3% of price = high quality)
const MAX_CONFIDENCE_RATIO: f64 = 0.03;

/// Exponential backoff parameters
const INITIAL_RETRY_DELAY_MS: u64 = 100;
const MAX_RETRY_DELAY_MS: u64 = 5000;
const MAX_RETRIES: u32 = 5;

/// UDP ports for broadcasting price updates
const BRAIN_UDP_PORT: u16 = 45100;
const EXECUTOR_UDP_PORT: u16 = 45110;

/// Message type for SolPriceUpdate
const SOL_PRICE_UPDATE_MSG_TYPE: u8 = 14;

/// Pyth price source identifier
const PYTH_SOURCE: u8 = 1;

#[derive(Debug, Deserialize)]
struct PythResponse {
    parsed: Vec<PythParsed>,
}

#[derive(Debug, Deserialize)]
struct PythParsed {
    price: PythPrice,
}

#[derive(Debug, Deserialize)]
struct PythPrice {
    price: String,
    conf: String,
    expo: i32,
    publish_time: i64,
}

/// Price data with confidence
#[derive(Debug, Clone)]
pub struct PriceData {
    pub price: f32,
    pub confidence: f32,
    pub confidence_ratio: f64,
    pub timestamp: i64,
}

pub struct PythHttp {
    client: reqwest::Client,
    udp_socket: UdpSocket,
    brain_addr: String,
    executor_addr: String,
    db: Option<Arc<Mutex<crate::Database>>>,
    price_buffer: Arc<Mutex<Vec<f32>>>, // Rolling buffer of last 3 prices for median filtering
}

impl PythHttp {
    pub fn new(db: Option<Arc<Mutex<crate::Database>>>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client")?;

        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind UDP socket")?;
        udp_socket
            .set_nonblocking(true)
            .context("Failed to set UDP socket to non-blocking")?;

        Ok(Self {
            client,
            udp_socket,
            brain_addr: format!("127.0.0.1:{}", BRAIN_UDP_PORT),
            executor_addr: format!("127.0.0.1:{}", EXECUTOR_UDP_PORT),
            db,
            price_buffer: Arc::new(Mutex::new(Vec::with_capacity(3))),
        })
    }

    pub async fn run(&self) -> Result<()> {
        info!("🔮 Starting Pyth SOL/USD Price Fetcher (HTTP API)");
        info!("   📡 Feed ID: {}", SOL_USD_FEED_ID);
        info!("   🎯 Broadcast to: Brain ({}), Executor ({})", 
            self.brain_addr, self.executor_addr
        );
        info!("   ✅ Confidence filtering enabled (max {:.1}%)", MAX_CONFIDENCE_RATIO * 100.0);
        info!("   🎲 Jitter enabled (±2s randomization to avoid collisions)");
        if self.db.is_some() {
            info!("   💾 SQLite price logging enabled");
        }

        let mut latest_price: Option<f32> = None;
        let mut rng = StdRng::from_entropy();

        loop {
            // Add jitter: 5 seconds ±2s = 3-7 seconds
            let jitter_ms = rng.gen_range(-2000..=2000);
            let interval_ms = (POLL_INTERVAL_SECS * 1000) as i64 + jitter_ms;
            let interval_ms = interval_ms.max(1000) as u64; // Minimum 1 second
            
            sleep(Duration::from_millis(interval_ms)).await;

            match self.fetch_price_with_retry().await {
                Ok(price_data) => {
                    // Check confidence ratio
                    if price_data.confidence_ratio > MAX_CONFIDENCE_RATIO {
                        warn!(
                            "⚠️  Skipping low-confidence price: ${:.4} (conf: {:.1}%)",
                            price_data.price,
                            price_data.confidence_ratio * 100.0
                        );
                        continue;
                    }

                    // Add to rolling buffer and compute median
                    let mut buffer = self.price_buffer.lock().unwrap();
                    buffer.push(price_data.price);
                    if buffer.len() > 3 {
                        buffer.remove(0); // Keep only last 3
                    }
                    
                    // Use median price if we have at least 3 samples
                    let filtered_price = if buffer.len() >= 3 {
                        let mut sorted = buffer.clone();
                        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        sorted[1] // Median of 3 values
                    } else {
                        price_data.price // Not enough samples yet, use raw price
                    };
                    drop(buffer);

                    // Detect significant price change (>$0.10) using filtered price
                    let should_broadcast = latest_price.map_or(true, |old| {
                        (filtered_price - old).abs() > 0.10
                    });

                    if should_broadcast {
                        // Broadcast filtered price to Brain & Executor
                        if let Err(e) = self.broadcast_price(filtered_price) {
                            warn!("Failed to broadcast price: {}", e);
                        } else {
                            info!(
                                "📊 SOL/USD: ${:.4} (median-filtered) ±${:.4} ({:.2}% conf)",
                                filtered_price,
                                price_data.confidence,
                                price_data.confidence_ratio * 100.0
                            );
                            latest_price = Some(filtered_price);
                        }

                        // Log to SQLite
                        if let Some(ref db) = self.db {
                            if let Err(e) = self.log_price_to_db(db, &price_data) {
                                debug!("Failed to log price to SQLite: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch Pyth price after retries: {}", e);
                }
            }
        }
    }

    /// Fetch price with exponential backoff retry
    async fn fetch_price_with_retry(&self) -> Result<PriceData> {
        let mut retry_count = 0;
        let mut delay_ms = INITIAL_RETRY_DELAY_MS;

        loop {
            match self.fetch_price().await {
                Ok(price_data) => return Ok(price_data),
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= MAX_RETRIES {
                        return Err(e.context(format!(
                            "Failed after {} retries",
                            MAX_RETRIES
                        )));
                    }

                    warn!(
                        "Retry {}/{}: {} (waiting {}ms)",
                        retry_count, MAX_RETRIES, e, delay_ms
                    );

                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                    // Exponential backoff with cap
                    delay_ms = (delay_ms * 2).min(MAX_RETRY_DELAY_MS);
                }
            }
        }
    }

    async fn fetch_price(&self) -> Result<PriceData> {
        let url = format!("{}?ids[]={}", PYTH_HERMES_API, SOL_USD_FEED_ID);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to Pyth API")?;

        let pyth_response: PythResponse = response
            .json()
            .await
            .context("Failed to parse Pyth response")?;

        let parsed_data = pyth_response
            .parsed
            .first()
            .context("No price data in response")?;

        let price_info = &parsed_data.price;

        // Parse price: price_str * 10^expo
        let price_raw: i64 = price_info
            .price
            .parse()
            .context("Failed to parse price string")?;

        let price = price_raw as f64 * 10f64.powi(price_info.expo);

        // Parse confidence interval
        let conf_raw: i64 = price_info
            .conf
            .parse()
            .context("Failed to parse confidence string")?;

        let confidence = conf_raw as f64 * 10f64.powi(price_info.expo);

        // Calculate confidence ratio (conf / price)
        let confidence_ratio = if price > 0.0 {
            confidence / price
        } else {
            1.0 // Invalid, will be filtered
        };

        // Sanity check
        if price < 1.0 || price > 10_000.0 {
            anyhow::bail!("Price out of range: ${:.2}", price);
        }

        Ok(PriceData {
            price: price as f32,
            confidence: confidence as f32,
            confidence_ratio,
            timestamp: price_info.publish_time,
        })
    }

    /// Log price to SQLite for analytics
    fn log_price_to_db(
        &self,
        db: &Arc<Mutex<crate::Database>>,
        price_data: &PriceData,
    ) -> Result<()> {
        let mut db = db.lock().unwrap();
        db.log_pyth_price(
            price_data.timestamp,
            price_data.price,
            price_data.confidence,
            price_data.confidence_ratio,
            "pyth_http",
        )?;
        Ok(())
    }

    fn broadcast_price(&self, price: f32) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Message format matching Brain's SolPriceUpdate struct:
        // [msg_type(1), price_usd(4), timestamp(8), source(1), padding(18)] = 32 bytes
        let mut msg = vec![0u8; 32];
        msg[0] = SOL_PRICE_UPDATE_MSG_TYPE;
        msg[1..5].copy_from_slice(&price.to_le_bytes());
        msg[5..13].copy_from_slice(&timestamp.to_le_bytes());
        msg[13] = PYTH_SOURCE;
        // bytes 14-31 are already zero (padding)

        // Send to Brain only (Executor doesn't need price updates)
        self.udp_socket
            .send_to(&msg, &self.brain_addr)
            .context("Failed to send price to Brain")?;

        Ok(())
    }
}

/// Spawn Pyth HTTP fetcher in background with optional DB logging
pub fn spawn_pyth_http(db: Option<Arc<Mutex<crate::Database>>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("🚀 Spawning Pyth HTTP fetcher task");

        loop {
            match PythHttp::new(db.clone()) {
                Ok(fetcher) => {
                    info!("✅ Pyth HTTP fetcher initialized");

                    if let Err(e) = fetcher.run().await {
                        error!("❌ Pyth HTTP fetcher error: {}", e);
                        error!("   Reconnecting in 5 seconds...");
                    }
                }
                Err(e) => {
                    error!("❌ Failed to create Pyth HTTP fetcher: {}", e);
                }
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
            info!("🔄 Retrying Pyth HTTP fetch...");
        }
    })
}
