use anyhow::Result;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::time::{interval, Duration};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signature::Signature;

mod config;
mod decoder;
mod heat_calculator;
mod udp_publisher;
mod transaction_monitor;
mod alpha_wallet_manager;
mod watch_signature;  // Basic WatchSignature message and tracker
mod watch_sig_enhanced;  // ENHANCED: WatchSigEnhanced with trade metadata
mod watch_listener;   // UDP listener for basic signature registration
mod watch_listener_enhanced;  // ENHANCED: UDP listener supporting both message types
mod tx_confirmed;     // Basic TxConfirmed message for confirmation notifications
mod tx_confirmed_context;  // ENHANCED: TxConfirmedContext with Δ-window data
mod confirmation_broadcaster;  // NEW: Broadcasts TxConfirmedContext with Δ-window analysis
mod exit_advice;      // NEW: ExitAdvice message for profit target/stop-loss alerts
mod position_update;  // NEW: PositionUpdate message for real-time P&L tracking
mod position_tracker; // NEW: Tracks active positions for P&L monitoring
mod manual_exit;      // NEW: ManualExitNotification for user manual exits

use config::Config;
use decoder::TransactionDecoder;
use heat_calculator::HeatCalculator;
use transaction_monitor::TransactionMonitor;
use udp_publisher::UdpPublisher;
use alpha_wallet_manager::AlphaWalletManager;
use watch_signature::SignatureTracker;
use watch_sig_enhanced::SignatureTrackerEnhanced;
use watch_listener_enhanced::WatchSignatureListenerEnhanced;
use position_tracker::PositionTracker;
use tx_confirmed::TxConfirmed;

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
    
    let alpha_wallet_manager = Arc::new(AlphaWalletManager::new(config.database.sqlite_path.clone()));
    
    let heat_calculator = Arc::new(HeatCalculator::new(
        config.monitoring.transaction_window_secs,
        config.thresholds.whale_threshold_sol,
        config.thresholds.bot_repeat_threshold,
    ));
    
    let udp_publisher = Arc::new(UdpPublisher::new(
        &config.udp.bind_address,
        config.udp.brain_port,
        config.udp.brain_confirmation_port,
    )?);
    
    // Initialize signature trackers (basic and enhanced)
    let signature_tracker = Arc::new(SignatureTracker::new());
    let signature_tracker_enhanced = Arc::new(SignatureTrackerEnhanced::new());
    
    // Initialize position tracker for P&L monitoring
    let position_tracker = Arc::new(PositionTracker::new());
    
    // Initialize RPC client for signature polling backup
    let rpc_client = Arc::new(RpcClient::new(config.rpc.url.clone()));

    info!("✅ All components initialized");

    // Start alpha wallet management
    alpha_wallet_manager.start_background_updates().await;
    info!("✅ Alpha wallet manager started");
    
    // Start Enhanced WatchSignature listener (receives signature registration from Executor)
    // Supports both basic WatchSignature and enhanced WatchSigEnhanced messages
    let watch_listener_handle = {
        let basic_tracker = signature_tracker.clone();
        let enhanced_tracker = signature_tracker_enhanced.clone();
        let pos_tracker = position_tracker.clone();
        let bind_addr = format!("{}:{}", config.udp.bind_address, config.udp.watch_listen_port);
        tokio::spawn(async move {
            match WatchSignatureListenerEnhanced::new(&bind_addr, basic_tracker, enhanced_tracker, pos_tracker).await {
                Ok(listener) => {
                    if let Err(e) = listener.listen().await {
                        error!("❌ Enhanced WatchSignature listener failed: {}", e);
                    }
                }
                Err(e) => {
                    error!("❌ Failed to start Enhanced WatchSignature listener: {}", e);
                }
            }
        })
    };
    
    // Spawn signature tracker cleanup task (remove stale signatures every 30s)
    let cleanup_handle = {
        let tracker = signature_tracker.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(30));
            loop {
                tick.tick().await;
                tracker.cleanup_stale(60).await;  // Remove signatures older than 60s
            }
        })
    };
    
    // Spawn RPC polling task (CRITICAL: backup for unreliable WebSocket)
    // Polls watched signatures every 2 seconds to detect confirmations
    let rpc_polling_handle = {
        let tracker = signature_tracker_enhanced.clone();
        let rpc = rpc_client.clone();
        let position_tracker = position_tracker.clone();
        let config = config.clone();
        
        tokio::spawn(async move {
            info!("🔄 RPC signature polling task started (interval: 2s)");
            
            // Create UDP sockets for sending TxConfirmed messages
            let executor_socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await
                .expect("Failed to bind RPC polling executor socket");
            let brain_socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await
                .expect("Failed to bind RPC polling brain socket");
            
            let executor_addr = format!("127.0.0.1:{}", config.udp.executor_confirmed_port);
            let brain_addr = format!("127.0.0.1:{}", config.udp.brain_confirmation_port);
            
            let mut tick = interval(Duration::from_secs(2));
            
            loop {
                tick.tick().await;
                
                let signatures = tracker.get_all_signatures().await;
                if signatures.is_empty() {
                    continue;
                }
                
                debug!("🔍 Polling {} signatures via RPC", signatures.len());
                
                // Parse signatures
                let mut sig_objects = Vec::new();
                for sig_str in &signatures {
                    match sig_str.parse::<Signature>() {
                        Ok(sig) => sig_objects.push(sig),
                        Err(e) => {
                            warn!("⚠️  Invalid signature format: {} - {}", &sig_str[..12], e);
                            continue;
                        }
                    }
                }
                
                if sig_objects.is_empty() {
                    continue;
                }
                
                // Batch query signature statuses
                match rpc.get_signature_statuses(&sig_objects).await {
                    Ok(response) => {
                        for (idx, status_opt) in response.value.iter().enumerate() {
                            if let Some(status) = status_opt {
                                let sig_str = &signatures[idx];
                                
                                // Check if confirmed or finalized
                                if status.confirmation_status.is_some() {
                                    info!("✅ RPC POLL: Signature {} confirmed via RPC backup", &sig_str[..12]);
                                    
                                    // Remove from tracker
                                    if let Some(watch) = tracker.remove(sig_str).await {
                                        let mint_str = watch.mint_str();
                                        
                                        // Determine status
                                        let tx_status = if status.err.is_some() {
                                            TxConfirmed::STATUS_FAILED
                                        } else {
                                            TxConfirmed::STATUS_SUCCESS
                                        };
                                        
                                        // Create confirmation message
                                        let tx_confirmed = TxConfirmed::new(
                                            watch.signature,
                                            watch.mint,
                                            watch.trade_id,
                                            watch.side,
                                            tx_status,
                                        );
                                        
                                        let bytes = tx_confirmed.to_bytes();
                                        
                                        // Send to Executor (port 45132)
                                        if let Err(e) = executor_socket.send_to(&bytes, &executor_addr).await {
                                            error!("❌ Failed to send TxConfirmed to Executor (RPC poll): {}", e);
                                        } else {
                                            info!("📤 Sent TxConfirmed to Executor (RPC poll): {} | {} | mint: {}", 
                                                  &sig_str[..12], tx_confirmed.status_str(), &mint_str[..12]);
                                        }
                                        
                                        // Send to Brain (port 45115)
                                        if let Err(e) = brain_socket.send_to(&bytes, &brain_addr).await {
                                            error!("❌ Failed to send TxConfirmed to Brain (RPC poll): {}", e);
                                        } else {
                                            info!("📤 Sent TxConfirmed to Brain (RPC poll): {} | {}", 
                                                  &sig_str[..12], tx_confirmed.status_str());
                                        }
                                        
                                        // Track position if BUY
                                        if watch.side == 0 {
                                            position_tracker.add_position(watch).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("⚠️  RPC batch status query failed: {}", e);
                    }
                }
            }
        })
    };
    
    // Spawn position update task (send updates every 5s for all tracked positions)
    let position_update_handle = {
        let pos_tracker = position_tracker.clone();
        let publisher = udp_publisher.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                
                // Send periodic updates for all positions
                let updates = pos_tracker.get_all_updates(150.0).await; // TODO: Get SOL price from oracle
                for update in updates {
                    // Copy packed fields to avoid alignment issues
                    let pnl_usd = update.realized_pnl_usd;
                    let pnl_pct = update.pnl_percent;
                    
                    if let Err(e) = publisher.send_position_update(&update) {
                        error!("❌ Failed to send periodic PositionUpdate: {}", e);
                    } else {
                        debug!("📊 Sent periodic PositionUpdate: {} | P&L: ${:.2} ({:.1}%)",
                               &update.mint_str()[..8], pnl_usd, pnl_pct);
                    }
                }
            }
        })
    };

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
        let tracker = signature_tracker.clone();
        let position_tracker = position_tracker.clone();
        let config = config.clone();
        
        tokio::spawn(async move {
            info!("🔄 Transaction processor started");
            
            // Create UDP sockets for sending tx_confirmed messages
            let executor_socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await
                .expect("Failed to bind tx_confirmed executor socket");
            let brain_socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await
                .expect("Failed to bind tx_confirmed brain socket");
            
            let executor_addr = format!("{}:{}", config.udp.bind_address, config.udp.executor_confirmed_port);  // Executor TxConfirmed listener (45132)
            let brain_addr = format!("{}:45115", config.udp.bind_address);     // Brain confirmation listener (45115)
            
            while let Some(raw_tx) = tx_receiver.recv().await {
                debug!("📦 Processing transaction: {}", &raw_tx.signature[..12]);
                
                // Check if this signature is being watched
                if tracker.is_watched(&raw_tx.signature).await {
                    info!("✅ CONFIRMED transaction detected: {}", &raw_tx.signature[..12]);
                    
                    // Remove from tracker and get watch data
                    if let Some(watch) = tracker.remove(&raw_tx.signature).await {
                        // Determine status (for now assume SUCCESS - would check meta.err in real impl)
                        let status = TxConfirmed::STATUS_SUCCESS;
                        
                        // Create confirmation message
                        let tx_confirmed = TxConfirmed::new(
                            watch.signature,
                            watch.mint,
                            watch.trade_id,
                            watch.side,
                            status,
                        );
                        
                        let bytes = tx_confirmed.to_bytes();
                        
                        // Send to Executor
                        if let Err(e) = executor_socket.send_to(&bytes, &executor_addr).await {
                            error!("❌ Failed to send tx_confirmed to Executor: {}", e);
                        } else {
                            info!("📡 Sent tx_confirmed to Executor: {} {} (trade_id: {})",
                                  tx_confirmed.side_str(), 
                                  tx_confirmed.status_str(),
                                  &tx_confirmed.trade_id_str()[..8]);
                        }
                        
                        // Send to Brain for immediate confirmation (parallel to Executor)
                        if let Err(e) = brain_socket.send_to(&bytes, &brain_addr).await {
                            error!("❌ Failed to send tx_confirmed to Brain: {}", e);
                        } else {
                            debug!("📡 Sent tx_confirmed to Brain: {} {}", 
                                   tx_confirmed.side_str(), tx_confirmed.status_str());
                        }
                    }
                } else {
                    // NOT a watched signature - check if it's a manual exit for a tracked position
                    // Parse the raw transaction to extract mint address
                    // For Pump.fun transactions, mint is typically in accounts[2]
                    
                    // Log ALL untracked confirmed transactions for debugging
                    debug!("🔍 Untracked transaction: {} | program: {} | accounts: {} | data_len: {}",
                           &raw_tx.signature[..12], &raw_tx.program_id[..8], 
                           raw_tx.accounts.len(), raw_tx.data.len());
                    
                    if raw_tx.program_id == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P" && raw_tx.accounts.len() > 2 {
                        let mint_str = &raw_tx.accounts[2];
                        
                        debug!("🔍 Pump.fun tx | mint: {} | checking if tracked...", &mint_str[..8]);
                        
                        // Check if we're tracking a position for this mint
                        if position_tracker.has_position(mint_str).await {
                            debug!("✅ Position tracked! Checking if SELL...");
                            
                            // Try to decode instruction to determine if it's a SELL
                            // Pump.fun SELL instruction discriminator: [51, 230, 133, 164, 1, 127, 131, 173]
                            if raw_tx.data.len() >= 8 {
                                let discriminator = &raw_tx.data[0..8];
                                debug!("🔍 Instruction discriminator: {:?}", discriminator);
                                
                                let is_sell = discriminator == [51, 230, 133, 164, 1, 127, 131, 173];
                                
                                if is_sell {
                                    info!("🔍 Manual SELL detected for tracked mint: {}", &mint_str[..8]);
                                    
                                    // Parse mint to [u8; 32]
                                    if let Ok(mint_bytes) = bs58::decode(mint_str).into_vec() {
                                        if mint_bytes.len() == 32 {
                                            let mut mint_array = [0u8; 32];
                                            mint_array.copy_from_slice(&mint_bytes);
                                            
                                            // Parse signature to [u8; 64]
                                            if let Ok(sig_bytes) = bs58::decode(&raw_tx.signature).into_vec() {
                                                if sig_bytes.len() == 64 {
                                                    let mut sig_array = [0u8; 64];
                                                    sig_array.copy_from_slice(&sig_bytes);
                                                    
                                                    // Extract exit price from SELL instruction using decoder
                                                    let exit_price_lamports = if let Ok(pubkey_accounts) = raw_tx.accounts.iter()
                                                        .map(|a| a.parse::<solana_sdk::pubkey::Pubkey>())
                                                        .collect::<Result<Vec<_>, _>>() {
                                                        
                                                        // Decode SELL instruction
                                                        if let Ok(Some(sell_ix)) = decoder.parse_pump_sell_instruction(&raw_tx.data, &pubkey_accounts) {
                                                            // Use min_sol_out as exit price approximation
                                                            // This is the minimum SOL the user expects to receive
                                                            debug!("💵 Decoded SELL: {} tokens for min {} lamports SOL",
                                                                   sell_ix.token_amount, sell_ix.min_sol_out);
                                                            sell_ix.min_sol_out
                                                        } else {
                                                            warn!("⚠️ Failed to decode SELL instruction, using 0");
                                                            0u64
                                                        }
                                                    } else {
                                                        warn!("⚠️ Failed to parse account pubkeys, using 0");
                                                        0u64
                                                    };
                                                    
                                                    // Get SOL price (TODO: integrate with oracle or data-mining)
                                                    // For now, use a reasonable estimate
                                                    let sol_price_usd = 200.0;  // Conservative estimate
                                                    
                                                    // Check for manual exit and calculate P&L
                                                    if let Some(manual_exit) = position_tracker.check_manual_exit(
                                                        &mint_array,
                                                        &sig_array,
                                                        exit_price_lamports,
                                                        sol_price_usd,
                                                    ).await {
                                                        // Copy values to avoid packed struct alignment issues
                                                        let pnl_usd = manual_exit.realized_pnl_usd;
                                                        let pnl_pct = manual_exit.pnl_percent;
                                                        
                                                        info!("💰 Manual exit P&L calculated: ${:.2} ({:.1}%)",
                                                              pnl_usd, pnl_pct);
                                                        
                                                        // Send notifications
                                                        if let Err(e) = udp_publisher.send_manual_exit(&manual_exit) {
                                                            error!("❌ Failed to send manual exit notification: {}", e);
                                                        }
                                                        
                                                        // Remove position from tracker
                                                        position_tracker.remove_position_by_str(mint_str).await;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            debug!("❌ Position NOT tracked for mint: {} | current tracked positions: {}",
                                   &mint_str[..12], position_tracker.count().await);
                        }
                    }
                }
                
                // Filter out stale transactions (older than 2 seconds)
                let current_timestamp = chrono::Utc::now().timestamp();
                let tx_age_seconds = current_timestamp - raw_tx.timestamp;
                
                if tx_age_seconds > 2 {
                    debug!("⏰ Skipping stale transaction: {} (age: {}s)", 
                           &raw_tx.signature[..12], tx_age_seconds);
                    continue;
                }
                
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
                    
                    // Publish to executor (with jitter)
                    if let Err(e) = udp_publisher.send_hot_signal_to_brain(&hot_signal).await {
                        error!("Failed to publish hot signal: {}", e);
                    }
                }
            }
        })
    };

    // Spawn heat calculation periodic task
    let heat_handle = {
        let heat_calculator = heat_calculator.clone();
        let udp_publisher = udp_publisher.clone();
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(5));
            
            loop {
                tick.tick().await;
                
                let heat = heat_calculator.calculate_heat();
                debug!("🌡️  Heat: {} | TxRate: {:.2}/s | Whale: {:.2} SOL | Bot: {:.1}%",
                    heat.score, heat.tx_rate, heat.whale_activity, heat.bot_density * 100.0);
                
                // Send heat to brain via UDP
                if let Err(e) = udp_publisher.send_heat_to_brain(&heat) {
                    error!("Failed to send heat to Brain: {}", e);
                }
            }
        })
    };

    info!("✅ All systems initialized");
    info!("🚀 Mempool monitoring active");
    info!("📡 Publishing hot signals to Brain on {}:{}", config.udp.bind_address, config.udp.brain_confirmation_port);
    info!("🎧 Listening for enhanced signature registration on {}:{}", config.udp.bind_address, config.udp.watch_listen_port);
    info!("📊 Position tracking enabled - sending P&L updates every 5s");

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
        _ = watch_listener_handle => {
            error!("Enhanced WatchSignature listener ended unexpectedly");
        }
        _ = cleanup_handle => {
            error!("Signature cleanup task ended unexpectedly");
        }
        _ = rpc_polling_handle => {
            error!("RPC polling task ended unexpectedly");
        }
        _ = position_update_handle => {
            error!("Position update task ended unexpectedly");
        }
    }

    Ok(())
}
