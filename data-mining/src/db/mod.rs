pub mod checkpoint;
pub mod aggregator;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

use crate::types::{Token, Trade, TradeSide, Window};

pub use checkpoint::Checkpoint;
pub use aggregator::WindowAggregator;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P, wal_mode: bool) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create database directory")?;
        }

        let conn = Connection::open(path)
            .context("Failed to open database connection")?;

        if wal_mode {
            conn.execute_batch("PRAGMA journal_mode=WAL;")
                .context("Failed to enable WAL mode")?;
        }

        let mut db = Self { conn };
        db.initialize_schema()?;
        
        info!("✅ Database initialized successfully");
        Ok(db)
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
                PRIMARY KEY(mint, window_sec, start_time)
            );

            -- Indexes for performance (comprehensive coverage)
            CREATE INDEX IF NOT EXISTS idx_trades_mint_time ON trades(mint, block_time, slot);
            CREATE INDEX IF NOT EXISTS idx_trades_trader_time ON trades(trader, block_time, slot);
            CREATE INDEX IF NOT EXISTS idx_trades_slot ON trades(slot);
            CREATE INDEX IF NOT EXISTS idx_tokens_launch_time ON tokens(launch_block_time);
            CREATE INDEX IF NOT EXISTS idx_windows_mint_start ON windows(mint, start_time);
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

    pub fn insert_trade(&mut self, trade: &Trade) -> Result<()> {
        self.conn.execute(
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
        ).context("Failed to insert trade")?;

        debug!("Inserted trade: {} for mint {}", trade.sig, trade.mint);
        Ok(())
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

    pub fn upsert_window(&mut self, window: &Window) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO windows (
                mint, window_sec, start_slot, start_time, end_time,
                num_buys, num_sells, uniq_buyers, vol_tokens, vol_sol,
                high, low, close, vwap, top1_share, top3_share, top5_share
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
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
                self.conn.execute(
                    "UPDATE wallet_stats 
                     SET total_trades = total_trades + 1,
                         buy_count = buy_count + 1,
                         total_sol_in = total_sol_in + ?1,
                         last_seen = ?2
                     WHERE wallet = ?3",
                    params![sol_amount.unwrap_or(0.0), now, wallet],
                )?;

                // Create open position if mint provided
                if let (Some(sol), Some(m)) = (sol_amount, mint) {
                    self.conn.execute(
                        "INSERT INTO positions (
                            wallet, mint, bought_at, token_amount, sol_spent,
                            avg_buy_price, is_closed
                        ) VALUES (?1, ?2, ?3, 0.0, ?4, 0.0, 0)",
                        params![wallet, m, now, sol],
                    )?;
                }
            }
            "SELL" => {
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

                // Close position and calculate P&L if mint provided
                if let (Some(sol_received), Some(m)) = (sol_amount, mint) {
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

                        // Close the position
                        self.conn.execute(
                            "UPDATE positions 
                             SET is_closed = 1, sold_at = ?1, sol_received = ?2, realized_pnl = ?3
                             WHERE wallet = ?4 AND mint = ?5 AND bought_at = ?6",
                            params![now, sol_received, realized_pnl, wallet, m, bought_at],
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
