//! 🧠 Brain Service - Trading Decision Engine
//!
//! Receives advice messages from collectors (RankBot, AdvisorBot, Heat Sentinel),
//! applies intelligent scoring and validation logic, enforces guardrails,
//! and sends TradeDecision messages to the Executor.
//!
//! ## Architecture
//! - Advice Bus (UDP 45100): Receives advice from collectors
//! - Decision Bus (UDP 45110): Sends decisions to executor
//! - Feature Caches: Lightning-fast in-memory lookups
//! - Decision Engine: Scoring, validation, guardrails
//! - Metrics: Prometheus endpoint on port 9090

mod config;
mod udp_bus;
mod feature_cache;
mod decision_engine;
mod metrics;

use anyhow::{Result, Context};
use log::{info, warn, error};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

use config::Config;
use udp_bus::{AdviceBusReceiver, DecisionBusSender, AdviceMessage, LateOpportunityAdvice, CopyTradeAdvice};
use feature_cache::{MintCache, WalletCache};
use decision_engine::{
    FollowThroughScorer, TradeValidator, Guardrails, DecisionLogger, DecisionLogEntry,
    TriggerEngine, TriggerType,
};

// Type aliases for shorter names
type Scorer = FollowThroughScorer;
type Validator = TradeValidator;

/// Global SOL price in cents (e.g., 19444 = $194.44)
static SOL_PRICE_CENTS: AtomicU32 = AtomicU32::new(19344); // Default $193.44

/// Get current SOL price in USD
fn get_sol_price_usd() -> f64 {
    SOL_PRICE_CENTS.load(Ordering::Relaxed) as f64 / 100.0
}

