// ============================================================================
// EXECUTOR - Lightweight Execution Only (SIMPLIFIED VERSION)
// Decision-making moved to Brain service
// ============================================================================

mod config;
mod telegram;
mod database;
mod trading;
mod advice_bus;
mod mempool_bus;  // NEW: Mempool hot signals (port 45130)
mod emoji;
mod metrics;
mod telemetry;
mod slippage;  // NEW: Slippage calculation module
mod performance_log;  // NEW: JSONL performance logging

// Re-export unused modules to prevent warnings
mod grpc_client;
mod pump_bonding_curve;
mod jito;
mod pump_instructions;
mod tpu_client;
mod data;

use std::sync::Arc;
use std::collections::HashMap;
use log::{info, error, warn, debug};
use tokio::sync::RwLock;

// Simplified position tracker
struct ActivePosition {
    token_address: String,
    entry_time: std::time::Instant,
    decision_id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logger
    env_logger::init();
    
    // Initialize metrics
    metrics::init_metrics();
    
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🤖 EXECUTOR - Lightweight Execution Only");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("");
    info!("   Decision-making: Brain Service (UDP:45100 → Executor)");
    info!("   Execution: This service");
    info!("   Telemetry: Executor → Brain (UDP:45110)");
    info!("");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    // Load .env file
    dotenv::dotenv().ok();
    
    // Load configuration
    let config = Arc::new(config::Config::from_env()?);
    info!("✅ Configuration: Loaded from .env");
    info!("   Advice Bus: Will listen on port 45100");
    info!("   Brain Telemetry: {}:{} (enabled: {})", 
          config.brain_telemetry_host, config.brain_telemetry_port, config.brain_telemetry_enabled);
    
    // Initialize telemetry sender
    let telemetry = if config.brain_telemetry_enabled {
        match telemetry::TelemetrySender::new(
            &config.brain_telemetry_host,
            config.brain_telemetry_port,
            true
        ) {
            Ok(sender) => {
                info!("✅ Telemetry: Active (sending to Brain:{})", config.brain_telemetry_port);
                Some(Arc::new(sender))
            }
            Err(e) => {
                warn!("⚠️  Telemetry: Failed to initialize ({}) - continuing without telemetry", e);
                None
            }
        }
    } else {
        info!("ℹ️  Telemetry: Disabled in config");
        None
    };
    
    // Initialize database
    let db = Arc::new(database::Database::new(&config).await?);
    info!("✅ Database: Connected ({}:{}/{})", config.db_host, config.db_port, config.db_name);
    
    // Initialize Telegram
    let telegram = Arc::new(telegram::TelegramClient::new(&config)?);
    info!("✅ Telegram: Initialized");
    telegram.send_message("🤖 Executor Started - Listening for Brain decisions").await?;
    
    // Initialize trading engine
    let trading = Arc::new(trading::TradingEngine::new(&config).await?);
    info!("✅ Trading Engine: Initialized");
    
    // Active positions tracker
    let active_positions: Arc<RwLock<HashMap<String, ActivePosition>>> = Arc::new(RwLock::new(HashMap::new()));
    info!("✅ Position Tracker: Initialized");
    
    // Start Advice Bus listener (receives TradeDecisions from Brain)
    let positions_clone = active_positions.clone();
    let trading_clone = trading.clone();
    let telegram_clone = telegram.clone();
    let db_clone = db.clone();
    let config_clone = config.clone();
    let telemetry_clone = telemetry.clone();
    
