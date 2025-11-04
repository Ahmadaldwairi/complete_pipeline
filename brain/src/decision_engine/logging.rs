//! 📝 Decision Logging
//!
//! Comprehensive logging of all trading decisions for analysis and debugging.
//! Records: decision_id, mint, trigger type, fees, impact, TP, score, size, EV, timestamp.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use log::info;
use anyhow::{Result, Context};

/// Entry trigger type for logging
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerType {
    Rank,
    Momentum,
    CopyTrade,
    LateOpportunity,
}

impl TriggerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerType::Rank => "rank",
            TriggerType::Momentum => "momentum",
            TriggerType::CopyTrade => "copy",
            TriggerType::LateOpportunity => "late",
        }
    }
    
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => TriggerType::Rank,
            1 => TriggerType::Momentum,
            2 => TriggerType::CopyTrade,
            3 => TriggerType::LateOpportunity,
            _ => TriggerType::Rank,
        }
    }
}

/// Trading decision log entry
#[derive(Debug, Clone)]
pub struct DecisionLogEntry {
    pub decision_id: u64,
    pub timestamp: u64,
    pub mint: String,
    pub trigger_type: TriggerType,
    pub side: u8, // 0=buy, 1=sell
    
    // Validation metrics
    pub predicted_fees_usd: f64,
    pub predicted_impact_usd: f64,
    pub tp_usd: f64,
    pub follow_through_score: u8,
    
    // Position sizing
    pub size_sol: f64,
    pub size_usd: f64,
    pub confidence: u8,
    
    // Expected value
    pub expected_ev_usd: f64,
    pub success_probability: f64,
    
    // Additional context
    pub rank: Option<u8>,
    pub wallet: Option<String>,
    pub wallet_tier: Option<u8>,
}

impl DecisionLogEntry {
    /// Convert to CSV row
    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{:.4},{:.4},{:.4},{},{:.4},{:.4},{},{:.4},{:.4},{},{},{},{}",
            self.decision_id,
            self.timestamp,
            self.mint,
            self.trigger_type.as_str(),
            self.side,
            self.predicted_fees_usd,
            self.predicted_impact_usd,
            self.tp_usd,
            self.follow_through_score,
            self.size_sol,
            self.size_usd,
            self.confidence,
            self.expected_ev_usd,
            self.success_probability,
            self.rank.map(|r| r.to_string()).unwrap_or_default(),
            self.wallet.as_deref().unwrap_or(""),
            self.wallet_tier.map(|t| t.to_string()).unwrap_or_default(),
            chrono::DateTime::from_timestamp(self.timestamp as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        )
    }
    
    /// CSV header
    pub fn csv_header() -> &'static str {
        "decision_id,timestamp,mint,trigger_type,side,predicted_fees_usd,predicted_impact_usd,tp_usd,follow_through_score,size_sol,size_usd,confidence,expected_ev_usd,success_probability,rank,wallet,wallet_tier,datetime"
    }
}

/// Decision logger that writes to CSV file
pub struct DecisionLogger {
    log_file: Arc<Mutex<File>>,
    decision_counter: Arc<Mutex<u64>>,
    entries_logged: Arc<Mutex<u64>>,
}

impl DecisionLogger {
    /// Create new decision logger
    /// 
    /// If the log file doesn't exist, it will be created with a CSV header.
    /// If it exists, new entries will be appended.
    pub fn new<P: AsRef<Path>>(log_path: P) -> Result<Self> {
        let path = log_path.as_ref();
        let file_exists = path.exists();
        
        // Open file in append mode
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context(format!("Failed to open log file: {:?}", path))?;
        
        // Write header if new file
        if !file_exists {
            writeln!(file, "{}", DecisionLogEntry::csv_header())
                .context("Failed to write CSV header")?;
            file.flush()?;
            info!("📝 Created new decision log: {:?}", path);
        } else {
            info!("📝 Opened existing decision log: {:?}", path);
        }
        
        Ok(Self {
            log_file: Arc::new(Mutex::new(file)),
            decision_counter: Arc::new(Mutex::new(1)),
            entries_logged: Arc::new(Mutex::new(0)),
        })
    }
    