/// Update SOL price from oracle
fn update_sol_price(price_usd: f32) {
    let cents = (price_usd * 100.0) as u32;
    SOL_PRICE_CENTS.store(cents, Ordering::Relaxed);
    metrics::update_sol_price(price_usd);
    info!("💵 SOL price updated: ${:.2}", price_usd);
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logger
    env_logger::init();
    
    // Initialize metrics
    metrics::init_metrics();
    info!("✅ Metrics: Initialized");
    
    // Spawn metrics HTTP server on port 9090
    tokio::spawn(async {
        if let Err(e) = metrics::start_metrics_server(9090).await {
            error!("❌ Metrics server error: {}", e);
        }
    });
    info!("✅ Metrics: Server started on port 9090");
    
    // Load configuration
    dotenv::dotenv().ok();
    let config = Arc::new(Config::from_env().context("Failed to load configuration")?);
    info!("✅ Configuration: Loaded");
    
    // Print startup banner
    print_banner(&config);
    
    // Initialize database connections
    info!("🔌 Connecting to databases...");
    
    // SQLite (LaunchTracker data)
    let sqlite_path = config.database.sqlite_path.clone();
    let sqlite_conn = tokio::task::spawn_blocking(move || {
        rusqlite::Connection::open(&sqlite_path)
    })
    .await?
    .context("Failed to open SQLite database")?;
    info!("✅ SQLite: Connected ({})", config.database.sqlite_path.display());
    
    // PostgreSQL (WalletTracker data) - Optional for now
    let pg_config = config.database.postgres_connection_string();
    let pg_client_opt = match tokio_postgres::connect(&pg_config, tokio_postgres::NoTls).await {
        Ok((pg_client, pg_connection)) => {
            // Spawn PostgreSQL connection handler
            tokio::spawn(async move {
                if let Err(e) = pg_connection.await {
                    error!("❌ PostgreSQL connection error: {}", e);
                }
            });
            info!("✅ PostgreSQL: Connected");
            Some(pg_client)
        }
        Err(e) => {
            warn!("⚠️  PostgreSQL not available: {}. Wallet cache will be empty.", e);
            warn!("   (This is OK for testing - only affects copy trade decisions)");
            None
        }
    };
    
    // Initialize feature caches
    info!("🗂️  Initializing feature caches...");
    let mint_cache = Arc::new(MintCache::new(config.database.sqlite_path.to_string_lossy().to_string()));
    let wallet_cache = Arc::new(WalletCache::new(config.database.postgres_connection_string()));
    info!("✅ Caches: Initialized");
    
    // Start cache updater tasks
    let mint_cache_updater = mint_cache.clone();
    let sqlite_conn_arc = Arc::new(tokio::sync::Mutex::new(sqlite_conn));
    let sqlite_for_mint = sqlite_conn_arc.clone();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(e) = update_mint_cache(&mint_cache_updater, &sqlite_for_mint).await {
                warn!("⚠️  Mint cache update failed: {}", e);
            } else {
                info!("♻️  Mint cache updated ({} entries)", mint_cache_updater.len());
            }
        }
    });
    
    let wallet_cache_updater = wallet_cache.clone();
    let pg_client_arc_opt = pg_client_opt.map(|c| Arc::new(tokio::sync::Mutex::new(c)));
    
    if let Some(pg_for_wallet) = pg_client_arc_opt {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                if let Err(e) = update_wallet_cache(&wallet_cache_updater, &pg_for_wallet).await {
                    warn!("⚠️  Wallet cache update failed: {}", e);
                } else {
                    info!("♻️  Wallet cache updated ({} entries)", wallet_cache_updater.len());
                }
            }
        });
        info!("✅ Cache updaters: Started (30s interval)");
    } else {
        info!("⚠️  Wallet cache updater: Skipped (PostgreSQL not available)");
        info!("✅ Mint cache updater: Started (30s interval)");
    }
    
    // Initialize decision engine components
    info!("🧠 Initializing decision engine...");
    let trigger_engine = TriggerEngine::new();
    let scorer = Scorer::new();
    let validator = Validator::new();
    
    // Configure guardrails from config
    let guardrail_config = decision_engine::guardrails::GuardrailConfig {
        loss_backoff_window_secs: config.guardrails.loss_backoff_window_secs,
        loss_backoff_threshold: config.guardrails.loss_backoff_threshold,
        loss_backoff_duration_secs: config.guardrails.loss_backoff_pause_secs,
        max_concurrent_positions: config.guardrails.max_concurrent_positions,
        max_advisor_positions: config.guardrails.max_advisor_positions,
        advisor_rate_limit_secs: config.guardrails.advisor_rate_limit_ms / 1000, // Convert ms to seconds
        min_decision_interval_ms: config.guardrails.rate_limit_ms,
        wallet_cooling_period_secs: config.guardrails.wallet_cooling_secs,
        tier_a_bypass_cooling: true, // Always allow Tier A bypass if profitable
    };
    let mut guardrails = Guardrails::with_config(guardrail_config);
    
    let logger = DecisionLogger::new(&config.logging.decision_log_path)
        .context("Failed to create decision logger")?;
    info!("✅ Decision engine: Ready");
    
    // Initialize position tracker
    let position_tracker = Arc::new(tokio::sync::RwLock::new(
        decision_engine::PositionTracker::new(config.guardrails.max_concurrent_positions)
    ));
    info!("✅ Position tracker: Initialized (max: {})", config.guardrails.max_concurrent_positions);
    
    // Initialize position sizer
    let sizer_config = decision_engine::PositionSizerConfig {
        strategy: decision_engine::SizingStrategy::ConfidenceScaled {
            min_size_sol: 0.05,
            max_size_sol: 0.2,
        },
        max_position_sol: 0.5,
        min_position_sol: 0.05,
        portfolio_sol: 10.0,  // TODO: Get from wallet balance
        max_position_pct: 5.0,
        risk_per_trade_pct: 2.0,
        scale_down_near_limit: true,
    };
    let position_sizer = Arc::new(decision_engine::PositionSizer::new(sizer_config));
    info!("✅ Position sizer: Initialized");
    
    // Initialize UDP communication
    info!("📡 Setting up UDP communication...");
    let advice_receiver = AdviceBusReceiver::new().await
        .context("Failed to create Advice Bus receiver")?;
    let target_addr = format!("127.0.0.1:{}", config.network.decision_bus_port)
        .parse()
        .context("Invalid decision bus address")?;
    let decision_sender = Arc::new(DecisionBusSender::new(target_addr).await
        .context("Failed to create Decision Bus sender")?);
    info!("✅ UDP: Advice Bus (port {}), Decision Bus (port {})", 
          config.network.advice_bus_port, config.network.decision_bus_port);
    
    info!("🚀 Brain service started - Listening for advice...\n");
    
    // Start receiving advice messages
    let mut advice_rx = advice_receiver.start().await;
    
    // Spawn position monitoring task
    let position_tracker_monitor = position_tracker.clone();
    let mint_cache_monitor = mint_cache.clone();
    let decision_sender_monitor = decision_sender.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        loop {
            interval.tick().await;
            
            // Check all active positions
            let tracker = position_tracker_monitor.read().await;
            let positions = tracker.get_all();
            
            for pos in positions {
                // Parse mint from bs58 string to Pubkey
                if let Ok(mint_pubkey) = bs58::decode(&pos.mint).into_vec() {
                    if mint_pubkey.len() == 32 {
                        let mut mint_bytes = [0u8; 32];
                        mint_bytes.copy_from_slice(&mint_pubkey);
                        let mint_pk = solana_sdk::pubkey::Pubkey::new_from_array(mint_bytes);
                        
                        // Get latest features for this mint
                        if let Some(features) = mint_cache_monitor.get(&mint_pk) {
                            let sol_price = 150.0; // TODO: Get real SOL price
                            
                            // Check if position should exit
                            if let Some((reason, position)) = tracker.check_position(&pos.mint, &features, sol_price) {
                                info!("🚨 EXIT SIGNAL: {} | reason: {}", &pos.mint[..8], reason.to_string());
                                
                                // Calculate exit size based on reason
                                let exit_percent = match &reason {
                                    decision_engine::ExitReason::ProfitTarget { exit_percent, .. } => *exit_percent,
                                    decision_engine::ExitReason::StopLoss { exit_percent, .. } => *exit_percent,
                                    decision_engine::ExitReason::TimeDecay { exit_percent, .. } => *exit_percent,
                                    decision_engine::ExitReason::VolumeDrop { exit_percent, .. } => *exit_percent,
                                    decision_engine::ExitReason::Emergency { exit_percent, .. } => *exit_percent,
                                };
                                
                                let exit_size_sol = position.size_sol * (exit_percent as f64 / 100.0);
                                let exit_size_lamports = (exit_size_sol * 1e9) as u64;
                                
                                // Create SELL decision
                                let sell_decision = crate::udp_bus::TradeDecision::new_sell(
                                    mint_bytes,
                                    exit_size_lamports,
                                    300, // 3% slippage for exits (wider)
                                    position.entry_confidence,
                                );
                                
                                // Send to executor
                                if let Err(e) = decision_sender_monitor.send_decision(&sell_decision).await {
                                    warn!("❌ Failed to send SELL decision: {}", e);
                                } else {
                                    info!("✅ SELL DECISION SENT: {} ({:.3} SOL, {}%)", 
                                          &pos.mint[..8], exit_size_sol, exit_percent);
                                    metrics::record_decision_sent();
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    
    // Main decision loop
    while let Some(advice) = advice_rx.recv().await {
        // Record that we received an advice message
        metrics::record_advice_received();
        
        // Start timing the decision processing
        let _timer = metrics::DecisionTimer::start();
        
        // Handle different advice types
        match advice {
            AdviceMessage::SolPriceUpdate(price) => {
                update_sol_price(price.price_usd);
                // Not a trading decision
            }
            
            AdviceMessage::ExtendHold(_) | AdviceMessage::WidenExit(_) => {
                // These are position management advisories, not entry triggers
                // TODO: Implement position management logic
            }
            
            AdviceMessage::LateOpportunity(ref late) => {
                info!("🎯 Late opportunity: {}", hex::encode(&late.mint[..4]));
                
                if let Err(e) = process_late_opportunity(
                    late,
                    &mint_cache,
                    &scorer,
                    &validator,
                    &mut guardrails,
                    &logger,
                    &decision_sender,
                    &position_tracker,
                    &position_sizer,
                    &config,
                ).await {
                    warn!("⚠️  Failed to process late opportunity: {}", e);
                }
            }
            
            AdviceMessage::CopyTrade(ref copy) => {
                info!("👥 Copy trade: {}", hex::encode(&copy.mint[..4]));
                
                if let Err(e) = process_copy_trade(
                    copy,
                    &mint_cache,
                    &wallet_cache,
                    &scorer,
                    &validator,
                    &mut guardrails,
                    &logger,
                    &decision_sender,
                    &position_tracker,
                    &position_sizer,
                    &config,
                ).await {
                    warn!("⚠️  Failed to process copy trade: {}", e);
                }
            }
        }
    }
    
    Ok(())
}

/// Process a late opportunity advice message
async fn process_late_opportunity(
    late: &LateOpportunityAdvice,
    mint_cache: &MintCache,
    scorer: &FollowThroughScorer,
    validator: &TradeValidator,
    guardrails: &mut Guardrails,
    logger: &DecisionLogger,
    sender: &Arc<DecisionBusSender>,
    position_tracker: &Arc<tokio::sync::RwLock<decision_engine::PositionTracker>>,
    position_sizer: &Arc<decision_engine::PositionSizer>,
    config: &Config,
) -> Result<()> {
    use metrics::{DecisionPathway, RejectionReason};
    
    metrics::record_decision_pathway(DecisionPathway::NewLaunch);
    
    // Convert mint bytes to Pubkey
    let mint = Pubkey::new_from_array(late.mint);
    
    // 1. Lookup mint features from cache
    let mint_features = match mint_cache.get(&mint) {
        Some(features) => {
            metrics::record_cache_access(metrics::CacheType::Mint, true);
            features
        }
        None => {
            metrics::record_cache_access(metrics::CacheType::Mint, false);
            warn!("❌ Mint not in cache: {}", hex::encode(&late.mint[..4]));
            metrics::record_decision_rejected(RejectionReason::Validation);
            return Ok(());
        }
    };
    
    // Check if data is stale
    if mint_features.is_stale() {
        warn!("⏱️  Stale data for mint: {}", hex::encode(&late.mint[..4]));
        metrics::record_decision_rejected(RejectionReason::Validation);
        return Ok(());
    }
    
    // 2. Score the opportunity
    let score_components = scorer.calculate(&mint_features);
    let confidence = score_components.total_score;
    
    info!("📊 Score: {} (buyers={}, vol={}, quality={})",
          confidence,
          score_components.buyer_score,
          score_components.volume_score,
          score_components.wallet_quality_score);
    
    // Check minimum confidence threshold
    if confidence < config.decision.min_decision_conf {
        info!("🚫 Below confidence threshold: {} < {}", confidence, config.decision.min_decision_conf);
        metrics::record_decision_rejected(RejectionReason::LowConfidence);
        return Ok(());
    }
    
    // 3. Calculate position size dynamically
    let tracker = position_tracker.read().await;
    let active_count = tracker.count();
    let total_exposure = tracker.get_all()
        .iter()
        .map(|p| p.size_sol)
        .sum::<f64>();
    drop(tracker); // Release lock
    
    let position_size_sol = position_sizer.calculate_size(
        confidence,
        active_count,
        config.guardrails.max_concurrent_positions,
        total_exposure,
    );
    let position_size_usd = position_size_sol * get_sol_price_usd();
    let position_size_lamports = (position_size_sol * 1e9) as u64;
    
    info!("💰 Position size: {:.3} SOL (${:.2}) | active: {}/{} | exposure: {:.2} SOL ({:.1}%)",
          position_size_sol,
          position_size_usd,
          active_count,
          config.guardrails.max_concurrent_positions,
          total_exposure,
          position_sizer.get_portfolio_utilization(total_exposure));
    
    // Check portfolio heat before proceeding
    if let Err(e) = position_sizer.check_portfolio_heat(total_exposure, position_size_sol) {
        warn!("🔥 {}", e);
        metrics::record_decision_rejected(RejectionReason::Guardrails);
        return Ok(());
    }
    
    // 4. Validate the trade
    let validated = match validator.validate(
        mint,
        &mint_features,
        position_size_usd,
        150, // 1.5% slippage
        confidence,
        None, // No creator check for now
    ) {
        Ok(v) => v,
        Err(e) => {
            info!("❌ Validation failed: {}", e);
            metrics::record_decision_rejected(RejectionReason::Validation);
            return Ok(());
        }
    };
    
    info!("✅ Validated: fees=${:.4}, impact={:.2}%, tp=${:.2}",
          validated.estimated_fees_usd,
          validated.estimated_impact_pct,
          validated.min_profit_target_usd);
    
    // 5. Check guardrails
    if let Err(reason) = guardrails.check_decision_allowed(
        3, // late opportunity
        &late.mint,
        None,
        None,
    ) {
        info!("🛡️  Blocked by guardrails: {}", reason);
        metrics::record_guardrail_block(metrics::GuardrailType::RateLimit);
        metrics::record_decision_rejected(RejectionReason::Guardrails);
        return Ok(());
    }
    
    // Record decision with guardrails for tracking
    guardrails.record_decision(3, &late.mint, None);
    
    // 6. Build trade decision
    let decision = udp_bus::TradeDecision::new_buy(
        late.mint,
        position_size_lamports,
        150, // 1.5% slippage
        confidence,
    );
    
    // 7. Log decision
    let log_entry = DecisionLogEntry {
        decision_id: 0, // Will be assigned by logger
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        mint: hex::encode(&late.mint),
        trigger_type: TriggerType::LateOpportunity,
        side: 0, // BUY
        predicted_fees_usd: validated.estimated_fees_usd,
        predicted_impact_usd: validated.estimated_impact_pct,
        tp_usd: validated.min_profit_target_usd,
        follow_through_score: confidence,
        size_sol: position_size_sol,
        size_usd: position_size_usd,
        confidence,
        expected_ev_usd: validated.expected_value_usd,
        success_probability: 0.0, // TODO: Calculate from validator
        rank: Some(late.follow_through_score),
        wallet: None,
        wallet_tier: None,
    };
    
    logger.log_decision(log_entry)?;
    
    // 8. Send to executor
    sender.send_decision(&decision).await?;
    metrics::record_decision_sent();
    metrics::record_decision_approved();
    
    info!("✅ DECISION SENT: BUY {} ({} SOL, conf={})",
          hex::encode(&late.mint[..8]),
          position_size_sol,
          confidence);
    
    // 9. Track position for exit monitoring
    let entry_position = decision_engine::ActivePosition {
        mint: bs58::encode(&late.mint).into_string(),
        entry_time: std::time::Instant::now(),
        entry_timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        size_sol: position_size_sol,
        size_usd: position_size_usd,
        entry_price_sol: mint_features.current_price,
        tokens: (position_size_sol / mint_features.current_price) * 0.99, // Account for slippage
        entry_confidence: confidence,
        profit_targets: (30.0, 60.0, 100.0), // 30%, 60%, 100% profit targets
        stop_loss_pct: 15.0, // 15% stop loss
        max_hold_secs: 300, // 5 minutes max hold
        trigger_source: "late_opportunity".to_string(),
    };
    
    position_tracker.write().await.add_position(entry_position)?;
    info!("📊 Position tracked: {} for exit monitoring", hex::encode(&late.mint[..8]));
    
    Ok(())
}

/// Process a copy trade advice message
async fn process_copy_trade(
    copy: &CopyTradeAdvice,
    mint_cache: &MintCache,
    wallet_cache: &WalletCache,
    scorer: &FollowThroughScorer,
    validator: &TradeValidator,
    guardrails: &mut Guardrails,
    logger: &DecisionLogger,
    sender: &Arc<DecisionBusSender>,
    position_tracker: &Arc<tokio::sync::RwLock<decision_engine::PositionTracker>>,
    position_sizer: &Arc<decision_engine::PositionSizer>,
    config: &Config,
) -> Result<()> {
    use metrics::{DecisionPathway, RejectionReason};
    
    metrics::record_decision_pathway(DecisionPathway::CopyTrade);
    
    // Convert to Pubkeys
    let mint = Pubkey::new_from_array(copy.mint);
    let wallet = Pubkey::new_from_array(copy.wallet);
    
    // 1. Lookup wallet features
    let wallet_features = match wallet_cache.get(&wallet) {
        Some(features) => {
            metrics::record_cache_access(metrics::CacheType::Wallet, true);
            features
        }
        None => {
            metrics::record_cache_access(metrics::CacheType::Wallet, false);
            warn!("❌ Wallet not in cache: {}", hex::encode(&copy.wallet[..4]));
            metrics::record_decision_rejected(RejectionReason::Validation);
            return Ok(());
        }
    };
    
    // Check wallet tier requirement
    if wallet_features.confidence < config.decision.min_copytrade_confidence {
        info!("� Wallet confidence too low: {} < {}",
              wallet_features.confidence,
              config.decision.min_copytrade_confidence);
        metrics::record_decision_rejected(RejectionReason::LowConfidence);
        return Ok(());
    }
    
    // 2. Lookup mint features
    let mint_features = match mint_cache.get(&mint) {
        Some(features) => {
            metrics::record_cache_access(metrics::CacheType::Mint, true);
            features
        }
        None => {
            metrics::record_cache_access(metrics::CacheType::Mint, false);
            warn!("❌ Mint not in cache: {}", hex::encode(&copy.mint[..4]));
            metrics::record_decision_rejected(RejectionReason::Validation);
            return Ok(());
        }
    };
    
    // 3. Score with wallet quality boost
    let score_components = scorer.calculate(&mint_features);
    let base_confidence = score_components.total_score;
    
    // Boost confidence based on wallet tier (5-15 point bonus)
    let wallet_bonus = ((wallet_features.confidence as f64 / 100.0) * 15.0) as u8;
    let confidence = (base_confidence + wallet_bonus).min(100);
    
    info!("📊 Copy trade score: {} (base={}, wallet_bonus=+{})",
          confidence, base_confidence, wallet_bonus);
    
    // 4. Calculate position size (scale with wallet tier + confidence)
    let tracker = position_tracker.read().await;
    let active_count = tracker.count();
    let total_exposure = tracker.get_all()
        .iter()
        .map(|p| p.size_sol)
        .sum::<f64>();
    drop(tracker);
    
    // Boost confidence slightly for higher tier wallets
    let tier_boosted_confidence = match wallet_features.tier {
        feature_cache::WalletTier::A => (base_confidence + 10).min(100),
        feature_cache::WalletTier::B => (base_confidence + 5).min(100),
        _ => base_confidence,
    };
    
    let position_size_sol = position_sizer.calculate_size(
        tier_boosted_confidence,
        active_count,
        config.guardrails.max_concurrent_positions,
        total_exposure,
    );
    let position_size_usd = position_size_sol * get_sol_price_usd();
    let position_size_lamports = (position_size_sol * 1e9) as u64;
    
    info!("💰 Position size: {:.3} SOL (${:.2}) | tier: {:?} | active: {}/{}",
          position_size_sol,
          position_size_usd,
          wallet_features.tier,
          active_count,
          config.guardrails.max_concurrent_positions);
    
    // Check portfolio heat
    if let Err(e) = position_sizer.check_portfolio_heat(total_exposure, position_size_sol) {
        warn!("🔥 {}", e);
        metrics::record_decision_rejected(RejectionReason::Guardrails);
        return Ok(());
    }
    
    // 5. Validate
    let validated = match validator.validate(
        mint,
        &mint_features,
        position_size_usd,
        150,
        confidence,
        None,
    ) {
        Ok(v) => v,
        Err(e) => {
            info!(
                "❌ Validation failed: {}. Fees={:.4} Impact={:.4}% MinProfit={:.4}",
                e,
                0.0, 0.0, 0.0, // Don't have validated values yet
            );
            metrics::record_decision_rejected(RejectionReason::Validation);
            return Ok(());
        }
    };
    
    // 6. Check guardrails
    if let Err(reason) = guardrails.check_decision_allowed(2, &copy.mint, Some(&copy.wallet), Some(wallet_features.tier as u8)) {
        info!("🛡️  Blocked by guardrails: {}", reason);
        metrics::record_guardrail_block(metrics::GuardrailType::WalletCooling);
        metrics::record_decision_rejected(RejectionReason::Guardrails);
        return Ok(());
    }
    
    // Record decision with guardrails for tracking
    guardrails.record_decision(2, &copy.mint, Some(&copy.wallet));
    
    // 7. Build decision
    let decision = udp_bus::TradeDecision::new_buy(
        copy.mint,
        position_size_lamports,
        150,
        confidence,
    );
    
    // 8. Log
    let log_entry = DecisionLogEntry {
        decision_id: 0,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        mint: hex::encode(&copy.mint),
        trigger_type: TriggerType::CopyTrade,
        side: 0,
        predicted_fees_usd: validated.estimated_fees_usd,
        predicted_impact_usd: validated.estimated_impact_pct,
        tp_usd: validated.min_profit_target_usd,
        follow_through_score: confidence,
        size_sol: position_size_sol,
        size_usd: position_size_usd,
        confidence,
        expected_ev_usd: validated.expected_value_usd,
        success_probability: 0.0, // TODO: Calculate from validator
        rank: None,
        wallet: Some(hex::encode(&copy.wallet)),
        wallet_tier: Some(wallet_features.tier as u8),
    };
    
    logger.log_decision(log_entry)?;
    
    // 9. Send
    sender.send_decision(&decision).await?;
    metrics::record_decision_sent();
    metrics::record_decision_approved();
    
    info!("✅ DECISION SENT: COPY BUY {} from wallet tier {:?} (conf={})",
          hex::encode(&copy.mint[..8]),
          wallet_features.tier,
          confidence);
    
    // 10. Track position for exit monitoring
    let entry_position = decision_engine::ActivePosition {
        mint: bs58::encode(&copy.mint).into_string(),
        entry_time: std::time::Instant::now(),
        entry_timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        size_sol: position_size_sol,
        size_usd: position_size_usd,
        entry_price_sol: mint_features.current_price,
        tokens: (position_size_sol / mint_features.current_price) * 0.99, // Account for slippage
        entry_confidence: confidence,
        profit_targets: (30.0, 60.0, 100.0), // 30%, 60%, 100% profit targets
        stop_loss_pct: 15.0, // 15% stop loss
        max_hold_secs: 300, // 5 minutes max hold
        trigger_source: "copy_trade".to_string(),
    };
    
    position_tracker.write().await.add_position(entry_position)?;
    info!("📊 Position tracked: {} for exit monitoring", hex::encode(&copy.mint[..8]));
    
    Ok(())
}

/// Calculate follow-through score for cache using same algorithm as FollowThroughScorer
/// 
/// This provides better score estimates than simple linear mapping.
/// Returns 0-100 score based on momentum signals.
fn calculate_cache_follow_through_score(
    buyers_2s: u32,
    vol_5s_sol: f64,
    buyers_60s: u32,
) -> u8 {
    // Use same thresholds as FollowThroughScorer
    const MAX_BUYERS_2S: u32 = 20;
    const MAX_VOL_5S: f64 = 50.0;
    const BUYER_WEIGHT: f64 = 0.4;
    const VOLUME_WEIGHT: f64 = 0.4;
    const WALLET_QUALITY_WEIGHT: f64 = 0.2;
    
    // Score buyer momentum (0-100)
    let buyer_score = if buyers_2s == 0 {
        0
    } else if buyers_2s <= 5 {
        ((buyers_2s as f64 / 5.0) * 50.0) as u8
    } else {
        let normalized = (buyers_2s as f64 / MAX_BUYERS_2S as f64).min(1.0);
        let log_score = (normalized.ln() + 1.0).max(0.0);
        (50.0 + log_score * 50.0) as u8
    };
    
    // Score volume momentum (0-100)
    let volume_score = if vol_5s_sol <= 0.0 {
        0
    } else {
        let normalized = (vol_5s_sol / MAX_VOL_5S).min(1.0);
        let sqrt_score = normalized.sqrt();
        (sqrt_score * 100.0) as u8
    };
    
    // Wallet quality proxy: use buyers_60s as proxy for activity level
    // More buyers in 60s suggests more quality participants
    let wallet_quality_score = if buyers_60s == 0 {
        50
    } else {
        // Normalize buyers_60s: 0-100 buyers → 40-90 points
        let normalized = (buyers_60s as f64 / 100.0).min(1.0);
        (40.0 + normalized * 50.0) as u8
    };
    
    // Weighted total
    let total_score = (
        (buyer_score as f64 * BUYER_WEIGHT) +
        (volume_score as f64 * VOLUME_WEIGHT) +
        (wallet_quality_score as f64 * WALLET_QUALITY_WEIGHT)
    ).round() as u8;
    
    total_score.min(100)
}

/// Update mint cache from SQLite
async fn update_mint_cache(
    cache: &MintCache,
    sqlite: &Arc<tokio::sync::Mutex<rusqlite::Connection>>,
) -> Result<()> {
    let _timer = metrics::DbQueryTimer::start();
    
    // Query recent tokens with aggregated window metrics
    let mint_cache_clone = cache.clone();
    let sqlite_clone = sqlite.clone();
    
    let result = tokio::task::spawn_blocking(move || -> Result<usize> {
        let conn = sqlite_clone.blocking_lock();
        
        let query = "
            SELECT 
                t.mint,
                t.launch_block_time,
                MAX(CASE WHEN w.window_sec = 10 THEN w.uniq_buyers ELSE 0 END) as buyers_2s,
                MAX(CASE WHEN w.window_sec = 10 THEN w.vol_sol ELSE 0 END) as vol_5s_sol,
                MAX(CASE WHEN w.window_sec = 60 THEN w.uniq_buyers ELSE 0 END) as buyers_60s,
                MAX(CASE WHEN w.window_sec = 60 THEN w.vol_sol ELSE 0 END) as vol_60s_sol,
                MAX(CASE WHEN w.window_sec = 60 THEN w.num_buys ELSE 0 END) as buys_60s,
                MAX(CASE WHEN w.window_sec = 60 THEN w.num_sells ELSE 1 END) as sells_60s,
                MAX(CASE WHEN w.window_sec = 60 THEN w.close ELSE 0 END) as current_price,
                MAX(w.start_time) as last_update
            FROM tokens t
            LEFT JOIN windows w ON t.mint = w.mint
            WHERE w.start_time > strftime('%s', 'now') - 300
            GROUP BY t.mint
            LIMIT 1000
        ";
        
        let mut stmt = conn.prepare(query)?;
        let mut rows = stmt.query([])?;
        let mut count = 0;
        
        while let Some(row) = rows.next()? {
            let mint_str: String = row.get(0)?;
            let launch_time: i64 = row.get(1)?;
            let buyers_2s: i64 = row.get(2)?;
            let vol_5s_sol: f64 = row.get(3)?;
            let buyers_60s: i64 = row.get(4)?;
            let vol_60s_sol: f64 = row.get(5)?;
            let buys_60s: i64 = row.get(6)?;
            let sells_60s: i64 = row.get(7)?;
            let current_price: f64 = row.get(8)?;
            let last_update: i64 = row.get(9)?;
            
            // Parse mint pubkey
            let mint = match Pubkey::from_str(&mint_str) {
                Ok(pk) => pk,
                Err(e) => {
                    warn!("⚠️  Invalid mint pubkey {}: {}", mint_str, e);
                    continue;
                }
            };
            
            // Calculate age
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let age_since_launch = (now - launch_time).max(0) as u64;
            
            // Calculate buy/sell ratio
            let buys_sells_ratio = if sells_60s > 0 {
                buys_60s as f64 / sells_60s as f64
            } else {
                buys_60s as f64
            };
            
            // Calculate follow-through score (0-100) using scoring algorithm
            // This provides a better proxy than simple linear mapping
            let follow_through_score = calculate_cache_follow_through_score(
                buyers_2s as u32,
                vol_5s_sol,
                buyers_60s as u32,
            );
            
            let features = feature_cache::MintFeatures {
                age_since_launch,
                current_price,
                vol_60s_sol,
                buyers_60s: buyers_60s as u32,
                buys_sells_ratio,
                curve_depth_proxy: 0, // TODO: Calculate from token supply
                follow_through_score,
                last_update: last_update as u64,
                buyers_2s: buyers_2s as u32,
                vol_5s_sol,
            };
            
            mint_cache_clone.insert(mint, features);
            count += 1;
        }
        
        Ok(count)
    }).await??;
    
    info!("📊 Mint cache updated: {} entries", result);
    Ok(())
}

/// Update wallet cache from PostgreSQL
async fn update_wallet_cache(
    cache: &WalletCache,
    pg: &Arc<tokio::sync::Mutex<tokio_postgres::Client>>,
) -> Result<()> {
    let _timer = metrics::DbQueryTimer::start();
    
    // Query wallet performance stats
    let client = pg.lock().await;
    
    let query = "
        SELECT 
            address,
            win_rate_7d,
            avg_hold_time_sec,
            total_pnl_sol,
            num_trades_7d,
            follow_through_rate,
            avg_entry_speed_ms
        FROM wallet_stats
        WHERE num_trades_7d > 5
          AND last_trade_time > NOW() - INTERVAL '7 days'
        ORDER BY win_rate_7d DESC
        LIMIT 500
    ";
    
    let rows = client.query(query, &[]).await?;
    let mut count = 0;
    
    for row in rows {
        let address_str: String = row.get(0);
        let win_rate_7d: f64 = row.get(1);
        let avg_hold_time_sec: i64 = row.get(2);
        let total_pnl_sol: f64 = row.get(3);
        let num_trades_7d: i64 = row.get(4);
        let follow_through_rate: f64 = row.get(5);
        let avg_entry_speed_ms: i64 = row.get(6);
        
        // Parse wallet pubkey
        let wallet = match Pubkey::from_str(&address_str) {
            Ok(pk) => pk,
            Err(e) => {
                warn!("⚠️  Invalid wallet pubkey {}: {}", address_str, e);
                continue;
            }
        };
        
        // Calculate wallet tier using the tier classifier
        let tier = feature_cache::WalletFeatures::classify_tier(
            win_rate_7d,
            total_pnl_sol,
            num_trades_7d as u32
        );
        
        // Calculate bootstrap score
        let wins = (win_rate_7d * num_trades_7d as f64) as u32;
        let bootstrap_score = ((50 + wins * 2) as i32 + (total_pnl_sol / 5.0) as i32)
            .min(90)
            .max(0) as u8;
        
        let features = feature_cache::WalletFeatures {
            win_rate_7d,
            realized_pnl_7d: total_pnl_sol,
            trade_count: num_trades_7d as u32,
            avg_size: if num_trades_7d > 0 { 
                total_pnl_sol.abs() / num_trades_7d as f64 
            } else { 
                0.0 
            },
            tier,
            confidence: tier.confidence(),
            last_trade: None,
            last_update: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            bootstrap_score,
        };
        
        cache.insert(wallet, features);
        count += 1;
    }
    
    info!("👥 Wallet cache updated: {} entries", count);
    Ok(())
}

/// Print startup banner
fn print_banner(config: &Config) {
    println!("\n======================================================================");
    println!("🧠 BRAIN SERVICE - TRADING DECISION ENGINE");
    println!("======================================================================");
    println!("⏰ {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("✅ All systems operational");
    println!("🛡️  Max positions: {}", config.guardrails.max_concurrent_positions);
    println!("📊 Metrics: http://localhost:9090/metrics");
    println!("🔍 Status: LISTENING FOR ADVICE...");
    println!("======================================================================\n");
}
