// Data Mining - Unified Collector
// Single bot that handles BOTH launch tracking AND wallet tracking
// Processes all Pump.fun transactions in one stream

use anyhow::{Context, Result};
use data_mining::{config::Config, Database};
use data_mining::checkpoint::Checkpoint;
use data_mining::db::aggregator::WindowAggregator;
use data_mining::parser::PumpParser;
use data_mining::parser::raydium::RaydiumParser;
use data_mining::types::{PumpEvent, Token, Trade, TradeSide};
use data_mining::udp::AdvisorySender;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::str::FromStr;
use tokio_stream::StreamExt;
use tracing::{info, warn, error};
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

    // Initialize unified database
    let db = Arc::new(Mutex::new(Database::new(&config.database.path, config.database.wal_mode)?));
    info!("✅ Database initialized: {}", config.database.path);

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

    // Main processing loop with auto-reconnect
    loop {
        info!("🔌 Connecting to gRPC: {}", config.grpc.endpoint);
        
        match run_unified_collector(
            &mut checkpoint,
            checkpoint_path,
            &config.grpc.endpoint,
            &pump_program,
            db.clone(),
            &tracked_wallets,
            advisory_sender.clone(),
            &window_aggregator,
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
    tracked_wallets: &HashMap<String, Option<String>>,
    advisory_sender: Option<AdvisorySender>,
    window_aggregator: &WindowAggregator,
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

    // Process all transactions
    loop {
        match stream.next().await {
            Some(Ok(msg)) => {
                if let Some(update) = msg.update_oneof {
                    match update {
                        UpdateOneof::Transaction(tx_update) => {
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
                                pump_program,
                                tracked_wallets,
                                &advisory_sender,
                                &mut launch_count,
                                &mut wallet_tx_count,
                                window_aggregator,
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
    pump_program: &Pubkey,
    tracked_wallets: &HashMap<String, Option<String>>,
    advisory_sender: &Option<AdvisorySender>,
    launch_count: &mut u64,
    wallet_tx_count: &mut u64,
    window_aggregator: &WindowAggregator,
) -> Result<()> {
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
    
    // Use current time as block_time (like launch_tracker does)
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

                // Check if token already exists (avoid duplicate inserts)
                let token_exists = db.lock().unwrap().token_exists(&mint)?;
                if !token_exists {
                    info!("🆕 NEW LAUNCH: {} by {}", &mint[..12], &creator[..8]);

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

                    if let Err(e) = db.lock().unwrap().insert_token(&token) {
                        warn!("Failed to insert token {}: {}", &mint[..12], e);
                    }
                }

                // 👤 WALLET TRACKING: Update creator stats
                match db.lock().unwrap().update_wallet_stats(&creator, "CREATE", None, Some(&mint)) {
                    Ok(is_new) => {
                        if is_new {
                            info!("🆕 New wallet discovered: {}", &creator[..8]);
                        }

                        // Check if creator is tracked wallet
                        if tracked_wallets.contains_key(&creator) {
                            let display_name = get_wallet_display_name(&creator, tracked_wallets);
                            info!("🔥 TRACKED WALLET CREATED TOKEN: {} by {}", &mint[..12], display_name);
                            
                            // Send CopyTrade advisory (99% confidence - creator knows what they're doing)
                            if let Some(sender) = advisory_sender {
                                if let Err(e) = sender.send_copy_trade(&mint, &creator, 99) {
                                    warn!("Failed to send CopyTrade advisory: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to update wallet stats for {}: {}", &creator[..8], e);
                    }
                }
            }

            PumpEvent::Trade { signature, slot, block_time, mint, side, trader, amount_tokens, amount_sol, price, is_amm } => {
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

                // Try to insert trade - silently ignore if token doesn't exist yet (foreign key constraint)
                // This is expected behavior when we see trades before CREATE events
                let _ = db.lock().unwrap().insert_trade(&trade);

                // Compute and update windows for this token after trade is recorded
                if let Err(e) = db.lock().unwrap().update_windows_for_mint(
                    window_aggregator,
                    &mint,
                    block_time,
                    slot,
                ) {
                    warn!("Failed to update windows for {}: {}", &mint[..12], e);
                }

                // 👤 WALLET TRACKING: Update trader stats
                let sol_amount = if is_buy {
                    sol_spent.or(Some(amount_sol as f64 / 1_000_000_000.0))
                } else {
                    sol_received.or(Some(amount_sol as f64 / 1_000_000_000.0))
                };

                let action = if is_buy { "BUY" } else { "SELL" };
                match db.lock().unwrap().update_wallet_stats(&trader, action, sol_amount, Some(&mint)) {
                    Ok(is_new) => {
                        if is_new {
                            info!("🆕 New wallet discovered: {}", &trader[..8]);
                        }

                        // Check if tracked wallet
                        if tracked_wallets.contains_key(&trader) {
                            *wallet_tx_count += 1;

                                let display_name = get_wallet_display_name(&trader, tracked_wallets);
                            if is_buy {
                                info!("🟢 TRACKED WALLET BUY: {} buys {} for {:.4} SOL", 
                                    display_name, &mint[..12], sol_amount.unwrap_or(0.0));
                                
                                // Send CopyTrade advisory (85% confidence for buys)
                                if let Some(sender) = advisory_sender {
                                    if let Err(e) = sender.send_copy_trade(&mint, &trader, 85) {
                                        warn!("Failed to send CopyTrade advisory: {}", e);
                                    }
                                }
                                
                                // If we already have a position, also send ExtendHold
                                // (suggests holding longer than normal exit strategy)
                                // TODO: Check if we have an open position in this mint
                            } else {
                                let display_name = get_wallet_display_name(&trader, tracked_wallets);
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
                    Err(e) => {
                        warn!("Failed to update wallet stats for {}: {}", &trader[..8], e);
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

    Ok(())
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