    /// Log a trading decision
    pub fn log_decision(&self, mut entry: DecisionLogEntry) -> Result<u64> {
        // Assign decision ID
        let decision_id = {
            let mut counter = self.decision_counter.lock().unwrap();
            let id = *counter;
            *counter += 1;
            id
        };
        
        entry.decision_id = decision_id;
        
        // Write to CSV
        {
            let mut file = self.log_file.lock().unwrap();
            writeln!(file, "{}", entry.to_csv_row())
                .context("Failed to write log entry")?;
            file.flush()?;
        }
        
        // Update stats
        {
            let mut count = self.entries_logged.lock().unwrap();
            *count += 1;
        }
        
        info!(
            "📝 Logged decision #{}: mint={}..., trigger={}, size={:.2} SOL, EV=${:.2}",
            decision_id,
            &entry.mint[..8],
            entry.trigger_type.as_str(),
            entry.size_sol,
            entry.expected_ev_usd
        );
        
        Ok(decision_id)
    }
    
    /// Get total number of logged entries
    pub fn entries_logged(&self) -> u64 {
        *self.entries_logged.lock().unwrap()
    }
    
    /// Get next decision ID
    pub fn next_decision_id(&self) -> u64 {
        *self.decision_counter.lock().unwrap()
    }
}

/// Builder for creating decision log entries
pub struct DecisionLogBuilder {
    timestamp: u64,
    mint: [u8; 32],
    trigger_type: TriggerType,
    side: u8,
    
    predicted_fees_usd: Option<f64>,
    predicted_impact_usd: Option<f64>,
    tp_usd: Option<f64>,
    follow_through_score: Option<u8>,
    
    size_sol: Option<f64>,
    size_usd: Option<f64>,
    confidence: Option<u8>,
    
    expected_ev_usd: Option<f64>,
    success_probability: Option<f64>,
    
    rank: Option<u8>,
    wallet: Option<[u8; 32]>,
    wallet_tier: Option<u8>,
}

