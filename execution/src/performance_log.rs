//! 📊 JSONL Performance Logger
//! 
//! Structured performance logging for deep trade analysis.
//! Each trade writes a single JSON line with complete execution metrics.
//! 
//! Output: execution/logs/performance.jsonl
//! Format: One JSON object per line (newline-delimited)

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use log::error;

/// Complete performance metrics for a single trade execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradePerformanceLog {
    // Identity
    pub decision_id: String,
    pub mint: String,
    pub side: String, // "BUY" or "SELL"
    
    // Timestamps (nanoseconds since epoch)
    pub ts_decision_ns: Option<u64>,      // Brain created decision
    pub ts_received_ns: u64,              // Executor received decision
    pub ts_execution_ns: u64,             // Transaction sent to network
    pub ts_confirmation_ns: u64,          // Transaction confirmed
    
    // Latencies (milliseconds)
    pub latency_decision_to_received_ms: Option<f64>,  // Brain → Executor
    pub latency_received_to_execution_ms: f64,         // Executor processing
    pub latency_execution_to_confirmation_ms: f64,     // Network confirmation
    pub latency_total_ms: Option<f64>,                 // Decision → Confirmation
    
    // Execution Details
    pub signature: String,
    pub position_size_usd: f64,
    pub actual_fee_lamports: Option<u64>,
    pub actual_fee_sol: Option<f64>,
    pub priority_fee_micro_lamports: u64,
    pub compute_units_used: Option<u64>,
    
    // Slippage Analysis
    pub expected_amount: Option<f64>,     // Expected tokens/SOL from simulation
    pub actual_amount: Option<f64>,       // Actual tokens/SOL from transaction
    pub slippage_pct: Option<f64>,
    pub slippage_bps: Option<i32>,
    
    // PnL (for sells)
    pub entry_price: Option<f64>,
    pub exit_price: Option<f64>,
    pub pnl_usd: Option<f64>,
    pub pnl_pct: Option<f64>,
    
    // Execution Outcome
    pub status: String, // "SUCCESS", "FAILED", "TIMEOUT"
    pub error_message: Option<String>,
    
    // Context
    pub tier: String,              // "t1", "t2", "t3", "t4"
    pub jito_bundle: bool,
    pub resubmitted: bool,
}

impl TradePerformanceLog {
    /// Log performance data to JSONL file
    pub fn write_to_file(&self, log_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure logs directory exists
        if let Some(parent) = Path::new(log_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Open file in append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        
        // Serialize to JSON and write single line
        let json_line = serde_json::to_string(self)?;
        writeln!(file, "{}", json_line)?;
        
        Ok(())
    }
    
    /// Log with automatic error handling (non-fatal)
    pub fn log(&self, log_path: &str) {
        match self.write_to_file(log_path) {
            Ok(_) => {
                // Silent success (reduce log noise)
            },
            Err(e) => {
                error!("❌ Failed to write performance log: {}", e);
            }
        }
    }
}

/// Builder for creating TradePerformanceLog with optional fields
pub struct PerformanceLogBuilder {
    log: TradePerformanceLog,
}

impl PerformanceLogBuilder {
    /// Create new builder with required fields
    pub fn new(
        decision_id: String,
        mint: String,
        side: String,
        ts_received_ns: u64,
        ts_execution_ns: u64,
        ts_confirmation_ns: u64,
    ) -> Self {
        let latency_received_to_execution_ms = 
            (ts_execution_ns.saturating_sub(ts_received_ns)) as f64 / 1_000_000.0;
        let latency_execution_to_confirmation_ms = 
            (ts_confirmation_ns.saturating_sub(ts_execution_ns)) as f64 / 1_000_000.0;
        
        Self {
            log: TradePerformanceLog {
                decision_id,
                mint,
                side,
                ts_decision_ns: None,
                ts_received_ns,
                ts_execution_ns,
                ts_confirmation_ns,
                latency_decision_to_received_ms: None,
                latency_received_to_execution_ms,
                latency_execution_to_confirmation_ms,
                latency_total_ms: None,
                signature: String::new(),
                position_size_usd: 0.0,
                actual_fee_lamports: None,
                actual_fee_sol: None,
                priority_fee_micro_lamports: 0,
                compute_units_used: None,
                expected_amount: None,
                actual_amount: None,
                slippage_pct: None,
                slippage_bps: None,
                entry_price: None,
                exit_price: None,
                pnl_usd: None,
                pnl_pct: None,
                status: "PENDING".to_string(),
                error_message: None,
                tier: "unknown".to_string(),
                jito_bundle: false,
                resubmitted: false,
            }
        }
    }
    
