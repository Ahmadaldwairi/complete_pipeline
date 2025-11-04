// Data Mining - Unified Collector
// Single bot that handles BOTH launch tracking AND wallet tracking
// Processes all Pump.fun transactions in one stream

use anyhow::{Context, Result};
use data_mining::{config::Config, Database};
use data_mining::checkpoint::Checkpoint;
use data_mining::db::aggregator::WindowAggregator;
use data_mining::momentum_tracker::MomentumTracker;
use data_mining::parser::PumpParser;
use data_mining::parser::raydium::RaydiumParser;
use data_mining::types::{PumpEvent, Token, Trade, TradeSide};
use data_mining::udp::{AdvisorySender, BatchedBrainSignalSender};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_stream::StreamExt;
use tracing::{info, warn, error, debug};
use tracing_subscriber::{fmt, EnvFilter};

use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof,
    SubscribeRequest,
    SubscribeRequestFilterTransactions,
    SubscribeUpdateTransaction,
    CommitmentLevel,
};
use solana_sdk::pubkey::Pubkey;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    init_logging();

    info!("🚀 Data Mining Collector Starting...");
    info!("   ✅ Unified Launch + Wallet Tracking");
    info!("   ✅ Single gRPC Stream");
    info!("   ✅ Single SQLite Database");

    // Load configuration
    let config = Config::load_or_default()?;
    info!("⚙️  Configuration loaded");

    // Initialize unified database and async writer
    let db = Arc::new(Mutex::new(Database::new(&config.database.path, config.database.wal_mode)?));
    info!("✅ Database initialized: {}", config.database.path);
    
    // Create async DB writer (separate task, non-blocking)
    let db_writer_tx = {
        let db_clone = db.lock().unwrap();
        let conn = db_clone.get_connection_for_writer()?;
        data_mining::db::spawn_db_writer(conn)
    };
    info!("✅ Async DB Writer: Started (non-blocking mode)");

    // Create in-memory token cache to avoid DB reads in hot path
    let token_cache = Arc::new(Mutex::new(std::collections::HashSet::<String>::new()));
    info!("🗂️  Token cache initialized (in-memory HashSet)");

    // Initialize batched UDP sender (event-driven, adaptive flushing)
    let udp_batch_tx = data_mining::udp::spawn_batched_sender();
    info!("✅ Batched UDP Sender: Started (max_batch=256, max_latency=15ms)");

    // Load or create checkpoint
    let checkpoint_path = "data/checkpoint.json";
    let mut checkpoint = match Checkpoint::load(checkpoint_path)? {
        Some(cp) => {
            info!("✅ Loaded checkpoint: slot {}", cp.last_processed_slot);
            cp
        }
        None => {
            info!("📍 No checkpoint found, starting fresh");
            Checkpoint::new(0)
        }
    };

    // Load tracked wallets from config or database
    let tracked_wallets = load_tracked_wallets(&db).await?;
    info!("👥 Loaded {} tracked wallets", tracked_wallets.len());

    let pump_program = Pubkey::from_str(&config.programs.pump_program)
        .context("Invalid pump program ID")?;
    info!("🎯 Monitoring Pump.fun: {}", pump_program);

    // Initialize advisory sender (optional - gracefully handles if execution bot is offline)
    let advisory_sender = if config.advice_bus.enabled {
        match AdvisorySender::new(&config.advice_bus.host, config.advice_bus.port) {
            Ok(sender) => {
                info!("✅ Advisory Sender: Connected to {}:{}", 
                    config.advice_bus.host, config.advice_bus.port);
                Some(sender)
            }
            Err(e) => {
                warn!("⚠️  Advisory Sender: Failed to initialize: {}", e);
                warn!("   Continuing without advisory sending...");
                None
            }
        }
    } else {
        info!("ℹ️  Advisory Sender: DISABLED in config");
        None
    };

    // Initialize WindowAggregator for time-series window computation
    let window_aggregator = WindowAggregator::new(config.windows.intervals.clone());
    info!("📊 Window Aggregator: Intervals {:?}", config.windows.intervals);

    // Initialize BrainSignalSender for market intelligence (optional - gracefully handles if brain is offline)
    let brain_signal_sender = if config.advice_bus.enabled {
        let sender = data_mining::udp::BatchedBrainSignalSender::new(
            udp_batch_tx.clone(),
            &config.advice_bus.host,
            45100  // Brain listens on 45100 for all advice messages including WindowMetrics
        );
        info!("✅ Brain Signal Sender: Connected to {}:45100 (batched)", config.advice_bus.host);
        Some(sender)
    } else {
        info!("ℹ️  Brain Signal Sender: DISABLED in config");
        None
    };

    // Initialize MomentumTracker for real-time pattern detection
    // Parameters: momentum_threshold (3 buys in 500ms), spike_multiplier (5x volume), cooldown_ms (5000ms)
    let momentum_tracker = Arc::new(Mutex::new(
        MomentumTracker::new(3, 5.0, 5000)
    ));
    info!("📈 Momentum Tracker: Initialized (threshold=3 buys/500ms, spike=5x, cooldown=5s)");

    // Initialize WindowTracker for sliding window analytics
    // Parameters: send_interval_ms (500ms), min_activity_threshold (3 trades)
    let window_tracker = Arc::new(Mutex::new(
        data_mining::window_tracker::WindowTracker::new(500, 3)
    ));
    info!("📊 Window Tracker: Initialized (interval=500ms, min_activity=3 trades)");

    // � Initialize Latency Tracker for performance monitoring
    let latency_tracker = Arc::new(Mutex::new(data_mining::latency_tracker::LatencyTracker::new()));
    data_mining::latency_tracker::spawn_latency_reporter(latency_tracker.clone());

    // �🔮 Spawn Pyth SOL/USD Price Fetcher (HTTP API with SQLite logging)
    let _pyth_handle = data_mining::pyth_http::spawn_pyth_http(Some(db.clone()));
    info!("🔮 Pyth HTTP fetcher spawned - broadcasting to ports 45100 & 45110");

    // 🎯 Spawn Hotlist Scorer for 1M+ MC hunting
    let hotlist_config = data_mining::hotlist_scorer::HotlistScorerConfig::default();
    let _hotlist_handle = data_mining::hotlist_scorer::spawn_hotlist_scorer(
        db.clone(),
        advisory_sender.clone(),
        window_tracker.clone(),
        hotlist_config,
    );
    info!("🎯 Hotlist Scorer: Spawned (scoring every 5s, broadcasting score ≥6.0)");

    // Main processing loop with auto-reconnect
    loop {
        info!("🔌 Connecting to gRPC: {}", config.grpc.endpoint);
        
        match run_unified_collector(
            &mut checkpoint,
            checkpoint_path,
            &config.grpc.endpoint,
            &pump_program,
            db.clone(),
            &db_writer_tx,
            &udp_batch_tx,
            &tracked_wallets,
            advisory_sender.clone(),
            brain_signal_sender.clone(),
            momentum_tracker.clone(),
            window_tracker.clone(),
            &window_aggregator,
            latency_tracker.clone(),
            token_cache.clone(),
        )
        .await
        {
            Ok(_) => {
                warn!("Stream ended normally, reconnecting...");
            }
            Err(e) => {
                error!("Stream error: {}, reconnecting in 5s...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}
/// Get display name for wallet (alias if available, otherwise short address)
fn get_wallet_display_name(wallet: &str, tracked_wallets: &HashMap<String, Option<String>>) -> String {
    if let Some(Some(alias)) = tracked_wallets.get(wallet) {
        alias.clone()
    } else {
        format!("{}", &wallet[..8])
    }
}


/// Main unified collector - processes ALL pump.fun transactions
/// Handles both launch tracking AND wallet tracking in the same stream
async fn run_unified_collector(
    checkpoint: &mut Checkpoint,
    checkpoint_path: &str,
    endpoint: &str,
    pump_program: &Pubkey,
    db: Arc<Mutex<Database>>,
    db_writer_tx: &tokio::sync::mpsc::Sender<data_mining::db::DbWriteCommand>,
    udp_batch_tx: &tokio::sync::mpsc::UnboundedSender<data_mining::udp::UdpMessage>,
    tracked_wallets: &HashMap<String, Option<String>>,
    advisory_sender: Option<AdvisorySender>,
    brain_signal_sender: Option<BatchedBrainSignalSender>,
    momentum_tracker: Arc<Mutex<MomentumTracker>>,
    window_tracker: Arc<Mutex<data_mining::window_tracker::WindowTracker>>,
    window_aggregator: &WindowAggregator,
    latency_tracker: Arc<Mutex<data_mining::latency_tracker::LatencyTracker>>,
    token_cache: Arc<Mutex<std::collections::HashSet<String>>>,
) -> Result<()> {
    // Connect to Yellowstone gRPC
    let mut client = GeyserGrpcClient::build_from_shared(endpoint.to_string())?
        .x_token::<String>(None)?
        .connect()
        .await?;

    info!("✅ Connected to Yellowstone gRPC");

    // Subscribe to ALL Pump.fun transactions
    let mut transactions: HashMap<String, SubscribeRequestFilterTransactions> = HashMap::new();
    transactions.insert(
        "pump_transactions".to_string(),
        SubscribeRequestFilterTransactions {
            vote: Some(false),
            failed: Some(false),
            signature: None,
            account_include: vec![pump_program.to_string()],
            account_exclude: vec![],
            account_required: vec![],
        },
    );

    let request = SubscribeRequest {
        accounts: HashMap::new(),
        slots: HashMap::new(),
        transactions,
        transactions_status: HashMap::new(),
        blocks: HashMap::new(),
        blocks_meta: HashMap::new(),
        entry: HashMap::new(),
        commitment: Some(CommitmentLevel::Confirmed as i32),
        accounts_data_slice: vec![],
        ping: None,
        from_slot: None,
    };

    let mut stream = client.subscribe_once(request).await?;
    info!("📡 Subscribed to Pump.fun transaction stream");
    info!("👂 Processing all transactions for launch + wallet tracking...");

    let mut tx_count = 0u64;
    let mut launch_count = 0u64;
    let mut wallet_tx_count = 0u64;
    
    // Dedup cache: Track recently seen transaction signatures
    // Prevents double-processing on reorgs or confirmed/finalized duplicates
    let mut seen_signatures: HashSet<String> = HashSet::with_capacity(10000);
    const MAX_CACHE_SIZE: usize = 50000; // Keep last 50K signatures

    // Process all transactions
    loop {
        match stream.next().await {
            Some(Ok(msg)) => {
                if let Some(update) = msg.update_oneof {
                    match update {
                        UpdateOneof::Transaction(tx_update) => {
                            // Extract signature for dedup check
                            if let Some(transaction) = &tx_update.transaction {
                                if let Some(tx_data) = &transaction.transaction {
                                    let sig = bs58::encode(&tx_data.signatures[0]).into_string();
                                    
                                    // Skip if already processed (reorg/duplicate)
                                    if seen_signatures.contains(&sig) {
                                        debug!("⏭️  Skipping duplicate transaction: {}", &sig[..12]);
                                        continue;
                                    }
                                    
                                    // Add to seen cache
                                    seen_signatures.insert(sig.clone());
                                    
                                    // Prune cache if it gets too large
                                    if seen_signatures.len() > MAX_CACHE_SIZE {
                                        // Remove oldest 10K entries (simple approach: clear and rebuild)
                                        debug!("🗑️  Pruning signature cache ({} entries)", seen_signatures.len());
                                        seen_signatures.clear();
                                    }
                                }
                            }
                            
                            tx_count += 1;

                            // Update checkpoint
                            checkpoint.update(tx_update.slot);

                            // Save checkpoint periodically (every 1000 slots)
                            if let Err(e) = checkpoint.save_if_needed(checkpoint_path, tx_update.slot, 1000) {
                                warn!("Failed to save checkpoint: {}", e);
                            }

                            // Process transaction for BOTH systems
                            match process_transaction(
                                &tx_update,
                                &db,
                                &db_writer_tx,
                                &udp_batch_tx,
                                pump_program,
                                tracked_wallets,
                                &advisory_sender,
                                &brain_signal_sender,
                                &momentum_tracker,
                                &window_tracker,
                                &mut launch_count,
                                &mut wallet_tx_count,
                                window_aggregator,
                                &latency_tracker,
                                &token_cache,
                            )
                            .await
                            {
                                Ok(_) => {}
                                Err(e) => {
                                    warn!("Failed to process transaction: {}", e);
                                }
                            }

                            // Progress logging
                            if tx_count % 100 == 0 {
                                info!(
                                    "📊 Processed {} txs | {} launches | {} wallet txs",
                                    tx_count, launch_count, wallet_tx_count
                                );
                            }
                        }
                        _ => {
                            // Ignore other update types
                        }
                    }
                }
            }
            Some(Err(e)) => {
                error!("Stream error: {}", e);
                return Err(e.into());
            }
            None => {
                warn!("Stream ended");
                return Ok(());
            }
        }
    }
}

/// Process a single transaction for BOTH launch tracking AND wallet tracking
async fn process_transaction(
    tx: &SubscribeUpdateTransaction,
    db: &Arc<Mutex<Database>>,
    db_writer: &tokio::sync::mpsc::Sender<data_mining::db::DbWriteCommand>,
    udp_batch_tx: &tokio::sync::mpsc::UnboundedSender<data_mining::udp::UdpMessage>,
    pump_program: &Pubkey,
    tracked_wallets: &HashMap<String, Option<String>>,
    advisory_sender: &Option<AdvisorySender>,
    brain_signal_sender: &Option<BatchedBrainSignalSender>,
    momentum_tracker: &Arc<Mutex<MomentumTracker>>,
    window_tracker: &Arc<Mutex<data_mining::window_tracker::WindowTracker>>,
    launch_count: &mut u64,
    wallet_tx_count: &mut u64,
    window_aggregator: &WindowAggregator,
    latency_tracker: &Arc<Mutex<data_mining::latency_tracker::LatencyTracker>>,
    token_cache: &Arc<Mutex<std::collections::HashSet<String>>>,
) -> Result<()> {
    // 📊 TIMESTAMP 1: Transaction created (from gRPC)
    let created_ns = data_mining::latency_tracker::now_ns();
    
    // Extract transaction data
    let transaction = tx.transaction.as_ref().context("No transaction")?;
    let meta = transaction.meta.as_ref().context("No meta")?;
    let tx_data = transaction.transaction.as_ref().context("No tx data")?;
    let message = tx_data.message.as_ref().context("No message")?;

    // Extract account keys
    let mut account_keys = Vec::new();
    for key in &message.account_keys {
        if let Ok(pubkey) = Pubkey::try_from(key.as_slice()) {
            account_keys.push(pubkey.to_string());
        }
    }

    // Get fee payer (first account = actual trader)
    let fee_payer = account_keys.get(0).map(|s| s.as_str());

    // Parse Pump.fun events
    let parser = PumpParser::new(&pump_program.to_string())?;

    // Raydium CPMM program for graduated tokens
    let raydium_program = Pubkey::from_str("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C")?;
    let raydium_parser = RaydiumParser::new(&raydium_program.to_string())?;
    
    // Use current UTC timestamp as block_time
    // Note: Yellowstone gRPC transactions don't include block_time (only blocks do)
    // This is accurate to within ~400ms on a well-synced server
    let block_time = chrono::Utc::now().timestamp();
    
    let pump_events = parser.parse_transaction(transaction, tx.slot, block_time)?;

        // Also check for Raydium swaps (graduated tokens)
        let raydium_events = raydium_parser.parse_transaction(transaction, tx.slot, block_time)?;
        let mut all_events = pump_events;
        all_events.extend(raydium_events);
    
    if all_events.is_empty() {
        return Ok(()); // No pump.fun events
    }

    // Get balance changes for SOL amount calculations
    let pre_balances = &meta.pre_balances;
    let post_balances = &meta.post_balances;

    // Calculate SOL spent/received by fee payer
    let (sol_spent, sol_received) = if let Some(fp) = fee_payer {
        if let Some(idx) = account_keys.iter().position(|k| k == fp) {
            if idx < pre_balances.len() && idx < post_balances.len() {
                let pre = pre_balances[idx] as f64 / 1_000_000_000.0;
                let post = post_balances[idx] as f64 / 1_000_000_000.0;
                let change = post - pre;

                if change < 0.0 {
                    (Some(-change), None) // Spent SOL (BUY)
                } else if change > 0.0 {
                    (None, Some(change)) // Received SOL (SELL)
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Process each Pump.fun event
    for event in all_events {
        match event {
            PumpEvent::Launch { mint, creator, bonding_curve, name, symbol, uri, slot, block_time, signature } => {
                // 🚀 LAUNCH TRACKING: New token created
                *launch_count += 1;

                // Check in-memory cache first (avoid DB read)
                let token_exists = token_cache.lock().unwrap().contains(&mint);
                if !token_exists {
                    info!("🆕 NEW LAUNCH: {} by {}", &mint[..12], &creator[..8]);

                    // Add to cache immediately (avoid duplicate checks)
                    token_cache.lock().unwrap().insert(mint.clone());

                    // Insert token into database
                    let token = Token {
                        mint: mint.clone(),
                        creator_wallet: creator.clone(),
                        bonding_curve_addr: Some(bonding_curve.clone()),
                        name: if name.is_empty() { None } else { Some(name.clone()) },
                        symbol: if symbol.is_empty() { None } else { Some(symbol.clone()) },
                        uri: if uri.is_empty() { None } else { Some(uri.clone()) },
                        decimals: 9,
                        launch_tx_sig: signature.clone(),
                        launch_slot: slot,
                        launch_block_time: block_time,
                        initial_price: None,
                        initial_liquidity_sol: None,
                        initial_supply: None,
                        market_cap_init: None,
                        mint_authority: None,
                        freeze_authority: None,
                        metadata_update_auth: None,
                        migrated_to_raydium: false,
                        migration_slot: None,
                        migration_block_time: None,
                        raydium_pool: None,
                        observed_at: chrono::Utc::now().timestamp(),
                    };

                    // Send to async writer with back-pressure handling
                    if let Err(e) = db_writer.try_send(data_mining::db::DbWriteCommand::InsertToken(token)) {
                        match e {
                            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                                warn!("⚠️  DB writer channel full, dropping CREATE token (back-pressure)");
                            }
                            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                                warn!("❌ DB writer channel closed!");
                            }
                        }
                    } else {
                        debug!("✅ Queued CREATE token {}", &mint[..12]);
                    }
                }

                // 👤 WALLET TRACKING: Skip wallet stats in hot path (too slow)
                // TODO: Move to separate async task if needed
                info!("🆕 New wallet discovered (creator): {}", &creator[..8]);

                // Check if creator is tracked wallet
                if tracked_wallets.contains_key(&creator) {
                    let display_name = get_wallet_display_name(&creator, tracked_wallets);
                    info!("🔥 TRACKED WALLET CREATED TOKEN: {} by {}", &mint[..12], display_name);
                    
                    // Send CopyTrade advisory (99% confidence - creator knows what they're doing)
                    // side=0 (BUY), size=0.0 (unknown at launch), tier=3 (assume A-tier creator)
                    if let Some(sender) = advisory_sender {
                        if let Err(e) = sender.send_copy_trade(&mint, &creator, 0, 0.0, 3, 99) {
                            warn!("Failed to send CopyTrade advisory: {}", e);
                        }
                    }
                }
            }

            PumpEvent::Trade { signature, slot, block_time, mint, side, trader, amount_tokens, amount_sol, price, is_amm, virtual_sol_reserves, virtual_token_reserves } => {
                let is_buy = side == TradeSide::Buy;
                let side_str = if is_buy { "buy" } else { "sell" };

                // 📊 LAUNCH TRACKING: Record trade
                let trade = Trade {
                    sig: signature.clone(),
                    slot,
                    block_time,
                    mint: mint.clone(),
                    side: side.clone(),
                    trader: trader.clone(),
                    amount_tokens: amount_tokens as f64,
                    amount_sol: amount_sol as f64 / 1_000_000_000.0,
                    price,
                    is_amm,
                };

                // Ensure token exists before inserting trade (FK constraint)
                // Check in-memory cache instead of DB
                let token_exists = token_cache.lock().unwrap().contains(&mint);
                
                if !token_exists {
                    // Token doesn't exist - create placeholder and add to cache
                    token_cache.lock().unwrap().insert(mint.clone());
                    
                    let token = Token {
                        mint: mint.clone(),
                        creator_wallet: trader.clone(), // Use trader as fallback
                        bonding_curve_addr: None,
                        name: None,
                        symbol: None,
                        uri: None,
                        decimals: 6,
                        launch_tx_sig: signature.clone(),
                        launch_slot: slot,
                        launch_block_time: block_time,
                        initial_price: Some(price),
                        initial_liquidity_sol: None,
                        initial_supply: None,
                        market_cap_init: None,
                        mint_authority: None,
                        freeze_authority: None,
                        metadata_update_auth: None,
                        migrated_to_raydium: false,
                        migration_slot: None,
                        migration_block_time: None,
                        raydium_pool: None,
                        observed_at: chrono::Utc::now().timestamp(),
                    };
                    
                    // Send to async writer with back-pressure (placeholder token is low priority)
                    if let Err(e) = db_writer.try_send(data_mining::db::DbWriteCommand::InsertToken(token)) {
                        match e {
                            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                                debug!("⚠️  DB writer channel full, dropping placeholder token (back-pressure)");
                            }
                            _ => {}
                        }
                    } else {
                        debug!("✅ Queued placeholder token {}", &mint[..12]);
                    }
                }

                // Send trade to async writer (CRITICAL - never drop trades!)
                if let Err(e) = db_writer.try_send(data_mining::db::DbWriteCommand::InsertTrade(trade.clone())) {
                    match e {
                        tokio::sync::mpsc::error::TrySendError::Full(_) => {
                            warn!("⚠️  DB writer channel FULL! Dropping TRADE (back-pressure) - this should not happen");
                        }
                        _ => {}
                    }
                }
                
                // � TIMESTAMP 2: DB enqueued
                let enqueued_ns = data_mining::latency_tracker::now_ns();
                {
                    let mut tracker = latency_tracker.lock().unwrap();
                    tracker.db_enqueue.record(enqueued_ns - created_ns);
                }
                
                //  INITIAL LIQUIDITY TRACKING: Capture from first trade
                // Always send liquidity update if virtual_sol_reserves > 0 (DB writer will handle duplicates)
                if virtual_sol_reserves > 0 {
                    let initial_liq_sol = (virtual_sol_reserves as f64) / 1e9;
                    // Send to async writer (low priority - can drop on back-pressure)
                    if let Err(e) = db_writer.try_send(data_mining::db::DbWriteCommand::UpdateInitialLiquidity {
                        mint: mint.clone(),
                        liquidity_sol: initial_liq_sol,
                    }) {
                        match e {
                            tokio::sync::mpsc::error::TrySendError::Full(_) => {
                                debug!("⚠️  DB writer channel full, dropping liquidity update (back-pressure)");
                            }
                            _ => {}
                        }
                    } else {
                        debug!("💰 Queued initial liquidity update for {}: {:.4} SOL", &mint[..12], initial_liq_sol);
                    }
                }
                
                // 📈 MOMENTUM TRACKING: Record trade and check for signals
                {
                    let side_enum = if is_buy { TradeSide::Buy } else { TradeSide::Sell };
                    let mut tracker = momentum_tracker.lock().unwrap();
                    
                    // Record trade in rolling window
                    tracker.record_trade(&mint, side_enum, amount_sol as f64, &trader);
                    
                    // Check for momentum signal (≥3 buys in 500ms)
                    if let Some(signal) = tracker.check_momentum(&mint) {
                        if let Some(ref sender) = brain_signal_sender {
                            let _ = sender.send_momentum_detected(
                                &mint,
                                signal.buys_in_last_500ms,
                                signal.volume_sol as f32,
                                signal.unique_buyers,
                                signal.confidence,
                            );
                            debug!("📈 Momentum signal sent: {} ({} buys, {:.2} SOL, {} buyers, conf={})",
                                &mint[..12], signal.buys_in_last_500ms, signal.volume_sol,
                                signal.unique_buyers, signal.confidence);
                        }
                    }
                    
                    // Check for volume spike (current > 5x average)
                    if let Some(signal) = tracker.check_volume_spike(&mint) {
                        if let Some(ref sender) = brain_signal_sender {
                            let _ = sender.send_volume_spike(
                                &mint,
                                signal.total_sol,
                                signal.tx_count,
                                signal.time_window_ms,
                                signal.confidence,
                            );
                            debug!("🔥 Volume spike sent: {} ({:.2} SOL in {}ms, {} txs, conf={})",
                                &mint[..12], signal.total_sol, signal.time_window_ms,
                                signal.tx_count, signal.confidence);
                        }
                    }
                } // Drop momentum_tracker lock

                // 📊 WINDOW TRACKING: Record trade and check for metrics to send
                {
                    let current_time_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    
                    let is_alpha_wallet = tracked_wallets.contains_key(&trader);
                    
                    // Calculate market cap (approximate using 1B supply for now)
                    // TODO: Get actual supply from token metadata
                    let supply_estimate = 1_000_000_000.0; // 1 billion tokens
                    let mc_sol = price * supply_estimate;
                    
                    let mut tracker = window_tracker.lock().unwrap();
                    
                    // Update MC history for velocity tracking
                    tracker.update_mc(&mint, current_time_ms, mc_sol);
                    
                    tracker.add_trade(
                        &mint,
                        current_time_ms,
                        amount_sol as f64 / 1_000_000_000.0,
                        price,
                        &trader,
                        is_alpha_wallet,
                    );
                    
                    // Check if metrics should be sent (throttled to avoid spam)
                    if let Some(metrics) = tracker.get_metrics_if_ready(&mint, mc_sol) {
                        if let Some(ref sender) = brain_signal_sender {
                            let _ = sender.send_window_metrics(
                                &mint,
                                metrics.volume_sol_1s,
                                metrics.unique_buyers_1s,
                                metrics.price_change_bps_2s,
                                metrics.alpha_wallet_hits_10s,
                            );
                            
                            // Log MC velocity for high-velocity tokens
                            if metrics.mc_velocity_sol_per_min > 1000.0 {
                                info!("🚀 High MC velocity: {} | {:.0} SOL/min | MC: {:.0} SOL",
                                    &mint[..12],
                                    metrics.mc_velocity_sol_per_min,
                                    metrics.mc_sol);
                            }
                        }
                        
                        // Check for late opportunity using real-time metrics
                        // Estimate 60s metrics from 1s data (conservative)
                        let vol_60s_estimate = metrics.volume_sol_1s * 20.0; // Assume sustained
                        let buyers_60s_estimate = metrics.unique_buyers_1s as u32 * 10; // Conservative
                        
                        // Get token age from launch tracking
                        let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                        if let Ok(launch_ts) = db.lock().unwrap().get_token_launch_time(&mint) {
                            if let Some(launch_time) = launch_ts {
                                let age_seconds = (current_time - launch_time).max(0) as u64;
                                
                                // Late opportunity criteria:
                                // - Age: 20 min to 2 hours
                                // - Volume: >= 0.5 SOL/s sustained (10 SOL/60s estimate)
                                // - Buyers: >= 1 buyer/s sustained (10 buyers/60s estimate)
                                // - Recent activity: Metrics updated in last 2s
                                if age_seconds > 1200 && age_seconds < 7200 // 20 min to 2 hours
                                    && vol_60s_estimate >= 10.0
                                    && buyers_60s_estimate >= 10
                                {
                                    // Calculate late opportunity score
                                    let vol_score = (vol_60s_estimate / 35.0 * 50.0).clamp(0.0, 50.0);
                                    let buyer_score = ((buyers_60s_estimate as f64 / 40.0) * 30.0).clamp(0.0, 30.0);
                                    let age_factor = ((age_seconds as f64 / 3600.0) * 20.0).clamp(0.0, 20.0);
                                    let late_score = (vol_score + buyer_score + age_factor) as u8;
                                    
                                    let horizon_sec = 300; // 5 minute opportunity window
                                    
                                    if let Some(ref advisory) = advisory_sender {
                                        if let Err(e) = advisory.send_late_opportunity(&mint, horizon_sec, late_score) {
                                            warn!("Failed to send LateOpportunity for {}: {}", &mint[..12], e);
                                        } else {
                                            info!("🎯 Late opportunity detected: {} | age: {}s | vol: {:.1} SOL/60s | buyers: {} | score: {}",
                                                &mint[..12], age_seconds, vol_60s_estimate, buyers_60s_estimate, late_score);
                                        }
                                    }
                                }
                            }
                        }
                    }
                } // Drop window_tracker lock

                // Compute and update windows for this token after trade is recorded
                // DISABLED: Causes mutex contention in hot path - TODO: move to async task
                /*
                let window_update_result = {
                    let mut db_guard = db.lock().unwrap();
                    db_guard.update_windows_for_mint(
                        window_aggregator,
                        &mint,
                        block_time,
                        slot,
                    )
                }; // Lock released here
                
                if let Err(e) = window_update_result {
                    warn!("Failed to update windows for {}: {}", &mint[..12], e);
                } else {
                    // ✨ DISABLED: Check if updated windows meet any trigger thresholds
                    // This function causes mutex contention - TODO: move to async task
                    // check_and_send_opportunities(
                    //     &mint,
                    //     &db,
                    //     &advisory_sender,
                    //     block_time,
                    // );
                }
                */

                // 👤 WALLET TRACKING: Update trader stats
                let sol_amount = if is_buy {
                    sol_spent.or(Some(amount_sol as f64 / 1_000_000_000.0))
                } else {
                    sol_received.or(Some(amount_sol as f64 / 1_000_000_000.0))
                };

                let action = if is_buy { "BUY" } else { "SELL" };
                // 👤 WALLET TRACKING: Skip wallet stats in hot path (causes mutex contention)
                // TODO: Move to separate async task if needed
                debug!("📊 Trade by {}: {} {} @ {:.8}", &trader[..8], action, &mint[..12], price);
                
                // Check if tracked wallet (still useful for advisory signals)
                let is_new_wallet = false; // Skip DB check
                info!("🆕 New wallet discovered: {}", &trader[..8]);

                // Check if tracked wallet
                        if tracked_wallets.contains_key(&trader) {
                            *wallet_tx_count += 1;

                            let display_name = get_wallet_display_name(&trader, tracked_wallets);
                            
                            // 👤 WALLET ACTIVITY SIGNAL: Send to brain for strategic decisions
                            if let Some(ref sender) = brain_signal_sender {
                                let action = if is_buy { 0u8 } else { 1u8 }; // 0=BUY, 1=SELL
                                let size = sol_amount.unwrap_or(0.0) as f32;
                                let wallet_tier = 2u8; // Assume B-tier for tracked wallets (2)
                                let confidence = if is_buy { 85u8 } else { 90u8 }; // Higher confidence for sells
                                
                                let _ = sender.send_wallet_activity(
                                    &mint,
                                    &trader,
                                    action,
                                    size,
                                    wallet_tier,
                                    confidence,
                                );
                                debug!("👤 Wallet activity signal sent: {} {} {} ({:.2} SOL, tier={}, conf={})",
                                    display_name, if is_buy { "buys" } else { "sells"}, 
                                    &mint[..12], size, wallet_tier, confidence);
                            }
                            
                            if is_buy {
                                info!("🟢 TRACKED WALLET BUY: {} buys {} for {:.4} SOL", 
                                    display_name, &mint[..12], sol_amount.unwrap_or(0.0));
                                
                                // Send CopyTrade advisory (85% confidence for buys)
                                // side=0 (BUY), use actual trade size, tier=2 (assume B-tier)
                                if let Some(sender) = advisory_sender {
                                    let size = sol_amount.unwrap_or(0.0);
                                    if let Err(e) = sender.send_copy_trade(&mint, &trader, 0, size as f32, 2, 85) {
                                        warn!("Failed to send CopyTrade advisory: {}", e);
                                    }
                                }
                                
                                // If we already have a position, also send ExtendHold
                                // (suggests holding longer than normal exit strategy)
                                // TODO: Check if we have an open position in this mint
                            } else {
                                info!("🔴 TRACKED WALLET SELL: {} sells {} for {:.4} SOL", 
                                    display_name, &mint[..12], sol_amount.unwrap_or(0.0));
                                
                                // Send WidenExit advisory (urgent sell signal)
                                // slip_bps: 500 = 5% slippage tolerance
                                // ttl_ms: 5000 = 5 second urgency
                                if let Some(sender) = advisory_sender {
                                    if let Err(e) = sender.send_widen_exit(&mint, 500, 5000, 90) {
                                        warn!("Failed to send WidenExit advisory: {}", e);
                                    }
                                }
                            }
                        }
            }

            PumpEvent::Migrated { mint, pool, slot, block_time, signature } => {
                // Handle migration to Raydium
                info!("🚀 MIGRATION: {} → Raydium pool {}", &mint[..12], &pool[..12]);
                // TODO: Update token record with migration info
            }
        }
    }

    // 📊 TIMESTAMP 3: End-to-end processing complete
    let done_ns = data_mining::latency_tracker::now_ns();
    {
        let mut tracker = latency_tracker.lock().unwrap();
        tracker.end_to_end.record(done_ns - created_ns);
    }

    Ok(())
}

/// Check if windows meet trigger thresholds and send UDP advice
fn check_and_send_opportunities(
    mint: &str,
    db: &Arc<Mutex<Database>>,
    advisory_sender: &Option<AdvisorySender>,
    current_time: i64,
) {
    let Some(sender) = advisory_sender else { return };
    
    let db_guard = db.lock().unwrap();
    
    // Query recent windows for this mint
    let Ok(windows) = db_guard.get_recent_windows(mint, current_time - 120) else { return };
    let launch_time = db_guard.get_token_launch_time(mint).ok().flatten();
    
    drop(db_guard);
    
    // Skip if data is too old (stale) - windows older than 2 minutes
    let has_recent_data = windows.iter().any(|(_, _, _, start_time, _)| {
        (current_time - start_time) < 120
    });
    
    if !has_recent_data {
        return; // Don't send opportunities for stale/inactive tokens
    }
    
    // Find relevant window sizes
    let w2s = windows.iter().find(|(sec, _, _, _, _)| *sec == 2);
    let w5s = windows.iter().find(|(sec, _, _, _, _)| *sec == 5);
    let w60s = windows.iter().find(|(sec, _, _, _, _)| *sec == 60);
    
    // Path B: Momentum (high 5s volume + 2s buyers)
    if let (Some((_, vol_5s, _, _, _)), Some((_, _, buyers_2s, _, _))) = (w5s, w2s) {
        // Thresholds: 2 SOL/5s, 2 buyers/2s (testing mode)
        if *vol_5s >= 2.0 && *buyers_2s >= 2 {
            // Calculate momentum score based on activity intensity
            let vol_score = (vol_5s / 8.0 * 50.0).clamp(0.0, 50.0);
            let buyer_score = ((*buyers_2s as f64 / 5.0) * 50.0).clamp(0.0, 50.0);
            let momentum_score = (vol_score + buyer_score) as u8;
            
            if let Err(e) = sender.send_momentum_opportunity(mint, *vol_5s, *buyers_2s, momentum_score) {
                warn!("Failed to send MomentumOpportunity for {}: {}", &mint[..12], e);
            }
        }
    }
    
    // Path D: Late Opportunity (mature token, high 60s volume)
    if let Some((_, vol_60s, buyers_60s, start_time, _price)) = w60s {
        if let Some(launch_ts) = launch_time {
            let age_seconds = current_time - launch_ts;
            
            // Thresholds: >20 min old, 10 SOL/60s, 10 buyers (testing mode)
            // Also check: not TOO old (max 2 hours), and has recent activity
            if age_seconds > 1200 && age_seconds < 7200 // 20 min to 2 hours
                && *vol_60s >= 10.0 && *buyers_60s >= 10
                && (current_time - start_time) < 120 // Window is recent
            {
                // Calculate late opportunity score
                let vol_score = (vol_60s / 35.0 * 50.0).clamp(0.0, 50.0);
                let buyer_score = ((*buyers_60s as f64 / 40.0) * 30.0).clamp(0.0, 30.0);
                let age_factor = ((age_seconds as f64 / 3600.0) * 20.0).clamp(0.0, 20.0);
                let late_score = (vol_score + buyer_score + age_factor) as u8;
                
                let horizon_sec = 300; // 5 minute opportunity window
                
                if let Err(e) = sender.send_late_opportunity(mint, horizon_sec, late_score) {
                    warn!("Failed to send LateOpportunity for {}: {}", &mint[..12], e);
                }
            }
        }
    }
    
    // Path A: Rank-based (new launch with strong initial metrics)
    // This checks tokens that just launched (< 5 minutes old) and ranks them
    if let Some(launch_ts) = launch_time {
        let age_seconds = current_time - launch_ts;
        
        // Only consider very new tokens (< 5 minutes since launch)
        if age_seconds < 300 {
            // Calculate rank based on initial metrics
            // Lower rank = better opportunity (1 is best)
            
            let mut rank_score: f64 = 100.0;
            
            // Factor 1: Early volume (2s window)
            if let Some((_, vol_2s, buyers_2s, _, _)) = w2s {
                if *vol_2s >= 1.0 {
                    rank_score -= (*vol_2s / 3.0 * 20.0).clamp(0.0, 20.0);
                }
                if *buyers_2s >= 2 {
                    rank_score -= ((*buyers_2s as f64 / 5.0) * 15.0).clamp(0.0, 15.0);
                }
            }
            
            // Factor 2: Sustained 5s activity
            if let Some((_, vol_5s, buyers_5s, _, _)) = w5s {
                if *vol_5s >= 2.0 {
                    rank_score -= (*vol_5s / 8.0 * 25.0).clamp(0.0, 25.0);
                }
                if *buyers_5s >= 3 {
                    rank_score -= ((*buyers_5s as f64 / 8.0) * 15.0).clamp(0.0, 15.0);
                }
            }
            
            // Factor 3: Early momentum building
            if let Some((_, vol_60s, _buyers_60s, _, _)) = w60s {
                if *vol_60s >= 5.0 {
                    rank_score -= (*vol_60s / 15.0 * 25.0).clamp(0.0, 25.0);
                }
            }
            
            // Convert to rank (1-100, lower is better)
            // Score 0-50 = Rank 1-10 (excellent)
            // Score 50-70 = Rank 11-30 (good)
            // Score 70-100 = Rank 31-100 (moderate)
            let rank = if rank_score < 50.0 {
                1 + ((rank_score / 50.0) * 9.0) as u8
            } else if rank_score < 70.0 {
                11 + (((rank_score - 50.0) / 20.0) * 19.0) as u8
            } else {
                31 + (((rank_score - 70.0) / 30.0) * 69.0) as u8
            }.clamp(1, 100);
            
            // Only send if rank is decent (top 30)
            if rank <= 30 {
                // Calculate follow-through score (how likely it is to maintain momentum)
                let followthrough = (100.0 - rank_score).clamp(0.0, 100.0) as u8;
                
                if let Err(e) = sender.send_rank_opportunity(mint, rank, followthrough) {
                    warn!("Failed to send RankOpportunity for {}: {}", &mint[..12], e);
                }
            }
        }
    }
}

/// Load tracked wallets from database
async fn load_tracked_wallets(db: &Arc<Mutex<Database>>) -> Result<HashMap<String, Option<String>>> {
    let db_lock = db.lock().unwrap();
    db_lock.get_tracked_wallets()
}


fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();
}
