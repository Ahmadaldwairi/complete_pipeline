use crate::config::Config;
use crate::trading::{BuyResult, ExitResult};
use tokio_postgres::{Client, NoTls};
use log::{info, error, debug};
use chrono::{Local};
use std::time::Instant;

/// Tracks end-to-end latency for a trade
#[derive(Debug, Clone)]
pub struct LatencyTrace {
    pub trace_id: String,
    pub t0_detect: Instant,      // Detection timestamp
    pub t1_decide: Option<Instant>,  // Decision made
    pub t2_build: Option<Instant>,   // Transaction built
    pub t3_send: Option<Instant>,    // Sent to TPU/Jito
    pub t4_landed: Option<Instant>,  // Observed in block (from gRPC)
    pub t5_confirm: Option<Instant>, // Confirmed
    
    // Additional metadata
    pub slot: Option<u64>,
    pub tx_index: Option<u32>,
    pub rank_in_slot: Option<u32>,
    pub pending_buys_at_entry: u32,
}

impl LatencyTrace {
    pub fn new(trace_id: String) -> Self {
        Self {
            trace_id,
            t0_detect: Instant::now(),
            t1_decide: None,
            t2_build: None,
            t3_send: None,
            t4_landed: None,
            t5_confirm: None,
            slot: None,
            tx_index: None,
            rank_in_slot: None,
            pending_buys_at_entry: 0,
        }
    }
    
    /// Mark decision point
    pub fn mark_decide(&mut self) {
        self.t1_decide = Some(Instant::now());
    }
    
    /// Mark transaction built
    pub fn mark_build(&mut self) {
        self.t2_build = Some(Instant::now());
    }
    
    /// Mark transaction sent
    pub fn mark_send(&mut self) {
        self.t3_send = Some(Instant::now());
    }
    
    /// Mark transaction landed in block
    pub fn mark_landed(&mut self, slot: u64, tx_index: u32) {
        self.t4_landed = Some(Instant::now());
        self.slot = Some(slot);
        self.tx_index = Some(tx_index);
    }
    
    /// Mark transaction confirmed
    pub fn mark_confirm(&mut self) {
        self.t5_confirm = Some(Instant::now());
    }
    
    /// Calculate latency from detect to send (microseconds)
    pub fn latency_detect_to_send_us(&self) -> Option<u64> {
        self.t3_send.map(|t3| (t3 - self.t0_detect).as_micros() as u64)
    }
    
    /// Calculate latency from send to land (microseconds)
    pub fn latency_send_to_land_us(&self) -> Option<u64> {
        match (self.t3_send, self.t4_landed) {
            (Some(t3), Some(t4)) => Some((t4 - t3).as_micros() as u64),
            _ => None,
        }
    }
    
    /// Calculate latency from detect to land (microseconds)
    pub fn latency_detect_to_land_us(&self) -> Option<u64> {
        self.t4_landed.map(|t4| (t4 - self.t0_detect).as_micros() as u64)
    }
    
    /// Calculate latency from land to confirm (microseconds)
    pub fn latency_land_to_confirm_us(&self) -> Option<u64> {
        match (self.t4_landed, self.t5_confirm) {
            (Some(t4), Some(t5)) => Some((t5 - t4).as_micros() as u64),
            _ => None,
        }
    }
    
    /// Print latency breakdown
    pub fn print_breakdown(&self) {
        println!("📊 Latency Breakdown [{}]:", &self.trace_id[..8]);
        
        if let Some(t1) = self.t1_decide {
            let decide_us = (t1 - self.t0_detect).as_micros();
            println!("   ⏱️  Detect → Decide: {} µs", decide_us);
        }
        
        if let (Some(t1), Some(t2)) = (self.t1_decide, self.t2_build) {
            let build_us = (t2 - t1).as_micros();
            println!("   🔨 Decide → Build: {} µs", build_us);
        }
        
        if let (Some(t2), Some(t3)) = (self.t2_build, self.t3_send) {
            let send_us = (t3 - t2).as_micros();
            println!("   📤 Build → Send: {} µs", send_us);
        }
        
        if let Some(latency) = self.latency_send_to_land_us() {
            println!("   🌐 Send → Land: {} µs ({:.2} ms)", latency, latency as f64 / 1000.0);
        }
        
        if let Some(latency) = self.latency_land_to_confirm_us() {
            println!("   ✅ Land → Confirm: {} µs ({:.2} ms)", latency, latency as f64 / 1000.0);
        }
        
        if let Some(total) = self.latency_detect_to_land_us() {
            println!("   🎯 TOTAL (Detect → Land): {} µs ({:.2} ms)", total, total as f64 / 1000.0);
        }
        
        if let Some(slot) = self.slot {
            println!("   📍 Slot: {} | Index: {:?} | Rank: {:?}", 
                slot, self.tx_index, self.rank_in_slot);
        }
    }
}

