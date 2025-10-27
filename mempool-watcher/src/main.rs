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
use decoder::TransactionDecoder;
use heat_calculator::HeatCalculator;
use transaction_monitor::TransactionMonitor;
use udp_publisher::UdpPublisher;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("🚀 Mempool Watcher Starting...");

    // Load configuration
    let config = Config::from_env()?;
    config.print_startup_info();

    // Wrap config in Arc for sharing across tasks
    let config = Arc::new(config);

    // Initialize components
    let (monitor, mut tx_receiver) = TransactionMonitor::new(config.rpc.ws_url.clone());
    
    let decoder = Arc::new(TransactionDecoder::new(config.thresholds.whale_threshold_sol));
    
    let heat_calculator = Arc::new(HeatCalculator::new(
        config.monitoring.transaction_window_secs,
        config.thresholds.whale_threshold_sol,
        config.thresholds.bot_repeat_threshold,
    ));
    
    let udp_publisher = Arc::new(UdpPublisher::new(
        &config.udp.bind_address,
        config.udp.brain_port,
        config.udp.executor_port,
    )?);

    info!("✅ All components initialized");

    // Spawn WebSocket monitoring task
    let monitor_handle = {
        let monitor = Arc::new(monitor);
        tokio::spawn(async move {
            if let Err(e) = monitor.start_monitoring().await {
                error!("❌ Monitor task failed: {}", e);
            }
        })
    };

    // Spawn transaction processing task
    let processing_handle = {
        let decoder = decoder.clone();
        let heat_calculator = heat_calculator.clone();
        let udp_publisher = udp_publisher.clone();
        
        tokio::spawn(async move {
            info!("🔄 Transaction processor started");
            
            while let Some(raw_tx) = tx_receiver.recv().await {
                debug!("📦 Processing transaction: {}", &raw_tx.signature[..12]);
                
                // In production, fetch full transaction details here
                // For now, we work with what we have from logs
                
                // Calculate heat and publish if hot
                let heat = heat_calculator.calculate_heat();
                
                if heat.score >= 70 {
                    info!("� HOT SIGNAL detected! Score: {}", heat.score);
                    
                    // Create hot signal
                    let hot_signal = heat_calculator::HotSignal {
                        mint: "unknown".to_string(), // Would be extracted from full tx
                        whale_wallet: "unknown".to_string(),
                        amount_sol: 0.0,
                        action: "BUY".to_string(),
                        urgency: heat.score,
                        timestamp: chrono::Utc::now().timestamp() as u64,
                    };
                    
                    // Publish to executor
                    if let Err(e) = udp_publisher.send_hot_signal_to_executor(&hot_signal) {
                        error!("Failed to publish hot signal: {}", e);
                    }
                }
            }
        })
    };

    // Spawn heat calculation periodic task
    let heat_handle = {
        let heat_calculator = heat_calculator.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(5));
            
            loop {
                tick.tick().await;
                
                let heat = heat_calculator.calculate_heat();
                debug!("🌡️  Heat: {} | TxRate: {:.2}/s | Whale: {:.2} SOL | Bot: {:.1}%",
                    heat.score, heat.tx_rate, heat.whale_activity, heat.bot_density * 100.0);
            }
        })
    };

    info!("✅ All systems initialized");
    info!("🚀 Mempool monitoring active");
    info!("📡 Publishing hot signals to {}:{}", config.udp.bind_address, config.udp.executor_port);

    // Wait for all tasks (runs indefinitely)
    tokio::select! {
        _ = monitor_handle => {
            error!("Monitor task ended unexpectedly");
        }
        _ = processing_handle => {
            error!("Processing task ended unexpectedly");
        }
        _ = heat_handle => {
            error!("Heat calculation task ended unexpectedly");
        }
    }

    Ok(())
}
