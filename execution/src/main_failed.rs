// ============================================================================
// EXECUTOR - Lightweight Execution Only
// Decision-making moved to Brain service
// ============================================================================

mod config;
mod grpc_client;
mod trading;
mod telegram;
mod database;
mod pump_bonding_curve;
mod jito;
mod pump_instructions;
mod tpu_client;
mod data;
mod advice_bus;
mod emoji;
mod metrics;
mod telemetry;

use std::sync::Arc;
use std::collections::HashMap;
use log::{info, error, warn, debug};
use tokio::sync::RwLock;
use uuid::Uuid;
use database::LatencyTrace;

// Track active positions (simplified - only execution tracking)
struct ActivePosition {
    token_address: String,
    buy_result: trading::BuyResult,
    entry_time: std::time::Instant,
    trace: LatencyTrace,
    decision_id: String,  // NEW: ID from Brain's TradeDecision
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
    info!("   Execution mode: TPU={}, Jito={}", config.use_tpu, config.use_jito);
    info!("   Advice Bus: Listening on port 45100");
    info!("   Brain Telemetry: {}:{} (enabled: {})", 
          config.brain_telemetry_host, config.brain_telemetry_port, config.brain_telemetry_enabled);
    
    // Initialize telemetry sender
    let telemetry = if config.brain_telemetry_enabled {
        match telemetry::TelemetrySender::new(&config.brain_telemetry_host, config.brain_telemetry_port) {
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
    
    // Initialize trading engine
    let trading = Arc::new(
        trading::TradingEngine::new(
            config.rpc_endpoint.clone(),
            config.websocket_endpoint.clone(),
            config.grpc_endpoint.clone(),
            config.wallet_private_key.clone(),
            config.use_tpu,
            config.use_jito,
            config.jito_block_engine_url.clone(),
            config.jito_tip_account.clone(),
            config.jito_tip_amount,
            config.jito_use_dynamic_tip,
            config.jito_entry_percentile,
            config.jito_exit_percentile,
        )
        .await?,
    );
    info!("✅ Trading Engine: Initialized");
    info!("   Wallet: {}", trading.get_wallet_pubkey());
    
    // Initialize Telegram
    let telegram = Arc::new(telegram::TelegramClient::new(&config)?);
    info!("✅ Telegram: Initialized");
    
    // Initialize database
    let db = Arc::new(database::Database::new(&config).await?);
    info!("✅ Database: Connected ({}:{}/{})", config.db_host, config.db_port, config.db_name);
    
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
                                let decision_id = Uuid::new_v4().to_string();
                                
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
                                
                                // Execute BUY
                                info!("🎯 EXECUTING BUY: {} (score: {})", &mint_str[..12], score);
                                
                                let buy_result = trading_clone.execute_pump_buy(
                                    &mint_str,
                                    0.01,  // TODO: Get position_size_sol from TradeDecision
                                    500,   // TODO: Get slippage from TradeDecision
                                ).await;
                                
                                let timestamp_confirmed = telemetry::now_ns();
                                
                                match buy_result {
                                    Ok(result) => {
                                        let latency_ms = (timestamp_confirmed - timestamp_received) / 1_000_000;
                                        
                                        info!("✅ BUY SUCCESS: {} | tx: {} | latency: {}ms", 
                                              &mint_str[..12], &result.signature[..8], latency_ms);
                                        
                                        // Send success telemetry to Brain
                                        if let Some(ref telem) = telemetry_clone {
                                            let telemetry_msg = telemetry::ExecutionTelemetry::buy_success(
                                                decision_id.clone(),
                                                mint_str.clone(),
                                                timestamp_received,
                                                timestamp_confirmed,
                                                result.tokens_received,
                                            );
                                            telem.send(&telemetry_msg);
                                        }
                                        
                                        // Telegram notification
                                        telegram_clone.send_message(&format!(
                                            "🟢 BUY: {}\nTokens: {:.2}\nLatency: {}ms",
                                            &mint_str[..12], result.tokens_received, latency_ms
                                        ));
                                        
                                        // Log to database
                                        let trace = LatencyTrace {
                                            uuid: decision_id.clone(),
                                            timestamp_ns_received: timestamp_received,
                                            timestamp_ns_grpc: result.grpc_latency_ns,
                                            timestamp_ns_build: result.build_latency_ns,
                                            timestamp_ns_send: result.send_latency_ns,
                                            timestamp_ns_confirmed: timestamp_confirmed,
                                        };
                                        
                                        if let Err(e) = db_clone.log_trade_entry(
                                            &decision_id,
                                            &mint_str,
                                            result.sol_spent,
                                            result.tokens_received,
                                            &result.signature,
                                            "BRAIN_DECISION",
                                            score,
                                            &serde_json::to_string(&trace).unwrap_or_default(),
                                        ).await {
                                            error!("❌ Database log failed: {}", e);
                                        }
                                        
                                        // Track position
                                        let position = ActivePosition {
                                            token_address: mint_str.clone(),
                                            buy_result: result,
                                            entry_time: std::time::Instant::now(),
                                            trace,
                                            decision_id: decision_id.clone(),
                                        };
                                        positions_clone.write().await.insert(mint_str.clone(), position);
                                        
                                        info!("📊 Active positions: {}", positions_clone.read().await.len());
                                    }
                                    Err(e) => {
                                        error!("❌ BUY FAILED: {} | error: {}", &mint_str[..12], e);
                                        
                                        // Send failure telemetry to Brain
                                        if let Some(ref telem) = telemetry_clone {
                                            let telemetry_msg = telemetry::ExecutionTelemetry::execution_failed(
                                                decision_id.clone(),
                                                mint_str.clone(),
                                                "BUY".to_string(),
                                                timestamp_received,
                                                e.to_string(),
                                            );
                                            telem.send(&telemetry_msg);
                                        }
                                        
                                        telegram_clone.send_message(&format!(
                                            "❌ BUY FAILED: {}\nError: {}",
                                            &mint_str[..12], e
                                        ));
                                    }
                                }
                            }
                            
                            advice_bus::Advisory::ExtendHold { mint, horizon_sec, confidence } => {
                                let mint_str = bs58::encode(mint).into_string();
                                info!("📥 RECEIVED ExtendHold: {} | runway: {}s | conf: {}", 
                                      &mint_str[..12], horizon_sec, confidence);
                                
                                // TODO: Implement hold extension logic
                                // This would extend the max hold time for an active position
                            }
                            
                            advice_bus::Advisory::WidenExit { mint, slippage_bps, confidence } => {
                                let mint_str = bs58::encode(mint).into_string();
                                info!("📥 RECEIVED WidenExit: {} | slippage: {}bps | conf: {}",
                                      &mint_str[..12], slippage_bps, confidence);
                                
                                // TODO: Implement widen exit logic
                                // This would allow higher slippage for an exit
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
    info!("   Sending telemetry back to Brain on port 45110");
    info!("");
    
    // Main loop - just monitor positions and check for exits
    // (Exit logic will also come from Brain in the future)
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        
        let positions_count = active_positions.read().await.len();
        if positions_count > 0 {
            debug!("📊 Active positions: {}", positions_count);
        }
        
        // TODO: Listen for SELL decisions from Brain
        // For now, positions are held indefinitely (Brain will send ExitDecision later)
    }
}
