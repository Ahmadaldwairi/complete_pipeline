// Telemetry Module - Send execution results back to Brain for monitoring
// Lightweight UDP telemetry for latency tracking and performance analysis

use serde::{Deserialize, Serialize};
use std::net::UdpSocket;
use anyhow::{Context, Result};
use log::{warn, debug};

/// Telemetry message sent from Executor → Brain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTelemetry {
    pub decision_id: String,              // UUID from Brain's decision
    pub mint: String,                     // Token address
    pub action: TelemetryAction,          // What happened
    pub timestamp_ns_received: u64,       // When executor received decision
    pub timestamp_ns_confirmed: u64,      // When tx confirmed (or failed)
    pub latency_exec_ms: f64,             // Execution latency (received → confirmed)
    pub status: ExecutionStatus,          // Success/failure
    pub realized_pnl_usd: Option<f64>,    // Actual PnL (for closes)
    pub error_msg: Option<String>,        // Error details if failed
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryAction {
    Buy,
    Sell,
    SkippedBuy,    // Rejected by safety checks
    SkippedSell,   // Position not found
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Success,
    Failed,
    Timeout,
    Rejected,  // Safety checks failed
}

/// Telemetry sender - non-blocking UDP
pub struct TelemetrySender {
    socket: UdpSocket,
    brain_addr: String,
    enabled: bool,
}

impl TelemetrySender {
    pub fn new(host: &str, port: u16, enabled: bool) -> Result<Self> {
        if !enabled {
            return Ok(Self {
                socket: UdpSocket::bind("0.0.0.0:0")?,
                brain_addr: String::new(),
                enabled: false,
            });
        }

        let socket = UdpSocket::bind("0.0.0.0:0")
            .context("Failed to bind telemetry socket")?;
        
        socket.set_nonblocking(true)
            .context("Failed to set telemetry socket to non-blocking")?;
        
        let brain_addr = format!("{}:{}", host, port);
        
        debug!("Telemetry sender initialized: {}", brain_addr);
        
        Ok(Self {
            socket,
            brain_addr,
            enabled: true,
        })
    }

    /// Send telemetry (fire-and-forget, non-blocking)
    pub fn send(&self, telemetry: ExecutionTelemetry) {
        if !self.enabled {
            return;
        }

        // Serialize to JSON
        let json = match serde_json::to_string(&telemetry) {
            Ok(j) => j,
            Err(e) => {
                warn!("Failed to serialize telemetry: {}", e);
                return;
            }
        };

        // Send UDP (non-blocking, fire-and-forget)
        match self.socket.send_to(json.as_bytes(), &self.brain_addr) {
            Ok(_) => {
                debug!(
                    "📡 Telemetry sent: {} {} ({}) - {}ms",
                    telemetry.decision_id,
                    telemetry.mint.chars().take(12).collect::<String>(),
                    match telemetry.action {
                        TelemetryAction::Buy => "BUY",
                        TelemetryAction::Sell => "SELL",
                        TelemetryAction::SkippedBuy => "SKIP_BUY",
                        TelemetryAction::SkippedSell => "SKIP_SELL",
                    },
                    telemetry.latency_exec_ms
                );
            }
            Err(e) => {
                // Don't spam logs if Brain is offline
                debug!("Failed to send telemetry: {}", e);
            }
        }
    }

    /// Helper: Create telemetry for successful buy
    pub fn buy_success(
        decision_id: String,
        mint: String,
        timestamp_ns_received: u64,
        timestamp_ns_confirmed: u64,
    ) -> ExecutionTelemetry {
        let latency_ms = (timestamp_ns_confirmed - timestamp_ns_received) as f64 / 1_000_000.0;
        
        ExecutionTelemetry {
            decision_id,
            mint,
            action: TelemetryAction::Buy,
            timestamp_ns_received,
            timestamp_ns_confirmed,
            latency_exec_ms: latency_ms,
            status: ExecutionStatus::Success,
            realized_pnl_usd: None,
            error_msg: None,
        }
    }

    /// Helper: Create telemetry for successful sell
    pub fn sell_success(
        decision_id: String,
        mint: String,
        timestamp_ns_received: u64,
        timestamp_ns_confirmed: u64,
        realized_pnl_usd: f64,
    ) -> ExecutionTelemetry {
        let latency_ms = (timestamp_ns_confirmed - timestamp_ns_received) as f64 / 1_000_000.0;
        
        ExecutionTelemetry {
            decision_id,
            mint,
            action: TelemetryAction::Sell,
            timestamp_ns_received,
            timestamp_ns_confirmed,
            latency_exec_ms: latency_ms,
            status: ExecutionStatus::Success,
            realized_pnl_usd: Some(realized_pnl_usd),
            error_msg: None,
        }
    }

    /// Helper: Create telemetry for failed execution
    pub fn execution_failed(
        decision_id: String,
        mint: String,
        action: TelemetryAction,
        timestamp_ns_received: u64,
        error_msg: String,
    ) -> ExecutionTelemetry {
        let timestamp_ns_confirmed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        
        let latency_ms = (timestamp_ns_confirmed - timestamp_ns_received) as f64 / 1_000_000.0;
        
        ExecutionTelemetry {
            decision_id,
            mint,
            action,
            timestamp_ns_received,
            timestamp_ns_confirmed,
            latency_exec_ms: latency_ms,
            status: ExecutionStatus::Failed,
            realized_pnl_usd: None,
            error_msg: Some(error_msg),
        }
    }
}

/// Get current time in nanoseconds (for timestamp tracking)
pub fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}
