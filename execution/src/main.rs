// ============================================================================
// EXECUTOR - Stateless Transaction Builder/Sender (SIMPLIFIED)
// Brain owns all state, decision-making, position tracking, confirmations
// Executor: Receive decision → Build tx → Send tx → Return signature
// ============================================================================

mod config;
mod database;
mod trading;
mod advice_bus;
mod metrics;
mod telemetry;
mod slippage;
mod performance_log;
mod execution_confirmation;  // Simple message: signature + price back to Brain
mod deduplicator;  // Minimal: prevent duplicate submissions within 5s window

// Re-export unused modules to prevent warnings
mod grpc_client;
mod pump_bonding_curve;
mod jito;
mod pump_instructions;
mod tpu_client;
mod emoji;
mod data;

use std::sync::Arc;
use std::time::Instant;
use log::{info, error, warn, debug};
use tokio::sync::RwLock;
use execution_confirmation::ExecutionConfirmation;

// Minimal deduplication tracking (prevents duplicate submissions within 5s)
struct RecentTrade {
    mint: [u8; 32],
    side: u8,  // 0=BUY, 1=SELL
    timestamp: std::time::Instant,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logger
    env_logger::init();
    
    // Initialize metrics
    metrics::init_metrics();
    
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("🤖 EXECUTOR - Stateless Transaction Builder");
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    info!("");
    
    // Load .env file
    dotenv::dotenv().ok();
    
    // Load configuration
    let config = Arc::new(config::Config::from_env()?);
    info!("✅ Configuration: Loaded from .env");
    info!("   Decision-making: Brain Service (UDP:{} → Executor)", config.advice_bus_port);
    info!("   Execution: This service");
    info!("   Telemetry: Executor → Brain (UDP:{})", config.brain_telemetry_port);
    info!("   Advice Bus: Will listen on port {}", config.advice_bus_port);
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
    
    // Initialize trading engine
    let trading = Arc::new(trading::TradingEngine::new(&config).await?);
    info!("✅ Trading Engine: Initialized");
    
    // Recent trades tracker (for deduplication - prevents double submissions within 5s)
    let recent_trades: Arc<RwLock<Vec<RecentTrade>>> = Arc::new(RwLock::new(Vec::new()));
    info!("✅ Deduplication: Active (5s window, max 100 trades)");
    
    // Initialize UDP socket for sending ExecutionConfirmations to Brain
    let confirmation_socket = Arc::new(
        tokio::net::UdpSocket::bind("0.0.0.0:0").await
            .expect("Failed to bind confirmation UDP socket")
    );
    let brain_confirmation_addr = "127.0.0.1:45115"; // Brain listens here for confirmations
    info!("✅ Confirmation Socket: Initialized (targeting Brain at {})", brain_confirmation_addr);
    
    // Start Advice Bus listener (receives TradeDecisions from Brain)
    let recent_trades_clone = recent_trades.clone();
    let trading_clone = trading.clone();
    let db_clone = db.clone();
    let config_clone = config.clone();
    let telemetry_clone = telemetry.clone();
    let confirmation_socket_clone = confirmation_socket.clone();
    let brain_addr_clone = brain_confirmation_addr.to_string();
    
    info!("📋 Starting Advice Bus Listener on port {}", config.advice_bus_port);
    
