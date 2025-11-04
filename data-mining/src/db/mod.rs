pub mod checkpoint;
pub mod aggregator;
pub mod writer;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, info};

use crate::types::{Token, Trade, TradeSide, Window};

pub use checkpoint::Checkpoint;
pub use aggregator::WindowAggregator;
pub use writer::{DbWriter, DbWriteCommand, spawn_db_writer};

pub struct Database {
    conn: Connection,
    db_path: String,
    trade_buffer: Vec<Trade>,
    buffer_last_flush: Instant,
    buffer_size_limit: usize,
    buffer_time_limit_ms: u128,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P, wal_mode: bool) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create database directory")?;
        }

        let conn = Connection::open(&path)
            .context("Failed to open database connection")?;

        if wal_mode {
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;"
            ).context("Failed to enable WAL mode")?;
        }

        let mut db = Self { 
            conn,
            db_path: path.as_ref().to_string_lossy().to_string(),
            trade_buffer: Vec::with_capacity(100),
            buffer_last_flush: Instant::now(),
            buffer_size_limit: 50,     // Flush every 50 trades - balanced performance
            buffer_time_limit_ms: 100,  // Flush every 100ms - safe and efficient
        };
        db.initialize_schema()?;
        
        info!("✅ Database initialized successfully");
        Ok(db)
    }
    
    /// Create a new connection for the async DB writer
    /// This allows writes to happen in parallel without blocking reads
    pub fn get_connection_for_writer(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)
            .context("Failed to open writer connection")?;
        
        // Enable WAL mode for writer connection
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA temp_store=MEMORY;
             PRAGMA mmap_size=268435456;
             PRAGMA cache_size=-65536;"
        ).context("Failed to configure writer connection")?;
        
        Ok(conn)
    }

    fn initialize_schema(&mut self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            -- Tokens table
            CREATE TABLE IF NOT EXISTS tokens (
                mint TEXT PRIMARY KEY,
                creator_wallet TEXT NOT NULL,
                bonding_curve_addr TEXT,
                name TEXT,
                symbol TEXT,
                uri TEXT,
                decimals INTEGER NOT NULL,
                launch_tx_sig TEXT NOT NULL,
                launch_slot INTEGER NOT NULL,
                launch_block_time INTEGER NOT NULL,
                initial_price REAL,
                initial_liquidity_sol REAL,
                initial_supply TEXT,
                market_cap_init REAL,
                mint_authority TEXT,
                freeze_authority TEXT,
                metadata_update_auth TEXT,
                migrated_to_raydium INTEGER DEFAULT 0,
                migration_slot INTEGER,
                migration_block_time INTEGER,
                raydium_pool TEXT,
                observed_at INTEGER NOT NULL
            );

            -- Trades table
            CREATE TABLE IF NOT EXISTS trades (
                sig TEXT PRIMARY KEY,
                slot INTEGER NOT NULL,
                block_time INTEGER NOT NULL,
                mint TEXT NOT NULL,
                side TEXT CHECK(side IN ('buy', 'sell')) NOT NULL,
                trader TEXT NOT NULL,
                amount_tokens REAL NOT NULL,
                amount_sol REAL NOT NULL,
                price REAL NOT NULL,
                is_amm INTEGER DEFAULT 0,
                processed_at INTEGER DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY(mint) REFERENCES tokens(mint)
            );

            -- Windows table (aggregated candles)
            CREATE TABLE IF NOT EXISTS windows (
                mint TEXT NOT NULL,
                window_sec INTEGER NOT NULL,
                start_slot INTEGER NOT NULL,
                start_time INTEGER NOT NULL,
                end_time INTEGER NOT NULL,
                num_buys INTEGER DEFAULT 0,
                num_sells INTEGER DEFAULT 0,
                uniq_buyers INTEGER DEFAULT 0,
                vol_tokens REAL DEFAULT 0.0,
                vol_sol REAL DEFAULT 0.0,
                high REAL DEFAULT 0.0,
                low REAL DEFAULT 0.0,
                close REAL DEFAULT 0.0,
                vwap REAL DEFAULT 0.0,
                top1_share REAL DEFAULT 0.0,
                top3_share REAL DEFAULT 0.0,
                top5_share REAL DEFAULT 0.0,
                price_volatility REAL DEFAULT 0.0,
                open REAL DEFAULT 0.0,
                processed_at INTEGER DEFAULT (strftime('%s', 'now')),
                PRIMARY KEY(mint, window_sec, start_time)
            );

            -- Pyth price history table (for analytics)
            CREATE TABLE IF NOT EXISTS pyth_prices (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                price REAL NOT NULL,
                confidence REAL NOT NULL,
                confidence_ratio REAL NOT NULL,
                source TEXT NOT NULL,
                created_at INTEGER DEFAULT (strftime('%s', 'now'))
            );

            -- Wallet statistics for discovery
            CREATE TABLE IF NOT EXISTS wallet_stats (
                wallet TEXT PRIMARY KEY,
                alias TEXT,
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                total_trades INTEGER DEFAULT 0,
                buy_count INTEGER DEFAULT 0,
                sell_count INTEGER DEFAULT 0,
                create_count INTEGER DEFAULT 0,
                total_sol_in REAL DEFAULT 0.0,
                total_sol_out REAL DEFAULT 0.0,
                net_pnl_sol REAL DEFAULT 0.0,
                realized_wins INTEGER DEFAULT 0,
                realized_losses INTEGER DEFAULT 0,
                win_rate REAL DEFAULT 0.0,
                profit_score REAL DEFAULT 0.0,
                avg_entry_price REAL DEFAULT 0.0,
                avg_exit_price REAL DEFAULT 0.0,
                is_tracked INTEGER DEFAULT 0
            );

            -- Position tracking for wallet discovery
            CREATE TABLE IF NOT EXISTS positions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                wallet TEXT NOT NULL,
                mint TEXT NOT NULL,
                bought_at INTEGER NOT NULL,
                sold_at INTEGER,
                token_amount REAL DEFAULT 0.0,
                sol_spent REAL DEFAULT 0.0,
                sol_received REAL DEFAULT 0.0,
                avg_buy_price REAL DEFAULT 0.0,
                avg_sell_price REAL DEFAULT 0.0,
                realized_pnl REAL DEFAULT 0.0,
                is_closed INTEGER DEFAULT 0,
                FOREIGN KEY(wallet) REFERENCES wallet_stats(wallet),
                FOREIGN KEY(mint) REFERENCES tokens(mint)
            );

            -- Indexes for performance (comprehensive coverage)
            CREATE INDEX IF NOT EXISTS idx_trades_mint_time ON trades(mint, block_time, slot);
            CREATE INDEX IF NOT EXISTS idx_trades_trader_time ON trades(trader, block_time, slot);
            CREATE INDEX IF NOT EXISTS idx_trades_slot ON trades(slot);
            CREATE INDEX IF NOT EXISTS idx_tokens_launch_time ON tokens(launch_block_time);
            CREATE INDEX IF NOT EXISTS idx_windows_mint_start ON windows(mint, start_time);
            CREATE INDEX IF NOT EXISTS idx_pyth_prices_timestamp ON pyth_prices(timestamp);
            CREATE INDEX IF NOT EXISTS idx_wallet_stats_profit ON wallet_stats(profit_score DESC);
            CREATE INDEX IF NOT EXISTS idx_wallet_stats_tracked ON wallet_stats(is_tracked);
            CREATE INDEX IF NOT EXISTS idx_positions_wallet_mint ON positions(wallet, mint, is_closed);
            
            -- Hotlist table for real-time explosive token scoring
            CREATE TABLE IF NOT EXISTS hotlist (
                mint TEXT PRIMARY KEY,
                score REAL NOT NULL,
                creator_score REAL DEFAULT 0.0,
                buyer_speed_score REAL DEFAULT 0.0,
                liquidity_score REAL DEFAULT 0.0,
                wallet_overlap_score REAL DEFAULT 0.0,
                concentration_score REAL DEFAULT 0.0,
                volume_accel_score REAL DEFAULT 0.0,
                mc_velocity_score REAL DEFAULT 0.0,
                mc_velocity REAL DEFAULT 0.0,
                unique_buyers_10s INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY(mint) REFERENCES tokens(mint)
            );
            
            CREATE INDEX IF NOT EXISTS idx_hotlist_score ON hotlist(score DESC);
            CREATE INDEX IF NOT EXISTS idx_hotlist_created ON hotlist(created_at);
            CREATE INDEX IF NOT EXISTS idx_hotlist_mc_velocity ON hotlist(mc_velocity DESC);
            "#
        ).context("Failed to initialize database schema")?;

        info!("📊 Database schema initialized");
        Ok(())
    }

    pub fn insert_token(&mut self, token: &Token) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO tokens (
                mint, creator_wallet, bonding_curve_addr, name, symbol, uri, decimals,
                launch_tx_sig, launch_slot, launch_block_time,
                initial_price, initial_liquidity_sol, initial_supply, market_cap_init,
                mint_authority, freeze_authority, metadata_update_auth,
                migrated_to_raydium, migration_slot, migration_block_time, raydium_pool,
                observed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
            "#,
            params![
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
            ],
        ).context("Failed to insert token")?;

        debug!("Inserted token: {}", token.mint);
        Ok(())
    }

    pub fn token_exists(&self, mint: &str) -> Result<bool> {
        let mut stmt = self.conn.prepare("SELECT 1 FROM tokens WHERE mint = ?1 LIMIT 1")?;
        let exists = stmt.exists(params![mint])?;
        Ok(exists)
    }

    /// Get token by mint address
    pub fn get_token(&self, mint: &str) -> Result<Option<Token>> {
        let mut stmt = self.conn.prepare(
            "SELECT mint, creator_wallet, initial_liquidity_sol FROM tokens WHERE mint = ?1 LIMIT 1"
        )?;
        
        let result = stmt.query_row(params![mint], |row| {
            Ok(Token {
                mint: row.get(0)?,
                creator_wallet: row.get(1)?,
                bonding_curve_addr: None,
                name: None,
                symbol: None,
                uri: None,
                decimals: 6,
                launch_tx_sig: String::new(),
                launch_slot: 0,
                launch_block_time: 0,
                initial_price: None,
                initial_liquidity_sol: row.get(2)?,
                initial_supply: None,
                market_cap_init: None,
                mint_authority: None,
                freeze_authority: None,
                metadata_update_auth: None,
                migrated_to_raydium: false,
                migration_slot: None,
                migration_block_time: None,
                raydium_pool: None,
                observed_at: 0,
            })
        });
        
        match result {
            Ok(token) => Ok(Some(token)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Insert trade with buffering for better performance
    /// Trades are buffered and flushed in batch when:
    /// - Buffer reaches 50 trades, OR
    /// - 100ms has passed since last flush
    /// 
    /// This provides optimal balance:
    /// - Responsive enough for Brain cache updates (well under 10s staleness threshold)
    /// - Efficient batching to minimize CPU and SQLite overhead
    /// - Prevents excessive flush frequency that causes lag
    pub fn insert_trade(&mut self, trade: &Trade) -> Result<()> {
        self.trade_buffer.push(trade.clone());
        
        let should_flush = self.trade_buffer.len() >= self.buffer_size_limit
            || self.buffer_last_flush.elapsed().as_millis() >= self.buffer_time_limit_ms;
        
        if should_flush {
            self.flush_trade_buffer()?;
        }
        
        Ok(())
    }
    
    /// Force flush the trade buffer (call this periodically or on shutdown)
    pub fn flush_trade_buffer(&mut self) -> Result<()> {
        if self.trade_buffer.is_empty() {
            return Ok(());
        }
        
        let count = self.trade_buffer.len();
        let start = Instant::now();
        
        // Begin transaction for batch insert
        let tx = self.conn.transaction()?;
        
        {
            let mut stmt = tx.prepare(
                r#"
                INSERT OR IGNORE INTO trades (
                    sig, slot, block_time, mint, side, trader,
                    amount_tokens, amount_sol, price, is_amm
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                "#,
            )?;
            
            for trade in &self.trade_buffer {
                stmt.execute(params![
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
        }
        
        tx.commit()?;
        
        let elapsed = start.elapsed();
        debug!(
            "📦 Flushed {} trades in {:?} ({:.1} trades/ms)",
            count,
            elapsed,
            count as f64 / elapsed.as_millis().max(1) as f64
        );
        
        self.trade_buffer.clear();
        self.buffer_last_flush = Instant::now();
        
        Ok(())
    }

    /// Old non-buffered insert method (deprecated, kept for compatibility)
    #[allow(dead_code)]
    fn insert_trade_direct(&mut self, trade: &Trade) -> Result<()> {
        let result = self.conn.execute(
            r#"
            INSERT OR IGNORE INTO trades (
                sig, slot, block_time, mint, side, trader,
                amount_tokens, amount_sol, price, is_amm
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
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
            ],
        );

        match result {
            Ok(rows) => {
                if rows == 0 {
                    debug!("Trade {} already exists (INSERT OR IGNORE)", trade.sig);
                } else {
                    debug!("Inserted trade: {} for mint {}", trade.sig, trade.mint);
                }
                Ok(())
            }
            Err(e) => {
                Err(anyhow::anyhow!("SQL error inserting trade {}: {}", trade.sig, e))
            }
        }
    }

    /// Get recent windows for a mint to check trigger thresholds
    pub fn get_recent_windows(&self, mint: &str, time_cutoff: i64) -> Result<Vec<(u32, f64, u32, i64, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT window_sec, vol_sol, uniq_buyers, start_time, close
             FROM windows 
             WHERE mint = ?1 AND end_time > ?2
             ORDER BY window_sec ASC"
        )?;
        
        let windows = stmt.query_map(params![mint, time_cutoff], |row| {
            Ok((
                row.get::<_, u32>(0)?,  // window_sec
                row.get::<_, f64>(1)?,  // vol_sol
                row.get::<_, u32>(2)?,  // uniq_buyers
                row.get::<_, i64>(3)?,  // start_time
                row.get::<_, f64>(4)?,  // close price
            ))
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(windows)
    }
    
    /// Get token launch time for age calculation
    pub fn get_token_launch_time(&self, mint: &str) -> Result<Option<i64>> {
        let launch_time = self.conn.query_row(
            "SELECT launch_block_time FROM tokens WHERE mint = ?1",
            params![mint],
            |row| row.get::<_, i64>(0)
        ).optional()?;
        
        Ok(launch_time)
    }

    pub fn mark_migrated(&mut self, mint: &str, pool: &str, slot: u64, block_time: i64) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE tokens 
            SET migrated_to_raydium = 1,
                migration_slot = ?1,
                migration_block_time = ?2,
                raydium_pool = ?3
            WHERE mint = ?4
            "#,
            params![slot, block_time, pool, mint],
        ).context("Failed to mark token as migrated")?;

        info!("Marked token {} as migrated to pool {}", mint, pool);
        Ok(())
    }

    /// Log Pyth price to database for analytics
    pub fn log_pyth_price(
        &mut self,
        timestamp: i64,
        price: f32,
        confidence: f32,
        confidence_ratio: f64,
        source: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO pyth_prices (timestamp, price, confidence, confidence_ratio, source) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![timestamp, price, confidence, confidence_ratio, source],
        )?;
        Ok(())
    }

    pub fn upsert_window(&mut self, window: &Window) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO windows (
                mint, window_sec, start_slot, start_time, end_time,
                num_buys, num_sells, uniq_buyers, vol_tokens, vol_sol,
                high, low, close, vwap, top1_share, top3_share, top5_share,
                price_volatility, open
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            "#,
            params![
                window.mint,
                window.window_sec,
                window.start_slot,
                window.start_time,
                window.end_time,
                window.num_buys,
                window.num_sells,
                window.uniq_buyers,
                window.vol_tokens,
                window.vol_sol,
                window.high,
                window.low,
                window.close,
                window.vwap,
                window.top1_share,
                window.top3_share,
                window.top5_share,
                window.price_volatility,
                window.open,
            ],
        ).context("Failed to upsert window")?;

        Ok(())
    }

    pub fn get_trades_for_window(
        &self,
        mint: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<Trade>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sig, slot, block_time, mint, side, trader, amount_tokens, amount_sol, price, is_amm
            FROM trades
            WHERE mint = ?1 AND block_time >= ?2 AND block_time < ?3
            ORDER BY block_time ASC
            "#
        )?;

        let trades = stmt.query_map(params![mint, start_time, end_time], |row| {
            let side_str: String = row.get(4)?;
            let side = if side_str == "buy" { TradeSide::Buy } else { TradeSide::Sell };
            
            Ok(Trade {
                sig: row.get(0)?,
                slot: row.get(1)?,
                block_time: row.get(2)?,
                mint: row.get(3)?,
                side,
                trader: row.get(5)?,
                amount_tokens: row.get(6)?,
                amount_sol: row.get(7)?,
                price: row.get(8)?,
                is_amm: row.get::<_, i32>(9)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(trades)
    }

    pub fn begin_transaction(&mut self) -> Result<()> {
        self.conn.execute("BEGIN TRANSACTION", [])?;
        Ok(())
    }

    pub fn commit_transaction(&mut self) -> Result<()> {
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    pub fn rollback_transaction(&mut self) -> Result<()> {
        self.conn.execute("ROLLBACK", [])?;
        Ok(())
    }

    // ========================================================================
    // HOTLIST METHODS (1M+ MC HUNTING)
    // ========================================================================

    /// Insert or update hotlist entry with 7-signal scoring breakdown
    pub fn upsert_hotlist(
        &mut self,
        mint: &str,
        score: f64,
        creator_score: f64,
        buyer_speed_score: f64,
        liquidity_score: f64,
        wallet_overlap_score: f64,
        concentration_score: f64,
        volume_accel_score: f64,
        mc_velocity_score: f64,
        mc_velocity: f64,
        unique_buyers_10s: u32,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        
        self.conn.execute(
            r#"
            INSERT INTO hotlist (
                mint, score, creator_score, buyer_speed_score, liquidity_score,
                wallet_overlap_score, concentration_score, volume_accel_score,
                mc_velocity_score, mc_velocity, unique_buyers_10s, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)
            ON CONFLICT(mint) DO UPDATE SET
                score = ?2,
                creator_score = ?3,
                buyer_speed_score = ?4,
                liquidity_score = ?5,
                wallet_overlap_score = ?6,
                concentration_score = ?7,
                volume_accel_score = ?8,
                mc_velocity_score = ?9,
                mc_velocity = ?10,
                unique_buyers_10s = ?11,
                updated_at = ?12
            "#,
            params![
                mint, score, creator_score, buyer_speed_score, liquidity_score,
                wallet_overlap_score, concentration_score, volume_accel_score,
                mc_velocity_score, mc_velocity, unique_buyers_10s, now
            ],
        )?;
        
        debug!("📝 Hotlist updated: {} | score: {:.1}/15.0 | MC velocity: {:.0} SOL/min", 
               &mint[..8], score, mc_velocity);
        Ok(())
    }

    /// Get top hotlist entries by score (for Brain to query)
    pub fn get_top_hotlist(&self, limit: usize, min_score: f64) -> Result<Vec<(String, f64, f64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT mint, score, mc_velocity, updated_at
             FROM hotlist 
             WHERE score >= ?1
             ORDER BY score DESC, mc_velocity DESC
             LIMIT ?2"
        )?;
        
        let entries = stmt.query_map(params![min_score, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,  // mint
                row.get::<_, f64>(1)?,     // score
                row.get::<_, f64>(2)?,     // mc_velocity
                row.get::<_, i64>(3)?,     // updated_at
            ))
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(entries)
    }

    /// Get hotlist entry by mint
    pub fn get_hotlist_entry(&self, mint: &str) -> Result<Option<(f64, f64, i64)>> {
        let result = self.conn.query_row(
            "SELECT score, mc_velocity, updated_at FROM hotlist WHERE mint = ?1",
            params![mint],
            |row| Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, f64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        ).optional()?;
        
        Ok(result)
    }

    /// Clean up old hotlist entries (older than 5 minutes)
    pub fn cleanup_old_hotlist(&mut self, age_seconds: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp() - age_seconds;
        let deleted = self.conn.execute(
            "DELETE FROM hotlist WHERE updated_at < ?1",
            params![cutoff],
        )?;
        
        if deleted > 0 {
            debug!("🧹 Cleaned up {} old hotlist entries (>{}s old)", deleted, age_seconds);
        }
        Ok(deleted)
    }

    /// Get recent tokens for hotlist scoring
    pub fn get_recent_tokens_for_scoring(
        &self,
        min_launch_time: i64,
        max_launch_time: i64,
    ) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT mint, launch_block_time 
             FROM tokens 
             WHERE launch_block_time >= ?1 AND launch_block_time <= ?2
             ORDER BY launch_block_time DESC"
        )?;
        
        let tokens = stmt.query_map([min_launch_time, max_launch_time], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
            ))
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(tokens)
    }

    /// Get recent trades for hotlist scoring
    pub fn get_recent_trades_for_scoring(
        &self,
        mint: &str,
        lookback_sec: i64,
    ) -> Result<Vec<(String, String, f64)>> {
        let cutoff = chrono::Utc::now().timestamp() - lookback_sec;
        
        let mut stmt = self.conn.prepare(
            "SELECT trader, side, amount_sol 
             FROM trades 
             WHERE mint = ?1 AND block_time >= ?2
             ORDER BY block_time ASC"
        )?;
        
        let trades = stmt.query_map(params![mint, cutoff], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(trades)
    }

    /// Get creator wallet for a token
    pub fn get_creator_wallet(&self, mint: &str) -> Result<String> {
        let creator = self.conn.query_row(
            "SELECT creator_wallet FROM tokens WHERE mint = ?1",
            params![mint],
            |row| row.get::<_, String>(0)
        )?;
        
        Ok(creator)
    }

    /// Get creator statistics (reputation scoring)
    /// Returns (net_pnl_sol, create_count) if found
    pub fn get_creator_stats(&self, creator_wallet: &str) -> Result<Option<(f64, i32)>> {
        match self.conn.query_row(
            "SELECT net_pnl_sol, create_count 
             FROM wallet_stats 
             WHERE wallet = ?1",
            params![creator_wallet],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, i32>(1)?))
        ) {
            Ok(stats) => Ok(Some(stats)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get initial liquidity for a token (for liquidity ratio calculation)
    /// Returns initial_liquidity_sol if found
    pub fn get_initial_liquidity(&self, mint: &str) -> Result<Option<f64>> {
        match self.conn.query_row(
            "SELECT initial_liquidity_sol FROM tokens WHERE mint = ?1",
            params![mint],
            |row| row.get::<_, Option<f64>>(0)
        ) {
            Ok(liq) => Ok(liq),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update initial liquidity for a token
    pub fn update_initial_liquidity(&mut self, mint: &str, liquidity_sol: f64) -> Result<()> {
        self.conn.execute(
            "UPDATE tokens SET initial_liquidity_sol = ?1 WHERE mint = ?2",
            params![liquidity_sol, mint],
        )?;
        Ok(())
    }    /// Get wallets with high profit scores (proven winners)
    /// Returns list of wallet addresses that have made significant profits
    pub fn get_profitable_wallets(
        &self,
        min_profit_sol: f64,
        min_win_rate: f64,
        limit: usize,
    ) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT wallet 
             FROM wallet_stats 
             WHERE net_pnl_sol >= ?1 
               AND win_rate >= ?2
               AND total_trades >= 5
             ORDER BY profit_score DESC, net_pnl_sol DESC
             LIMIT ?3"
        )?;
        
        let wallets = stmt.query_map(params![min_profit_sol, min_win_rate, limit], |row| {
            row.get::<_, String>(0)
        })?.collect::<Result<Vec<_>, _>>()?;
        
        Ok(wallets)
    }

    // ========================================================================
    // WALLET TRACKING METHODS
    // ========================================================================

    /// Update wallet statistics (handles BUY/SELL/CREATE actions)
    /// Returns true if this is a newly discovered wallet
    pub fn update_wallet_stats(
        &mut self,
        wallet: &str,
        action: &str,
        sol_amount: Option<f64>,
        mint: Option<&str>,
        price: Option<f64>,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();

        // Check if wallet exists
        let exists: bool = self.conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM wallet_stats WHERE wallet = ?1)",
                params![wallet],
                |row| row.get(0),
            )?;

        let is_new = !exists;

        if !exists {
            // Create new wallet entry
            self.conn.execute(
                "INSERT INTO wallet_stats (
                    wallet, first_seen, last_seen, total_trades, buy_count,
                    sell_count, create_count, total_sol_in, total_sol_out,
                    net_pnl_sol, realized_wins, realized_losses, win_rate,
                    is_tracked, profit_score
                ) VALUES (?1, ?2, ?3, 0, 0, 0, 0, 0.0, 0.0, 0.0, 0, 0, 0.0, 0, 0.0)",
                params![wallet, now, now],
            )?;
        }

        // Update based on action
        match action {
            "BUY" => {
                // Update wallet stats with weighted average entry price
                if let Some(p) = price {
                    let sol = sol_amount.unwrap_or(0.0);
                    self.conn.execute(
                        "UPDATE wallet_stats 
                         SET total_trades = total_trades + 1,
                             buy_count = buy_count + 1,
                             total_sol_in = total_sol_in + ?1,
                             avg_entry_price = (avg_entry_price * buy_count + ?2 * ?1) / (buy_count + 1),
                             last_seen = ?3
                         WHERE wallet = ?4",
                        params![sol, p, now, wallet],
                    )?;
                } else {
                    self.conn.execute(
                        "UPDATE wallet_stats 
                         SET total_trades = total_trades + 1,
                             buy_count = buy_count + 1,
                             total_sol_in = total_sol_in + ?1,
                             last_seen = ?2
                         WHERE wallet = ?3",
                        params![sol_amount.unwrap_or(0.0), now, wallet],
                    )?;
                }

                // Create or update open position if mint and price provided
                if let (Some(sol), Some(m), Some(p)) = (sol_amount, mint, price) {
                    // Check if open position exists for this wallet+mint
                    let existing: Option<(i64, f64, f64)> = self.conn
                        .query_row(
                            "SELECT bought_at, sol_spent, avg_buy_price FROM positions 
                             WHERE wallet = ?1 AND mint = ?2 AND is_closed = 0
                             ORDER BY bought_at ASC LIMIT 1",
                            params![wallet, m],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )
                        .optional()?;

                    if let Some((bought_at, prev_sol, prev_avg)) = existing {
                        // Update existing position with new weighted average
                        let new_sol_total = prev_sol + sol;
                        let new_avg = (prev_avg * prev_sol + p * sol) / new_sol_total;
                        self.conn.execute(
                            "UPDATE positions 
                             SET sol_spent = ?1, avg_buy_price = ?2
                             WHERE wallet = ?3 AND mint = ?4 AND bought_at = ?5",
                            params![new_sol_total, new_avg, wallet, m, bought_at],
                        )?;
                    } else {
                        // Create new position
                        self.conn.execute(
                            "INSERT INTO positions (
                                wallet, mint, bought_at, token_amount, sol_spent,
                                avg_buy_price, is_closed
                            ) VALUES (?1, ?2, ?3, 0.0, ?4, ?5, 0)",
                            params![wallet, m, now, sol, p],
                        )?;
                    }
                }
            }
            "SELL" => {
                // Update wallet stats with weighted average exit price
                if let Some(p) = price {
                    let sol = sol_amount.unwrap_or(0.0);
                    self.conn.execute(
                        "UPDATE wallet_stats 
                         SET total_trades = total_trades + 1,
                             sell_count = sell_count + 1,
                             total_sol_out = total_sol_out + ?1,
                             net_pnl_sol = total_sol_out + ?1 - total_sol_in,
                             avg_exit_price = (avg_exit_price * sell_count + ?2 * ?1) / (sell_count + 1),
                             last_seen = ?3
                         WHERE wallet = ?4",
                        params![sol, p, now, wallet],
                    )?;
                } else {
                    self.conn.execute(
                        "UPDATE wallet_stats 
                         SET total_trades = total_trades + 1,
                             sell_count = sell_count + 1,
                             total_sol_out = total_sol_out + ?1,
                             net_pnl_sol = total_sol_out + ?1 - total_sol_in,
                             last_seen = ?2
                         WHERE wallet = ?3",
                        params![sol_amount.unwrap_or(0.0), now, wallet],
                    )?;
                }

                // Close position and calculate P&L if mint and price provided
                if let (Some(sol_received), Some(m), Some(p)) = (sol_amount, mint, price) {
                    // Find open position
                    let position: Option<(i64, f64)> = self.conn
                        .query_row(
                            "SELECT bought_at, sol_spent FROM positions 
                             WHERE wallet = ?1 AND mint = ?2 AND is_closed = 0
                             ORDER BY bought_at ASC LIMIT 1",
                            params![wallet, m],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()?;

                    if let Some((bought_at, sol_spent)) = position {
                        let realized_pnl = sol_received - sol_spent;
                        let is_win = realized_pnl > 0.0;

                        // Close the position with avg_sell_price
                        self.conn.execute(
                            "UPDATE positions 
                             SET is_closed = 1, sold_at = ?1, sol_received = ?2, 
                                 realized_pnl = ?3, avg_sell_price = ?4
                             WHERE wallet = ?5 AND mint = ?6 AND bought_at = ?7",
                            params![now, sol_received, realized_pnl, p, wallet, m, bought_at],
                        )?;

                        // Update win/loss stats
                        if is_win {
                            self.conn.execute(
                                "UPDATE wallet_stats 
                                 SET realized_wins = realized_wins + 1,
                                     win_rate = CAST(realized_wins + 1 AS REAL) / 
                                               CAST(realized_wins + realized_losses + 1 AS REAL)
                                 WHERE wallet = ?1",
                                params![wallet],
                            )?;
                        } else {
                            self.conn.execute(
                                "UPDATE wallet_stats 
                                 SET realized_losses = realized_losses + 1,
                                     win_rate = CAST(realized_wins AS REAL) / 
                                               CAST(realized_wins + realized_losses + 1 AS REAL)
                                 WHERE wallet = ?1",
                                params![wallet],
                            )?;
                        }
                    }
                }
            }
            "CREATE" => {
                self.conn.execute(
                    "UPDATE wallet_stats 
                     SET total_trades = total_trades + 1,
                         create_count = create_count + 1,
                         last_seen = ?1
                     WHERE wallet = ?2",
                    params![now, wallet],
                )?;
            }
            _ => {}
        }

        // Update profit score
        self.conn.execute(
            "UPDATE wallet_stats 
             SET profit_score = (net_pnl_sol * 10.0) + (win_rate * 50.0)
             WHERE wallet = ?1",
            params![wallet],
        )?;

        Ok(is_new)
    }

    /// Get wallet statistics
    pub fn get_wallet_stats(&self, wallet: &str) -> Result<Option<WalletStats>> {
        let stats = self.conn
            .query_row(
                "SELECT wallet, total_trades, net_pnl_sol, win_rate, profit_score,
                        realized_wins, realized_losses, is_tracked
                 FROM wallet_stats WHERE wallet = ?1",
                params![wallet],
                |row| {
                    Ok(WalletStats {
                        wallet: row.get(0)?,
                        total_trades: row.get(1)?,
                        net_pnl_sol: row.get(2)?,
                        win_rate: row.get(3)?,
                        profit_score: row.get(4)?,
                        realized_wins: row.get(5)?,
                        realized_losses: row.get(6)?,
                        is_tracked: row.get::<_, i32>(7)? == 1,
                    })
                },
            )
            .optional()?;
        Ok(stats)
    }
    
    /// Get all tracked wallets with their aliases
    pub fn get_tracked_wallets(&self) -> Result<HashMap<String, Option<String>>> {
        let mut tracked = HashMap::new();
        let mut stmt = self.conn.prepare("SELECT wallet, alias FROM wallet_stats WHERE is_tracked = 1")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
            ))
        })?;
        
        for row in rows {
            let (wallet, alias) = row?;
            tracked.insert(wallet, alias);
        }
        
        Ok(tracked)
    }

    /// Update time-series windows for a specific mint
    /// Called after each trade is recorded
    pub fn update_windows_for_mint(
        &mut self,
        aggregator: &WindowAggregator,
        mint: &str,
        current_block_time: i64,
        current_slot: u64,
    ) -> Result<()> {
        aggregator.update_windows(self, mint, current_block_time, current_slot)
    }
}

// ============================================================================
// WALLET TRACKING DATA STRUCTURES
// ============================================================================

#[derive(Debug, Clone)]
pub struct WalletStats {
    pub wallet: String,
    pub total_trades: i32,
    pub net_pnl_sol: f64,
    pub win_rate: f64,
    pub profit_score: f64,
    pub realized_wins: i32,
    pub realized_losses: i32,
    pub is_tracked: bool,
}