    tokio::spawn(async move {
        match advice_bus::AdviceBusListener::new(45100, 0) {
            Ok(listener) => {
                info!("✅ Advice Bus Listener: Active on port 45100 (waiting for Brain decisions)");
                
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    
                    if let Some(advisory) = listener.try_recv() {
                        let timestamp_received = telemetry::now_ns();
                        
                        match advisory {
                            advice_bus::Advisory::LateOpportunity { mint, score, .. } |
                            advice_bus::Advisory::CopyTrade { mint, confidence: score, .. } => {
                                let mint_str = bs58::encode(mint).into_string();
                                let decision_id = uuid::Uuid::new_v4().to_string();
                                
                                info!("📥 RECEIVED TradeDecision: {} | score: {} | decision_id: {}",
                                      &mint_str[..12], score, &decision_id[..8]);
                                
                                // Apply advice constraints
                                if score < config_clone.advice_min_confidence {
                                    warn!("⚠️  Skipping {}: score {} < min_confidence {}", 
                                          &mint_str[..12], score, config_clone.advice_min_confidence);
                                    continue;
                                }
                                
                                // Check if we already have a position
                                if positions_clone.read().await.contains_key(&mint_str) {
                                    warn!("⚠️  Skipping {}: already have position", &mint_str[..12]);
                                    continue;
                                }
                                
                                // TODO: Execute BUY using trading engine
                                info!("🎯 WOULD EXECUTE BUY: {} (score: {})", &mint_str[..12], score);
                                info!("   [Execution logic to be implemented]");
                                
                                // Track position (simplified)
                                let position = ActivePosition {
                                    token_address: mint_str.clone(),
                                    entry_time: std::time::Instant::now(),
                                    decision_id: decision_id.clone(),
                                };
                                positions_clone.write().await.insert(mint_str.clone(), position);
                                
                                // Send telemetry (if enabled)
                                if let Some(ref telem) = telemetry_clone {
                                    let latency_ms = (telemetry::now_ns() - timestamp_received) as f64 / 1_000_000.0;
                                    let telemetry_msg = telemetry::ExecutionTelemetry {
                                        decision_id: decision_id.clone(),
                                        mint: mint_str.clone(),
                                        action: telemetry::TelemetryAction::Buy,
                                        timestamp_ns_received: timestamp_received,
                                        timestamp_ns_confirmed: telemetry::now_ns(),
                                        latency_exec_ms: latency_ms,
                                        status: telemetry::ExecutionStatus::Success,
                                        realized_pnl_usd: None,
                                        error_msg: None,
                                    };
                                    telem.send(telemetry_msg);
                                }
                                
                                // Notify Telegram
                                telegram_clone.send_message(&format!(
                                    "📥 Decision received: {}\nScore: {}\nStatus: Processing...",
                                    &mint_str[..12], score
                                )).await.ok();
                                
                                info!("📊 Active positions: {}", positions_clone.read().await.len());
                            }
                            
                            advice_bus::Advisory::ExtendHold { mint, .. } => {
                                let mint_str = bs58::encode(mint).into_string();
                                info!("📥 RECEIVED ExtendHold: {}", &mint_str[..12]);
                                // TODO: Implement hold extension logic
                            }
                            
                            advice_bus::Advisory::WidenExit { mint, .. } => {
                                let mint_str = bs58::encode(mint).into_string();
                                info!("📥 RECEIVED WidenExit: {}", &mint_str[..12]);
                                // TODO: Implement widen exit logic
                            }
                            
                            advice_bus::Advisory::SolPriceUpdate { price_cents, timestamp_secs, source, .. } => {
                                // Convert cents to dollars
                                let price_usd = price_cents as f64 / 100.0;
                                let source_name = match source {
                                    1 => "Pyth",
                                    2 => "Jupiter",
                                    3 => "Fallback",
                                    _ => "Unknown",
                                };
                                
                                debug!("📊 RECEIVED SolPriceUpdate: ${:.2} from {} (ts: {})", 
                                    price_usd, source_name, timestamp_secs);
                                
                                // Update the cache used by trading engine
                                trading::update_sol_price_cache(price_usd).await;
                            }
                            
                            advice_bus::Advisory::EmergencyExit { mint, .. } => {
                                let mint_str = bs58::encode(mint).into_string();
                                warn!("📥 RECEIVED EmergencyExit: {}", &mint_str[..12]);
                                // TODO: Implement emergency exit logic
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("❌ Failed to start Advice Bus listener: {}", e);
                error!("   Cannot receive decisions from Brain!");
            }
        }
    });
    
    info!("");
    info!("🚀 EXECUTOR READY");
    info!("   Listening for TradeDecisions from Brain on port 45100");
    info!("   Listening for Hot Signals from Mempool on port 45130");
    info!("   Sending telemetry back to Brain on port 45110");
    info!("");
    
    // Start Mempool Bus listener (receives Hot Signals for frontrunning)
    let positions_mempool = active_positions.clone();
    let trading_mempool = trading.clone();
    let telegram_mempool = telegram.clone();
    
    tokio::spawn(async move {
        match mempool_bus::MempoolBusListener::new(45130) {
            Ok(listener) => {
                info!("✅ Mempool Bus Listener: Active on port 45130 (waiting for hot signals)");
                
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;  // 10ms poll (fast)
                    
                    if let Some(signal) = listener.try_recv() {
                        info!("🔥 HOT SIGNAL: {} | urgency: {} | whale: {}...{} | SOL: {:.4} | action: {}", 
                              &signal.mint[..12],
                              signal.urgency,
                              &signal.whale_wallet[..4],
                              &signal.whale_wallet[signal.whale_wallet.len()-4..],
                              signal.amount_sol,
                              signal.action);
                        
                        // Priority check: High urgency signals (>= 80) get immediate attention
                        if signal.urgency >= 80 {
                            warn!("⚡ HIGH URGENCY SIGNAL ({}) - Priority execution recommended", signal.urgency);
                            
                            // Check if we already have a position
                            if positions_mempool.read().await.contains_key(&signal.mint) {
                                debug!("   Already have position in {}, skipping", &signal.mint[..12]);
                                continue;
                            }
                            
                            // TODO: Implement hot signal execution
                            // This should:
                            // 1. Validate signal freshness (timestamp < 100ms old)
                            // 2. Execute BUY if action == "buy" and urgency >= threshold
                            // 3. Use higher priority fees for urgency >= 90
                            // 4. Skip normal Brain decision queue (direct execution)
                            
                            info!("   🎯 WOULD EXECUTE HOT SIGNAL: {} ({} SOL)", 
                                  &signal.mint[..12], signal.amount_sol);
                            info!("   [Hot signal execution to be implemented]");
                            
                            // Notify Telegram for high urgency
                            telegram_mempool.send_message(&format!(
                                "🔥 HOT SIGNAL DETECTED!\nToken: {}\nUrgency: {}/100\nWhale: {:.4} SOL\nAction: {}",
                                &signal.mint[..12],
                                signal.urgency,
                                signal.amount_sol,
                                signal.action
                            )).await.ok();
                        } else if signal.urgency >= 60 {
                            info!("   💡 Medium urgency ({}), monitoring...", signal.urgency);
                            // Medium urgency - track but don't execute immediately
                        } else {
                            debug!("   Low urgency ({}), ignoring", signal.urgency);
                        }
                    }
                }
            }
            Err(e) => {
                error!("❌ Failed to start Mempool Bus listener: {}", e);
                error!("   Will not receive hot signals from mempool watcher!");
                error!("   This reduces reaction speed to frontrunning opportunities.");
            }
        }
    });
    
    // Main loop - just monitor positions
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        
        let positions_count = active_positions.read().await.len();
        if positions_count > 0 {
            debug!("📊 Active positions: {}", positions_count);
        }
        
        // TODO: Listen for SELL decisions from Brain
    }
}