    tokio::spawn(async move {
        match advice_bus::AdviceBusListener::new(config_clone.advice_bus_port, 0) {
            Ok(listener) => {
                info!("✅ Advice Bus Listener: Active on port {} (waiting for Brain decisions)", config_clone.advice_bus_port);
                
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    
                    // UNIFIED MESSAGE RECEIVING: Read once, route by size
                    if let Some(message) = listener.try_recv_any() {
                        match message {
                            advice_bus::MessageType::TradeDecision(decision) => {
                                let timestamp_received = telemetry::now_ns();
                                let mint_str = bs58::encode(decision.mint).into_string();
                                let decision_id = uuid::Uuid::new_v4().to_string();
                                
                                info!("📥 RECEIVED TradeDecision: {} {} | mint: {} | conf: {}",
                                      if decision.is_buy() { "BUY" } else { "SELL" },
                                      decision.size_lamports as f64 / 1e9,
                                      &mint_str[..12], 
                                      decision.confidence);
                        
                        if decision.is_buy() {
                            // Check deduplication (prevent double-buy within 5s)
                            let now = std::time::Instant::now();
                            let mut trades = recent_trades_clone.write().await;
                            
                            // Clean up old entries (>5s)
                            trades.retain(|t| now.duration_since(t.timestamp).as_secs() < 5);
                            
                            // Check if we've seen this BUY recently
                            if trades.iter().any(|t| t.mint == decision.mint && t.side == 0) {
                                warn!("⚠️ Ignoring duplicate BUY within 5s window: {}", &mint_str[..12]);
                                drop(trades);
                                continue;
                            }
                            
                            // Record this BUY
                            trades.push(RecentTrade {
                                mint: decision.mint,
                                side: 0,
                                timestamp: now,
                            });
                            
                            // Limit to 100 recent trades
                            if trades.len() > 100 {
                                trades.remove(0);
                            }
                            drop(trades);
                            
                            info!("✅ BUY accepted (deduplication check passed)");
                            
                            // Execute BUY
                            let position_size_sol = decision.size_lamports as f64 / 1_000_000_000.0;
                            let position_size_usd = position_size_sol * 200.0; // Rough estimate
                            
                            info!("🔨 Building BUY transaction: {} SOL with {}% slippage",
                                  position_size_sol, decision.slippage_bps as f64 / 100.0);
                            
                            // Get warmed blockhash from cache
                            let cached_blockhash = Some(trading::get_cached_blockhash().await);
                            
                            // Execute the buy trade
                            match trading_clone.buy(
                                decision_id.clone(),
                                &mint_str,
                                position_size_usd,
                                1, // estimated_position
                                0.0, // mempool_volume
                                0, // pending_buys
                                Some(decision_id.clone()), // trace_id
                                cached_blockhash,
                                decision.entry_type,
                            ).await {
                                Ok(result) => {
                                    info!("✅ BUY executed successfully!");
                                    info!("   📝 Signature: {}", result.signature);
                                    info!("   💰 Tokens bought: {:.2}", result.token_amount);
                                    info!("   💵 SOL spent: {:.4}", result.position_size / 200.0);
                                    info!("   📊 Price: {:.10} SOL/token", result.price);
                                    
                                    // Send ExecutionConfirmation to Brain (Brain handles rest)
                                    let tx_sig_bytes: [u8; 32] = bs58::decode(&result.signature)
                                        .into_vec()
                                        .unwrap_or_default()
                                        .get(..32)
                                        .and_then(|s| s.try_into().ok())
                                        .unwrap_or([0u8; 32]);
                                    
                                    let confirmation = ExecutionConfirmation::new_success(
                                        decision.mint,
                                        0, // BUY side
                                        decision.size_lamports,
                                        result.price,
                                        tx_sig_bytes,
                                    );
                                    
                                    if let Err(e) = confirmation_socket_clone.send_to(
                                        &confirmation.to_bytes(),
                                        &brain_addr_clone
                                    ).await {
                                        error!("❌ Failed to send BUY confirmation to Brain: {}", e);
                                    } else {
                                        info!("📡 Sent BUY confirmation to Brain: {}", &mint_str[..12]);
                                    }
                                }
                                Err(e) => {
                                    error!("❌ BUY failed for {}: {}", &mint_str[..12], e);
                                    
                                    // Send failure confirmation to Brain
                                    let confirmation = ExecutionConfirmation::new_failure(decision.mint, 0);
                                    if let Err(send_err) = confirmation_socket_clone.send_to(
                                        &confirmation.to_bytes(),
                                        &brain_addr_clone
                                    ).await {
                                        error!("❌ Failed to send BUY failure confirmation: {}", send_err);
                                    }
                                }
                            }
                        } else {
                            // SELL: Check deduplication (prevent double-sell within 5s)
                            let mint_key = decision.mint;
                            let now = Instant::now();
                            let is_duplicate = {
                                let mut trades = recent_trades_clone.write().await;
                                
                                // Check if SELL already seen within 5s
                                let duplicate = trades.iter().any(|t| 
                                    t.mint == mint_key && t.side == 1 && now.duration_since(t.timestamp).as_secs() < 5
                                );
                                
                                // Clean old entries (>5s)
                                trades.retain(|t| now.duration_since(t.timestamp).as_secs() < 5);
                                
                                // Add this trade
                                if !duplicate {
                                    trades.push(RecentTrade {
                                        mint: mint_key,
                                        side: 1, // SELL
                                        timestamp: now,
                                    });
                                    
                                    // Keep max 100 recent trades
                                    if trades.len() > 100 {
                                        trades.remove(0);
                                    }
                                }
                                
                                duplicate
                            };
                            
                            if is_duplicate {
                                info!("⏭️  Skipping duplicate SELL for {} (seen <5s ago)", &mint_str[..12]);
                                continue;
                            }
                            
                            info!("🔨 Building SELL transaction (slippage: {}%)", 
                                  decision.slippage_bps as f64 / 100.0);
                            
                            // Execute SELL - Brain provides all needed data in TradeDecision
                            // No position lookup needed - Brain is source of truth
                            let cached_blockhash = Some(trading::get_cached_blockhash().await);
                            
                            match trading_clone.sell_simple(
                                &decision_id,
                                &mint_str,
                                decision.size_lamports,
                                decision.slippage_bps,
                                cached_blockhash,
                            ).await {
                                Ok(result) => {
                                    info!("✅ SELL executed successfully!");
                                    info!("   📝 Signature: {}", result.signature);
                                    info!("   💰 Exit price: {:.10} SOL/token", result.exit_price);
                                    
                                    // Send ExecutionConfirmation to Brain
                                    let tx_sig_bytes: [u8; 32] = bs58::decode(&result.signature)
                                        .into_vec()
                                        .unwrap_or_default()
                                        .get(..32)
                                        .and_then(|s| s.try_into().ok())
                                        .unwrap_or([0u8; 32]);
                                    
                                    let confirmation = ExecutionConfirmation::new_success(
                                        decision.mint,
                                        1, // SELL side
                                        decision.size_lamports,
                                        result.exit_price,
                                        tx_sig_bytes,
                                    );
                                    
                                    if let Err(e) = confirmation_socket_clone.send_to(
                                        &confirmation.to_bytes(),
                                        &brain_addr_clone
                                    ).await {
                                        error!("❌ Failed to send SELL confirmation to Brain: {}", e);
                                    } else {
                                        info!("📡 Sent SELL confirmation to Brain: {}", &mint_str[..12]);
                                    }
                                }
                                Err(e) => {
                                    error!("❌ SELL failed for {}: {}", &mint_str[..12], e);
                                    
                                    // Send failure confirmation to Brain
                                    let confirmation = ExecutionConfirmation::new_failure(decision.mint, 1);
                                    if let Err(send_err) = confirmation_socket_clone.send_to(
                                        &confirmation.to_bytes(),
                                        &brain_addr_clone
                                    ).await {
                                        error!("❌ Failed to send SELL failure confirmation: {}", send_err);
                                    }
                                }
                            }
                        }
                            }
                            
                            advice_bus::MessageType::Advisory(advisory) => {
                                // Handle Advisory messages (SOL price updates, etc.)
                                match advisory {
                                    advice_bus::Advisory::SolPriceUpdate { price_cents, timestamp_secs, source, .. } => {
                                        let price_usd = price_cents as f64 / 100.0;
                                        let source_name = match source {
                                            1 => "Pyth",
                                            2 => "Jupiter",
                                            3 => "Fallback",
                                            _ => "Unknown",
                                        };
                                        
                                        info!("📊 RECEIVED SolPriceUpdate: ${:.2} from {} (ts: {})", 
                                            price_usd, source_name, timestamp_secs);
                                        
                                        // Update the cache used by trading engine
                                        trading::update_sol_price_cache(price_usd).await;
                                    }
                                    _ => {
                                        debug!("Received other advisory type: {:?}", advisory);
                                    }
                                }
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
    info!("   Listening for TradeDecisions from Brain on port {}", config.advice_bus_port);
    info!("   Sending ExecutionConfirmations back to Brain on port {}", config.brain_telemetry_port);
    info!("");
    
    // Main loop - keep process alive
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        debug!("📊 Executor running, listening for trade decisions from Brain");
    }
}
