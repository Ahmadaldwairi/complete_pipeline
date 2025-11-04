//! 📝 Async DB Writer - Decoupled from hot path
//!
//! Dedicated task for batched database writes to prevent blocking gRPC stream.
//! Receives write commands via channel and processes them in background.

use anyhow::Result;
use rusqlite::Connection;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};
use std::time::Instant;

use crate::types::{Token, Trade};

/// Maximum batch size before forcing a flush
const BATCH_MAX_SIZE: usize = 500;

/// Maximum time to hold items in batch before flushing (ms)
const BATCH_MAX_LATENCY_MS: u64 = 50;

/// DB write commands sent from hot path
#[derive(Debug, Clone)]
pub enum DbWriteCommand {
    InsertTrade(Trade),
    InsertToken(Token),
    UpdateInitialLiquidity { mint: String, liquidity_sol: f64 },
}

/// Async DB Writer - runs in separate task
pub struct DbWriter {
    conn: Connection,
    trade_batch: Vec<Trade>,
    token_batch: Vec<Token>,
    liquidity_updates: Vec<(String, f64)>,
    batch_start: Instant,
}

impl DbWriter {
    /// Create new DB writer (must be called from blocking context)
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            trade_batch: Vec::with_capacity(BATCH_MAX_SIZE),
            token_batch: Vec::with_capacity(100),
            liquidity_updates: Vec::with_capacity(100),
            batch_start: Instant::now(),
        }
    }

    /// Main writer loop - processes commands from channel
    /// Runs in BLOCKING mode (not async) to avoid blocking the tokio runtime
    pub fn run_blocking(mut self, mut rx: mpsc::UnboundedReceiver<DbWriteCommand>) {
        info!("📝 DB Writer task started (blocking thread)");
        
        use std::time::{Duration, Instant};
        let mut last_flush = Instant::now();
        let flush_interval = Duration::from_millis(BATCH_MAX_LATENCY_MS);

        loop {
            // Try to receive commands with timeout for periodic flush
            match rx.blocking_recv() {
                Some(cmd) => {
                    self.handle_command(cmd);
                    
                    // Drain additional messages if available (batch efficiently)
                    while let Ok(cmd) = rx.try_recv() {
                        self.handle_command(cmd);
                        
                        // Check if batch size threshold reached
                        if self.should_flush_size() {
                            break;
                        }
                    }
                    
                    // Flush if size threshold reached OR time elapsed
                    if self.should_flush_size() || last_flush.elapsed() >= flush_interval {
                        if let Err(e) = self.flush_all() {
                            warn!("❌ DB flush failed: {}", e);
                        }
                        last_flush = Instant::now();
                    }
                }
                None => {
                    warn!("📝 DB Writer channel closed, exiting");
                    break;
                }
            }
        }
    }

    /// Main writer loop with bounded channel (back-pressure support)
    pub fn run_blocking_bounded(mut self, mut rx: mpsc::Receiver<DbWriteCommand>) {
        info!("📝 DB Writer task started (blocking thread, bounded channel)");
        
        use std::time::{Duration, Instant};
        let mut last_flush = Instant::now();
        let flush_interval = Duration::from_millis(BATCH_MAX_LATENCY_MS);
        let mut dropped_count = 0u64;
        let mut last_drop_log = Instant::now();

        loop {
            // Try to receive commands with timeout for periodic flush
            match rx.blocking_recv() {
                Some(cmd) => {
                    self.handle_command(cmd);
                    
                    // Drain additional messages if available (batch efficiently)
                    while let Ok(cmd) = rx.try_recv() {
                        self.handle_command(cmd);
                        
                        // Check if batch size threshold reached
                        if self.should_flush_size() {
                            break;
                        }
                    }
                    
                    // Flush if size threshold reached OR time elapsed
                    if self.should_flush_size() || last_flush.elapsed() >= flush_interval {
                        if let Err(e) = self.flush_all() {
                            warn!("❌ DB flush failed: {}", e);
                        }
                        last_flush = Instant::now();
                    }
                }
                None => {
                    warn!("📝 DB Writer channel closed, exiting");
                    break;
                }
            }
        }
    }

    /// Handle a single write command
    fn handle_command(&mut self, cmd: DbWriteCommand) {
        match cmd {
            DbWriteCommand::InsertTrade(trade) => {
                self.trade_batch.push(trade);
            }
            DbWriteCommand::InsertToken(token) => {
                self.token_batch.push(token);
            }
            DbWriteCommand::UpdateInitialLiquidity { mint, liquidity_sol } => {
                self.liquidity_updates.push((mint, liquidity_sol));
            }
        }
    }

    /// Check if batch size threshold reached
    fn should_flush_size(&self) -> bool {
        self.trade_batch.len() >= BATCH_MAX_SIZE
            || self.token_batch.len() >= 100
            || self.liquidity_updates.len() >= 100
    }

    /// Check if time threshold reached
    fn should_flush_time(&self) -> bool {
        !self.trade_batch.is_empty() 
            || !self.token_batch.is_empty()
            || !self.liquidity_updates.is_empty()
    }

    /// Flush all batches to database in a SINGLE transaction
    fn flush_all(&mut self) -> Result<()> {
        let start = Instant::now();
        let total_items = self.trade_batch.len() + self.token_batch.len() + self.liquidity_updates.len();

        if total_items == 0 {
            return Ok(());
        }

        // Use a SINGLE transaction for all inserts (much faster!)
        let tx = self.conn.transaction()?;

        // CRITICAL ORDER: Flush tokens FIRST (trades have FK to tokens)
        if !self.token_batch.is_empty() {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT OR REPLACE INTO tokens (
                    mint, creator_wallet, bonding_curve_addr, name, symbol, uri, decimals,
                    launch_tx_sig, launch_slot, launch_block_time,
                    initial_price, initial_liquidity_sol, initial_supply, market_cap_init,
                    mint_authority, freeze_authority, metadata_update_auth,
                    migrated_to_raydium, migration_slot, migration_block_time, raydium_pool,
                    observed_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
                "#
            )?;

            for token in &self.token_batch {
                stmt.execute(rusqlite::params![
                    token.mint,
                    token.creator_wallet,
                    token.bonding_curve_addr,
                    token.name,
                    token.symbol,
                    token.uri,
                    token.decimals,
                    token.launch_tx_sig,
                    token.launch_slot,
                    token.launch_block_time,
                    token.initial_price,
                    token.initial_liquidity_sol,
                    token.initial_supply,
                    token.market_cap_init,
                    token.mint_authority,
                    token.freeze_authority,
                    token.metadata_update_auth,
                    token.migrated_to_raydium as i32,
                    token.migration_slot,
                    token.migration_block_time,
                    token.raydium_pool,
                    token.observed_at,
                ])?;
            }
            self.token_batch.clear();
        }

        // THEN flush trades (now tokens exist)
        if !self.trade_batch.is_empty() {
            let mut stmt = tx.prepare_cached(
                r#"
                INSERT OR REPLACE INTO trades (
                    sig, slot, block_time, mint, side, trader,
                    amount_tokens, amount_sol, price, is_amm
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#
            )?;

            for trade in &self.trade_batch {
                stmt.execute(rusqlite::params![
                    trade.sig,
                    trade.slot,
                    trade.block_time,
                    trade.mint,
                    trade.side.as_str(),
                    trade.trader,
                    trade.amount_tokens,
                    trade.amount_sol,
                    trade.price,
                    trade.is_amm as i32,
                ])?;
            }
            self.trade_batch.clear();
        }

        // Finally flush liquidity updates
        if !self.liquidity_updates.is_empty() {
            let mut stmt = tx.prepare_cached(
                r#"
                UPDATE tokens 
                SET initial_liquidity_sol = ?2 
                WHERE mint = ?1 AND initial_liquidity_sol IS NULL
                "#
            )?;

            for (mint, liquidity_sol) in &self.liquidity_updates {
                stmt.execute(rusqlite::params![mint, liquidity_sol])?;
            }
            self.liquidity_updates.clear();
        }

        // Commit everything at once
        tx.commit()?;

        let elapsed = start.elapsed();
        if total_items > 50 || elapsed.as_millis() > 10 {
            debug!("💾 DB flush: {} items in {:?}", total_items, elapsed);
        }
        
        self.batch_start = Instant::now();
        Ok(())
    }
}

/// Spawn DB writer task and return channel for sending commands
pub fn spawn_db_writer(conn: Connection) -> mpsc::Sender<DbWriteCommand> {
    // Use bounded channel with 50k capacity for back-pressure
    const CHANNEL_CAPACITY: usize = 50_000;
    let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);
    
    // Spawn writer in dedicated blocking thread (SQLite operations are blocking)
    std::thread::spawn(move || {
        let writer = DbWriter::new(conn);
        writer.run_blocking_bounded(rx);
    });
    
    info!("✅ DB Writer channel created (blocking thread, capacity={})", CHANNEL_CAPACITY);
    tx
}