impl DecisionLogBuilder {
    /// Create new log builder
    pub fn new(mint: [u8; 32], trigger_type: TriggerType, side: u8) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            timestamp,
            mint,
            trigger_type,
            side,
            predicted_fees_usd: None,
            predicted_impact_usd: None,
            tp_usd: None,
            follow_through_score: None,
            size_sol: None,
            size_usd: None,
            confidence: None,
            expected_ev_usd: None,
            success_probability: None,
            rank: None,
            wallet: None,
            wallet_tier: None,
        }
    }
    
    /// Set validation metrics
    pub fn validation(mut self, fees: f64, impact: f64, tp: f64) -> Self {
        self.predicted_fees_usd = Some(fees);
        self.predicted_impact_usd = Some(impact);
        self.tp_usd = Some(tp);
        self
    }
    
    /// Set follow-through score
    pub fn score(mut self, score: u8) -> Self {
        self.follow_through_score = Some(score);
        self
    }
    
    /// Set position sizing
    pub fn position(mut self, size_sol: f64, size_usd: f64, confidence: u8) -> Self {
        self.size_sol = Some(size_sol);
        self.size_usd = Some(size_usd);
        self.confidence = Some(confidence);
        self
    }
    
    /// Set expected value
    pub fn ev(mut self, ev_usd: f64, success_prob: f64) -> Self {
        self.expected_ev_usd = Some(ev_usd);
        self.success_probability = Some(success_prob);
        self
    }
    
    /// Set rank (for rank-based triggers)
    pub fn rank(mut self, rank: u8) -> Self {
        self.rank = Some(rank);
        self
    }
    
    /// Set wallet info (for copy-trade triggers)
    pub fn wallet(mut self, wallet: [u8; 32], tier: u8) -> Self {
        self.wallet = Some(wallet);
        self.wallet_tier = Some(tier);
        self
    }
    
    /// Build the log entry
    pub fn build(self) -> DecisionLogEntry {
        DecisionLogEntry {
            decision_id: 0, // Will be assigned by logger
            timestamp: self.timestamp,
            mint: hex::encode(self.mint),
            trigger_type: self.trigger_type,
            side: self.side,
            predicted_fees_usd: self.predicted_fees_usd.unwrap_or(0.0),
            predicted_impact_usd: self.predicted_impact_usd.unwrap_or(0.0),
            tp_usd: self.tp_usd.unwrap_or(0.0),
            follow_through_score: self.follow_through_score.unwrap_or(0),
            size_sol: self.size_sol.unwrap_or(0.0),
            size_usd: self.size_usd.unwrap_or(0.0),
            confidence: self.confidence.unwrap_or(0),
            expected_ev_usd: self.expected_ev_usd.unwrap_or(0.0),
            success_probability: self.success_probability.unwrap_or(0.0),
            rank: self.rank,
            wallet: self.wallet.map(hex::encode),
            wallet_tier: self.wallet_tier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    #[test]
    fn test_trigger_type_conversions() {
        assert_eq!(TriggerType::Rank.as_str(), "rank");
        assert_eq!(TriggerType::CopyTrade.as_str(), "copy");
        assert_eq!(TriggerType::from_u8(0), TriggerType::Rank);
        assert_eq!(TriggerType::from_u8(2), TriggerType::CopyTrade);
    }
    
    #[test]
    fn test_csv_header() {
        let header = DecisionLogEntry::csv_header();
        assert!(header.contains("decision_id"));
        assert!(header.contains("mint"));
        assert!(header.contains("trigger_type"));
        assert!(header.contains("expected_ev_usd"));
    }
    
    #[test]
    fn test_log_builder() {
        let mint = [1u8; 32];
        let entry = DecisionLogBuilder::new(mint, TriggerType::Rank, 0)
            .validation(0.5, 0.3, 2.0)
            .score(75)
            .position(0.5, 100.0, 85)
            .ev(1.5, 0.65)
            .rank(1)
            .build();
        
        assert_eq!(entry.trigger_type, TriggerType::Rank);
        assert_eq!(entry.predicted_fees_usd, 0.5);
        assert_eq!(entry.follow_through_score, 75);
        assert_eq!(entry.confidence, 85);
        assert_eq!(entry.rank, Some(1));
    }
    
    #[test]
    fn test_csv_row_format() {
        let mint = [1u8; 32];
        let entry = DecisionLogBuilder::new(mint, TriggerType::Momentum, 0)
            .validation(0.5, 0.3, 2.0)
            .score(80)
            .position(1.0, 200.0, 90)
            .ev(2.0, 0.70)
            .build();
        
        let csv = entry.to_csv_row();
        assert!(csv.contains("momentum"));
        assert!(csv.contains("0.5000")); // fees
        assert!(csv.contains("80")); // score
    }
    
    #[test]
    fn test_logger_creation() {
        let temp_path = "/tmp/test_decisions.csv";
        let _ = fs::remove_file(temp_path); // Clean up if exists
        
        let logger = DecisionLogger::new(temp_path);
        assert!(logger.is_ok());
        
        // Verify file was created with header
        let content = fs::read_to_string(temp_path).unwrap();
        assert!(content.contains("decision_id,timestamp"));
        
        // Clean up
        let _ = fs::remove_file(temp_path);
    }
    
    #[test]
    fn test_logging_entry() {
        let temp_path = "/tmp/test_decisions_2.csv";
        let _ = fs::remove_file(temp_path);
        
        let logger = DecisionLogger::new(temp_path).unwrap();
        let mint = [2u8; 32];
        
        let entry = DecisionLogBuilder::new(mint, TriggerType::CopyTrade, 0)
            .validation(0.6, 0.4, 2.5)
            .score(85)
            .position(0.75, 150.0, 88)
            .ev(1.8, 0.68)
            .wallet([100u8; 32], 2)
            .build();
        
        let decision_id = logger.log_decision(entry);
        assert!(decision_id.is_ok());
        assert_eq!(decision_id.unwrap(), 1);
        
        // Verify file content
        let content = fs::read_to_string(temp_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2); // Header + 1 entry
        assert!(lines[1].contains("copy"));
        
        // Clean up
        let _ = fs::remove_file(temp_path);
    }
    
    #[test]
    fn test_multiple_entries() {
        let temp_path = "/tmp/test_decisions_3.csv";
        let _ = fs::remove_file(temp_path);
        
        let logger = DecisionLogger::new(temp_path).unwrap();
        
        // Log 3 entries
        for i in 0..3 {
            let mint = [i as u8; 32];
            let entry = DecisionLogBuilder::new(mint, TriggerType::Rank, 0)
                .validation(0.5, 0.3, 2.0)
                .score(70 + i * 5)
                .position(0.5, 100.0, 80)
                .ev(1.5, 0.65)
                .rank(i + 1)
                .build();
            
            let _ = logger.log_decision(entry);
        }
        
        assert_eq!(logger.entries_logged(), 3);
        assert_eq!(logger.next_decision_id(), 4);
        
        // Clean up
        let _ = fs::remove_file(temp_path);
    }
}