    pub fn decision_timestamp(mut self, ts_ns: u64) -> Self {
        self.log.ts_decision_ns = Some(ts_ns);
        self.log.latency_decision_to_received_ms = Some(
            (self.log.ts_received_ns.saturating_sub(ts_ns)) as f64 / 1_000_000.0
        );
        if let Some(ts_decision) = self.log.ts_decision_ns {
            self.log.latency_total_ms = Some(
                (self.log.ts_confirmation_ns.saturating_sub(ts_decision)) as f64 / 1_000_000.0
            );
        }
        self
    }
    
    pub fn signature(mut self, sig: String) -> Self {
        self.log.signature = sig;
        self
    }
    
    pub fn position_size(mut self, size_usd: f64) -> Self {
        self.log.position_size_usd = size_usd;
        self
    }
    
    pub fn actual_fee(mut self, lamports: u64) -> Self {
        self.log.actual_fee_lamports = Some(lamports);
        self.log.actual_fee_sol = Some(lamports as f64 / 1_000_000_000.0);
        self
    }
    
    pub fn priority_fee(mut self, micro_lamports: u64) -> Self {
        self.log.priority_fee_micro_lamports = micro_lamports;
        self
    }
    
    pub fn compute_units(mut self, cu: u64) -> Self {
        self.log.compute_units_used = Some(cu);
        self
    }
    
    pub fn slippage(mut self, expected: f64, actual: f64, slippage_bps: i32) -> Self {
        self.log.expected_amount = Some(expected);
        self.log.actual_amount = Some(actual);
        let slippage_pct = ((expected - actual) / expected) * 100.0;
        self.log.slippage_pct = Some(slippage_pct);
        self.log.slippage_bps = Some(slippage_bps);
        self
    }
    
    pub fn pnl(mut self, entry_price: f64, exit_price: f64, pnl_usd: f64) -> Self {
        self.log.entry_price = Some(entry_price);
        self.log.exit_price = Some(exit_price);
        self.log.pnl_usd = Some(pnl_usd);
        if entry_price > 0.0 {
            self.log.pnl_pct = Some(((exit_price - entry_price) / entry_price) * 100.0);
        }
        self
    }
    
    pub fn status(mut self, status: String) -> Self {
        self.log.status = status;
        self
    }
    
    pub fn error(mut self, error_msg: String) -> Self {
        self.log.error_message = Some(error_msg);
        self
    }
    
    pub fn tier(mut self, tier: String) -> Self {
        self.log.tier = tier;
        self
    }
    
    pub fn jito_bundle(mut self, enabled: bool) -> Self {
        self.log.jito_bundle = enabled;
        self
    }
    
    pub fn resubmitted(mut self, resubmitted: bool) -> Self {
        self.log.resubmitted = resubmitted;
        self
    }
    
    pub fn build(self) -> TradePerformanceLog {
        self.log
    }
}

/// Helper to get current timestamp in nanoseconds
pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_performance_log_serialization() {
        let log = PerformanceLogBuilder::new(
            "test-123".to_string(),
            "ABC123".to_string(),
            "BUY".to_string(),
            1000000000,
            1000050000,
            1000150000,
        )
        .signature("sig123".to_string())
        .position_size(100.0)
        .status("SUCCESS".to_string())
        .build();
        
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("\"decision_id\":\"test-123\""));
        assert!(json.contains("\"side\":\"BUY\""));
        assert_eq!(log.latency_received_to_execution_ms, 0.05);
        assert_eq!(log.latency_execution_to_confirmation_ms, 0.1);
    }
    
    #[test]
    fn test_latency_calculations() {
        let ts_decision = 1000000000u64;
        let ts_received = 1000020000u64; // +20µs = 0.02ms
        let ts_execution = 1000050000u64; // +30µs = 0.03ms
        let ts_confirmation = 1000200000u64; // +150µs = 0.15ms
        
        let log = PerformanceLogBuilder::new(
            "test".to_string(),
            "mint".to_string(),
            "BUY".to_string(),
            ts_received,
            ts_execution,
            ts_confirmation,
        )
        .decision_timestamp(ts_decision)
        .build();
        
        assert_eq!(log.latency_decision_to_received_ms, Some(0.02));
        assert_eq!(log.latency_received_to_execution_ms, 0.03);
        assert_eq!(log.latency_execution_to_confirmation_ms, 0.15);
        assert_eq!(log.latency_total_ms, Some(0.2));
    }
}
