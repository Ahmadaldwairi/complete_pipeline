use anyhow::Result;
use log::{debug, error, info};
use std::sync::Arc;
use tokio::time::{interval, Duration};

mod config;
mod decoder;
mod heat_calculator;
mod udp_publisher;
mod transaction_monitor;

use config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Load configuration
    let config = Config::from_env()?;
    config.print_startup_info();

    // Wrap config in Arc for sharing across tasks
    let config = Arc::new(config);

    // TODO: Initialize components
    // - Transaction monitor (WebSocket listener)
    // - Transaction decoder (parse Pump.fun/Raydium)
    // - Heat calculator (real-time scoring)
    // - UDP publisher (send to Brain/Executor)

    info!("✅ All systems initialized");
    info!("🚀 Starting mempool monitoring...");

    // Main service loop
    let mut tick = interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        
        // TODO: Implement main loop logic
        // - Monitor transactions
        // - Calculate heat index
        // - Publish updates
    }
}