pub struct Database {
    client: Client,
}

impl Database {
    pub async fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let connection_string = format!(
            "host={} port={} dbname={} user={} password={}",
            config.db_host,
            config.db_port,
            config.db_name,
            config.db_user,
            config.db_password
        );
        
        let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;
        
        // Spawn connection handler
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Database connection error: {}", e);
            }
        });
        
        // Create table if not exists
        client.execute(
            "CREATE TABLE IF NOT EXISTS my_trades (
                id SERIAL PRIMARY KEY,
                token_address VARCHAR(44),
                entry_time TIMESTAMP,
                exit_time TIMESTAMP,
                entry_price DOUBLE PRECISION,
                exit_price DOUBLE PRECISION,
                position_size DOUBLE PRECISION,
                profit_loss DOUBLE PRECISION,
                tier VARCHAR(20),
                holding_time INTEGER,
                entry_position INTEGER,
                mempool_volume DOUBLE PRECISION,
                signature_buy VARCHAR(88),
                signature_sell VARCHAR(88),
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            &[],
        ).await?;
        
        // Add new latency tracking columns if they don't exist
        let add_columns = vec![
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS trace_id TEXT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS slot BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS tx_index INTEGER",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS rank_in_slot INTEGER",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS t_detect_ns BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS t_decide_ns BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS t_build_ns BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS t_send_ns BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS t_landed_ns BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS t_confirm_ns BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS confirmed_slot BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS cu_price_ulamports BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS cu_limit INTEGER",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS slippage_bps INTEGER",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS amount_lamports BIGINT",
            "ALTER TABLE my_trades ADD COLUMN IF NOT EXISTS pending_buys_at_entry INTEGER",
        ];
        
        for alter_sql in add_columns {
            if let Err(e) = client.execute(alter_sql, &[]).await {
                // Ignore "column already exists" errors
                if !e.to_string().contains("already exists") {
                    error!("Error adding column: {}", e);
                }
            }
        }
        
        info!("Database table 'my_trades' ready");
        
        // Create executions table for clean PnL tracking
        // This table tracks realized PnL with actual fees and slippage
        client.execute(
            "CREATE TABLE IF NOT EXISTS executions (
                decision_id TEXT PRIMARY KEY,
                mint TEXT NOT NULL,
                open_sig TEXT NOT NULL,
                close_sig TEXT,
                entry_sol REAL NOT NULL,
                exit_sol REAL,
                fee_entry_sol REAL NOT NULL,
                fee_exit_sol REAL,
                entry_slip_pct REAL,
                exit_slip_pct REAL,
                net_pnl_sol REAL,
                net_pnl_usd REAL,
                tp_hit INTEGER DEFAULT 0,
                sl_hit INTEGER DEFAULT 0,
                ts_open BIGINT NOT NULL,
                ts_close BIGINT,
                status TEXT DEFAULT 'open' CHECK(status IN ('open', 'closed', 'failed')),
                sol_price_usd REAL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            &[],
        ).await?;
        
        // Create index on status for fast open position queries
        client.execute(
            "CREATE INDEX IF NOT EXISTS idx_executions_status ON executions(status)",
            &[],
        ).await?;
        
        // Create index on mint for fast lookup by token
        client.execute(
            "CREATE INDEX IF NOT EXISTS idx_executions_mint ON executions(mint)",
            &[],
        ).await?;
        
        info!("Database table 'executions' ready");
        
        Ok(Database { client })
    }
    
    pub async fn log_trade(
        &self,
        buy: &BuyResult,
        exit: &ExitResult,
        trace: Option<&LatencyTrace>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Convert DateTime<Local> to NaiveDateTime for PostgreSQL
        let entry_time = buy.timestamp.naive_local();
        let exit_time = Local::now().naive_local();
        
        // Extract latency trace data if available
        let (trace_id, slot, tx_index, rank_in_slot, 
             t_detect_ns, t_decide_ns, t_build_ns, t_send_ns, t_landed_ns, t_confirm_ns,
             pending_buys) = if let Some(t) = trace {
            let base = t.t0_detect;
            (
                Some(t.trace_id.clone()),
                t.slot.map(|s| s as i64),
                t.tx_index.map(|i| i as i32),
                t.rank_in_slot.map(|r| r as i32),
                Some(base.elapsed().as_nanos() as i64),  // Using base as reference
                t.t1_decide.map(|t1| (t1 - base).as_nanos() as i64),
                t.t2_build.map(|t2| (t2 - base).as_nanos() as i64),
                t.t3_send.map(|t3| (t3 - base).as_nanos() as i64),
                t.t4_landed.map(|t4| (t4 - base).as_nanos() as i64),
                t.t5_confirm.map(|t5| (t5 - base).as_nanos() as i64),
                Some(t.pending_buys_at_entry as i32),
            )
        } else {
            (None, None, None, None, None, None, None, None, None, None, None)
        };
        
        self.client.execute(
            "INSERT INTO my_trades (
                token_address, entry_time, exit_time, entry_price, exit_price,
                position_size, profit_loss, net_profit_sol, tier, holding_time, entry_position,
                mempool_volume, signature_buy, signature_sell,
                trace_id, slot, tx_index, rank_in_slot,
                t_detect_ns, t_decide_ns, t_build_ns, t_send_ns, t_landed_ns, t_confirm_ns,
                pending_buys_at_entry
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25)",
            &[
                &buy.token_address,
                &entry_time,
                &exit_time,
                &buy.price,
                &exit.exit_price,
                &buy.position_size,
                &exit.net_profit,
                &exit.net_profit_sol,
                &exit.tier,
                &(exit.holding_time as i32),
                &(buy.actual_position as i32),
                &buy.mempool_volume,
                &buy.signature,
                &exit.signature,
                &trace_id,
                &slot,
                &tx_index,
                &rank_in_slot,
                &t_detect_ns,
                &t_decide_ns,
                &t_build_ns,
                &t_send_ns,
                &t_landed_ns,
                &t_confirm_ns,
                &pending_buys,
            ],
        ).await?;
        
        Ok(())
    }
    
    /// Update trace with transaction landing metrics from gRPC monitoring
    pub async fn update_trace_landing(
        &self,
        trace_id: &str,
        slot: u64,
        tx_index: u64,
        rank_in_slot: u32,
        t_landed_ns: i64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.execute(
            "UPDATE my_trades 
             SET slot = $1, 
                 tx_index = $2, 
                 rank_in_slot = $3, 
                 t_landed_ns = $4
             WHERE trace_id = $5",
            &[&(slot as i64), &(tx_index as i64), &(rank_in_slot as i32), &t_landed_ns, &trace_id],
        ).await?;
        
        Ok(())
    }
    
    /// Update trace with confirmation timing (Phase 5: t5_confirm)
    pub async fn update_trace_confirm(
        &self,
        trace_id: &str,
        t_confirm_ns: i64,
        confirmed_slot: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Retry logic for DB connection issues
        let mut attempts = 0;
        let max_attempts = 3;
        
        loop {
            attempts += 1;
            
            match self.client.execute(
                "UPDATE my_trades 
                 SET t_confirm_ns = $1,
                     confirmed_slot = $2
                 WHERE trace_id = $3",
                &[&t_confirm_ns, &(confirmed_slot as i64), &trace_id],
            ).await {
                Ok(_) => {
                    debug!("✅ Updated t5 confirmation for trace {}", &trace_id[..8]);
                    return Ok(());
                }
                Err(e) => {
                    error!("DB confirm update failed (attempt {}/{}): {} - trace: {}", 
                        attempts, max_attempts, e, &trace_id[..8]);
                    
                    if attempts >= max_attempts {
                        return Err(Box::new(e));
                    }
                    
                                        // Small backoff before retry
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }
    
    // ============================================================================
    // Executions Table Methods (Clean PnL Tracking)
    // ============================================================================
    
    /// Record a new execution (entry)
    pub async fn insert_execution(
        &self,
        decision_id: &str,
        mint: &str,
        open_sig: &str,
        entry_sol: f64,
        fee_entry_sol: f64,
        entry_slip_pct: Option<f64>,
        sol_price_usd: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ts_open = chrono::Utc::now().timestamp();
        
        self.client.execute(
            "INSERT INTO executions (
                decision_id, mint, open_sig, entry_sol, fee_entry_sol, 
                entry_slip_pct, ts_open, status, sol_price_usd
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'open', $8)",
            &[
                &decision_id,
                &mint,
                &open_sig,
                &(entry_sol as f32),
                &(fee_entry_sol as f32),
                &entry_slip_pct.map(|s| s as f32),
                &ts_open,
                &(sol_price_usd as f32),
            ],
        ).await?;
        
        debug!("📝 Execution recorded: {} (entry: {:.4} SOL, fee: {:.6} SOL)", 
            &decision_id[..8], entry_sol, fee_entry_sol);
        
        Ok(())
    }
    
    /// Update execution with exit data
    pub async fn update_execution_exit(
        &self,
        decision_id: &str,
        close_sig: &str,
        exit_sol: f64,
        fee_exit_sol: f64,
        exit_slip_pct: Option<f64>,
        tp_hit: bool,
        sl_hit: bool,
        sol_price_usd: f64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ts_close = chrono::Utc::now().timestamp();
        
        // Fetch entry data to calculate PnL
        let row = self.client.query_one(
            "SELECT entry_sol, fee_entry_sol FROM executions WHERE decision_id = $1",
            &[&decision_id],
        ).await?;
        
        let entry_sol: f32 = row.get(0);
        let fee_entry_sol: f32 = row.get(1);
        
        // Calculate net PnL
        let gross_pnl_sol = exit_sol - entry_sol as f64;
        let total_fees_sol = fee_entry_sol as f64 + fee_exit_sol;
        let net_pnl_sol = gross_pnl_sol - total_fees_sol;
        let net_pnl_usd = net_pnl_sol * sol_price_usd;
        
        self.client.execute(
            "UPDATE executions SET 
                close_sig = $1,
                exit_sol = $2,
                fee_exit_sol = $3,
                exit_slip_pct = $4,
                net_pnl_sol = $5,
                net_pnl_usd = $6,
                tp_hit = $7,
                sl_hit = $8,
                ts_close = $9,
                status = 'closed',
                updated_at = CURRENT_TIMESTAMP
             WHERE decision_id = $10",
            &[
                &close_sig,
                &(exit_sol as f32),
                &(fee_exit_sol as f32),
                &exit_slip_pct.map(|s| s as f32),
                &(net_pnl_sol as f32),
                &(net_pnl_usd as f32),
                &(if tp_hit { 1 } else { 0 }),
                &(if sl_hit { 1 } else { 0 }),
                &ts_close,
                &decision_id,
            ],
        ).await?;
        
        info!("💰 Execution closed: {} (PnL: {:.4} SOL / ${:.2} USD, TP: {}, SL: {})", 
            &decision_id[..8], net_pnl_sol, net_pnl_usd, tp_hit, sl_hit);
        
        Ok(())
    }
    
    /// Mark execution as failed
    pub async fn mark_execution_failed(
        &self,
        decision_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client.execute(
            "UPDATE executions SET 
                status = 'failed',
                updated_at = CURRENT_TIMESTAMP
             WHERE decision_id = $1",
            &[&decision_id],
        ).await?;
        
        info!("❌ Execution marked failed: {}", &decision_id[..8]);
        
        Ok(())
    }
    
    /// Get total PnL stats from executions table
    pub async fn get_pnl_stats(&self) -> Result<(f64, f64, i64), Box<dyn std::error::Error + Send + Sync>> {
        let row = self.client.query_one(
            "SELECT 
                COALESCE(SUM(net_pnl_sol), 0) as total_pnl_sol,
                COALESCE(SUM(net_pnl_usd), 0) as total_pnl_usd,
                COUNT(*) FILTER (WHERE status = 'closed') as closed_count
             FROM executions",
            &[],
        ).await?;
        
        let total_pnl_sol: f32 = row.get(0);
        let total_pnl_usd: f32 = row.get(1);
        let closed_count: i64 = row.get(2);
        
        Ok((total_pnl_sol as f64, total_pnl_usd as f64, closed_count))
    }
}
