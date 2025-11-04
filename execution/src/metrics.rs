/*!
 * Prometheus Metrics Module for Brain Service
 * 
 * Add to brain/Cargo.toml:
 * ```toml
 * prometheus = "0.13"
 * axum = "0.7"
 * ```
 * 
 * Usage:
 * 1. Copy this file to brain/src/metrics.rs
 * 2. Add `mod metrics;` to brain/src/main.rs
 * 3. Call metrics::init_metrics() at startup
 * 4. Start HTTP server with metrics::start_metrics_server()
 * 5. Use metrics throughout the codebase
 */

use prometheus::{
    Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry,
};
use std::sync::Arc;
use tokio::net::TcpListener;
use axum::{
    routing::get,
    Router,
    response::{IntoResponse, Response},
    http::StatusCode,
};
use log::{info, error};

/// Global metrics registry
static METRICS: once_cell::sync::Lazy<Arc<BrainMetrics>> = once_cell::sync::Lazy::new(|| {
    Arc::new(BrainMetrics::new())
});

/// Brain service metrics
pub struct BrainMetrics {
    // Registry for Prometheus
    registry: Registry,
    
    // Decision counters
    pub decisions_total: IntCounter,
    pub decisions_approved: IntCounter,
    pub decisions_rejected: IntCounter,
    
    // Decision breakdown by type
    pub copytrade_decisions: IntCounter,
    pub newlaunch_decisions: IntCounter,
    pub wallet_activity_decisions: IntCounter,
    
    // Rejection reasons
    pub rejected_low_confidence: IntCounter,
    pub rejected_guardrails: IntCounter,
    pub rejected_validation: IntCounter,
    
    // Cache metrics
    pub mint_cache_hits: IntCounter,
    pub mint_cache_misses: IntCounter,
    pub wallet_cache_hits: IntCounter,
    pub wallet_cache_misses: IntCounter,
    
    // Guardrail blocks
    pub guardrail_loss_backoff: IntCounter,
    pub guardrail_position_limit: IntCounter,
    pub guardrail_rate_limit: IntCounter,
    pub guardrail_wallet_cooling: IntCounter,
    
    // Performance metrics
    pub decision_latency: Histogram,
    pub advice_processing_latency: Histogram,
    
    // System metrics
    pub sol_price_usd: Gauge,
    pub active_positions: IntGauge,
    pub advice_messages_received: IntCounter,
    pub decision_messages_sent: IntCounter,
    
    // Database metrics
    pub db_query_duration: Histogram,
    pub db_errors: IntCounter,
    
    // UDP metrics
    pub udp_packets_received: IntCounter,
    pub udp_packets_sent: IntCounter,
    pub udp_parse_errors: IntCounter,
}

impl BrainMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();
        
        // Decision counters
        let decisions_total = IntCounter::with_opts(
            Opts::new("brain_decisions_total", "Total number of trading decisions made")
        ).unwrap();
        registry.register(Box::new(decisions_total.clone())).unwrap();
        
        let decisions_approved = IntCounter::with_opts(
            Opts::new("brain_decisions_approved", "Number of approved trading decisions")
        ).unwrap();
        registry.register(Box::new(decisions_approved.clone())).unwrap();
        
        let decisions_rejected = IntCounter::with_opts(
            Opts::new("brain_decisions_rejected", "Number of rejected trading decisions")
        ).unwrap();
        registry.register(Box::new(decisions_rejected.clone())).unwrap();
        
        // Decision type breakdown
        let copytrade_decisions = IntCounter::with_opts(
            Opts::new("brain_copytrade_decisions", "CopyTrade decision pathway triggers")
        ).unwrap();
        registry.register(Box::new(copytrade_decisions.clone())).unwrap();
        
        let newlaunch_decisions = IntCounter::with_opts(
            Opts::new("brain_newlaunch_decisions", "NewLaunch decision pathway triggers")
        ).unwrap();
        registry.register(Box::new(newlaunch_decisions.clone())).unwrap();
        
        let wallet_activity_decisions = IntCounter::with_opts(
            Opts::new("brain_wallet_activity_decisions", "WalletActivity decision pathway triggers")
        ).unwrap();
        registry.register(Box::new(wallet_activity_decisions.clone())).unwrap();
        
        // Rejection reasons
        let rejected_low_confidence = IntCounter::with_opts(
            Opts::new("brain_rejected_low_confidence", "Decisions rejected due to low confidence")
        ).unwrap();
        registry.register(Box::new(rejected_low_confidence.clone())).unwrap();
        
        let rejected_guardrails = IntCounter::with_opts(
            Opts::new("brain_rejected_guardrails", "Decisions blocked by guardrails")
        ).unwrap();
        registry.register(Box::new(rejected_guardrails.clone())).unwrap();
        
        let rejected_validation = IntCounter::with_opts(
            Opts::new("brain_rejected_validation", "Decisions rejected during validation")
        ).unwrap();
        registry.register(Box::new(rejected_validation.clone())).unwrap();
        
        // Cache metrics
        let mint_cache_hits = IntCounter::with_opts(
            Opts::new("brain_mint_cache_hits", "Mint cache hits")
        ).unwrap();
        registry.register(Box::new(mint_cache_hits.clone())).unwrap();
        
        let mint_cache_misses = IntCounter::with_opts(
            Opts::new("brain_mint_cache_misses", "Mint cache misses")
        ).unwrap();
        registry.register(Box::new(mint_cache_misses.clone())).unwrap();
        
        let wallet_cache_hits = IntCounter::with_opts(
            Opts::new("brain_wallet_cache_hits", "Wallet cache hits")
        ).unwrap();
        registry.register(Box::new(wallet_cache_hits.clone())).unwrap();
        
        let wallet_cache_misses = IntCounter::with_opts(
            Opts::new("brain_wallet_cache_misses", "Wallet cache misses")
        ).unwrap();
        registry.register(Box::new(wallet_cache_misses.clone())).unwrap();
        
        // Guardrail blocks
        let guardrail_loss_backoff = IntCounter::with_opts(
            Opts::new("brain_guardrail_loss_backoff", "Decisions blocked by loss backoff")
        ).unwrap();
        registry.register(Box::new(guardrail_loss_backoff.clone())).unwrap();
        
        let guardrail_position_limit = IntCounter::with_opts(
            Opts::new("brain_guardrail_position_limit", "Decisions blocked by position limit")
        ).unwrap();
        registry.register(Box::new(guardrail_position_limit.clone())).unwrap();
        
        let guardrail_rate_limit = IntCounter::with_opts(
            Opts::new("brain_guardrail_rate_limit", "Decisions blocked by rate limit")
        ).unwrap();
        registry.register(Box::new(guardrail_rate_limit.clone())).unwrap();
        
        let guardrail_wallet_cooling = IntCounter::with_opts(
            Opts::new("brain_guardrail_wallet_cooling", "Decisions blocked by wallet cooling")
        ).unwrap();
        registry.register(Box::new(guardrail_wallet_cooling.clone())).unwrap();
        
        // Performance histograms
        let decision_latency = Histogram::with_opts(
            HistogramOpts::new("brain_decision_latency_seconds", "Decision processing latency")
                .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5])
        ).unwrap();
        registry.register(Box::new(decision_latency.clone())).unwrap();
        
        let advice_processing_latency = Histogram::with_opts(
            HistogramOpts::new("brain_advice_processing_latency_seconds", "Advice message processing latency")
                .buckets(vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1])
        ).unwrap();
        registry.register(Box::new(advice_processing_latency.clone())).unwrap();
        
        // System gauges
        let sol_price_usd = Gauge::with_opts(
            Opts::new("brain_sol_price_usd", "Current SOL price in USD")
        ).unwrap();
        registry.register(Box::new(sol_price_usd.clone())).unwrap();
        
        let active_positions = IntGauge::with_opts(
            Opts::new("brain_active_positions", "Number of active positions")
        ).unwrap();
        registry.register(Box::new(active_positions.clone())).unwrap();
        
        let advice_messages_received = IntCounter::with_opts(
            Opts::new("brain_advice_messages_received", "Total advice messages received")
        ).unwrap();
        registry.register(Box::new(advice_messages_received.clone())).unwrap();
        
        let decision_messages_sent = IntCounter::with_opts(
            Opts::new("brain_decision_messages_sent", "Total decision messages sent")
        ).unwrap();
        registry.register(Box::new(decision_messages_sent.clone())).unwrap();
        
        // Database metrics
        let db_query_duration = Histogram::with_opts(
            HistogramOpts::new("brain_db_query_duration_seconds", "Database query duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
        ).unwrap();
        registry.register(Box::new(db_query_duration.clone())).unwrap();
        
        let db_errors = IntCounter::with_opts(
            Opts::new("brain_db_errors", "Database error count")
        ).unwrap();
        registry.register(Box::new(db_errors.clone())).unwrap();
        
        // UDP metrics
        let udp_packets_received = IntCounter::with_opts(
            Opts::new("brain_udp_packets_received", "UDP packets received")
        ).unwrap();
        registry.register(Box::new(udp_packets_received.clone())).unwrap();
        
        let udp_packets_sent = IntCounter::with_opts(
            Opts::new("brain_udp_packets_sent", "UDP packets sent")
        ).unwrap();
        registry.register(Box::new(udp_packets_sent.clone())).unwrap();
        
        let udp_parse_errors = IntCounter::with_opts(
            Opts::new("brain_udp_parse_errors", "UDP packet parse errors")
        ).unwrap();
        registry.register(Box::new(udp_parse_errors.clone())).unwrap();
        
        Self {
            registry,
            decisions_total,
            decisions_approved,
            decisions_rejected,
            copytrade_decisions,
            newlaunch_decisions,
            wallet_activity_decisions,
            rejected_low_confidence,
            rejected_guardrails,
            rejected_validation,
            mint_cache_hits,
            mint_cache_misses,
            wallet_cache_hits,
            wallet_cache_misses,
            guardrail_loss_backoff,
            guardrail_position_limit,
            guardrail_rate_limit,
            guardrail_wallet_cooling,
            decision_latency,
            advice_processing_latency,
            sol_price_usd,
            active_positions,
            advice_messages_received,
            decision_messages_sent,
            db_query_duration,
            db_errors,
            udp_packets_received,
            udp_packets_sent,
            udp_parse_errors,
        }
    }
    
    /// Get the metrics registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

/// Get global metrics instance
pub fn metrics() -> Arc<BrainMetrics> {
    METRICS.clone()
}

/// Initialize metrics (called at startup)
pub fn init_metrics() {
    // Force initialization of lazy static
    let _ = METRICS.clone();
    info!("📊 Metrics system initialized");
}

/// Start Prometheus metrics HTTP server
pub async fn start_metrics_server(port: u16) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    
    info!("📊 Starting metrics server on {}", addr);
    
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler));
    
    let listener = TcpListener::bind(&addr).await?;
    
    info!("✓ Metrics server listening on http://{}", addr);
    info!("  • Metrics endpoint: http://{}/metrics", addr);
    info!("  • Health endpoint: http://{}/health", addr);
    
    axum::serve(listener, app).await?;
    
    Ok(())
}

/// Metrics endpoint handler
async fn metrics_handler() -> Response {
    let metrics = METRICS.clone();
    let encoder = prometheus::TextEncoder::new();
    
    match encoder.encode_to_string(&metrics.registry().gather()) {
        Ok(body) => {
            (
                StatusCode::OK,
                [("content-type", "text/plain; version=0.0.4")],
                body,
            ).into_response()
        }
        Err(e) => {
            error!("Failed to encode metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to encode metrics: {}", e),
            ).into_response()
        }
    }
}

/// Health check endpoint
async fn health_handler() -> Response {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r#"{"status":"healthy","service":"brain"}"#,
    ).into_response()
}

// ============================================================================
// Helper Functions for Recording Metrics
// ============================================================================

/// Record a decision approval
pub fn record_decision_approved() {
    let m = metrics();
    m.decisions_total.inc();
    m.decisions_approved.inc();
}

/// Record a decision rejection
pub fn record_decision_rejected(reason: RejectionReason) {
    let m = metrics();
    m.decisions_total.inc();
    m.decisions_rejected.inc();
    
    match reason {
        RejectionReason::LowConfidence => m.rejected_low_confidence.inc(),
        RejectionReason::Guardrails => m.rejected_guardrails.inc(),
        RejectionReason::Validation => m.rejected_validation.inc(),
    }
}

/// Decision rejection reasons
pub enum RejectionReason {
    LowConfidence,
    Guardrails,
    Validation,
}

/// Record decision pathway trigger
pub fn record_decision_pathway(pathway: DecisionPathway) {
    let m = metrics();
    match pathway {
        DecisionPathway::CopyTrade => m.copytrade_decisions.inc(),
        DecisionPathway::NewLaunch => m.newlaunch_decisions.inc(),
        DecisionPathway::WalletActivity => m.wallet_activity_decisions.inc(),
    }
}

/// Decision pathway types
pub enum DecisionPathway {
    CopyTrade,
    NewLaunch,
    WalletActivity,
}

/// Record guardrail block
pub fn record_guardrail_block(guardrail: GuardrailType) {
    let m = metrics();
    m.rejected_guardrails.inc();
    
    match guardrail {
        GuardrailType::LossBackoff => m.guardrail_loss_backoff.inc(),
        GuardrailType::PositionLimit => m.guardrail_position_limit.inc(),
        GuardrailType::RateLimit => m.guardrail_rate_limit.inc(),
        GuardrailType::WalletCooling => m.guardrail_wallet_cooling.inc(),
    }
}

/// Guardrail types
pub enum GuardrailType {
    LossBackoff,
    PositionLimit,
    RateLimit,
    WalletCooling,
}

/// Record cache hit/miss
pub fn record_cache_access(cache: CacheType, hit: bool) {
    let m = metrics();
    match (cache, hit) {
        (CacheType::Mint, true) => m.mint_cache_hits.inc(),
        (CacheType::Mint, false) => m.mint_cache_misses.inc(),
        (CacheType::Wallet, true) => m.wallet_cache_hits.inc(),
        (CacheType::Wallet, false) => m.wallet_cache_misses.inc(),
    }
}

/// Cache types
pub enum CacheType {
    Mint,
    Wallet,
}

/// Update SOL price gauge
pub fn update_sol_price(price: f32) {
    metrics().sol_price_usd.set(price as f64);
}

/// Update active positions count
pub fn update_active_positions(count: i64) {
    metrics().active_positions.set(count);
}

/// Record advice message received
pub fn record_advice_received() {
    let m = metrics();
    m.advice_messages_received.inc();
    m.udp_packets_received.inc();
}

/// Record decision message sent
pub fn record_decision_sent() {
    let m = metrics();
    m.decision_messages_sent.inc();
    m.udp_packets_sent.inc();
}

/// Record UDP parse error
pub fn record_udp_parse_error() {
    metrics().udp_parse_errors.inc();
}

/// Record database error
pub fn record_db_error() {
    metrics().db_errors.inc();
}

/// Timer for measuring decision latency
pub struct DecisionTimer {
    start: std::time::Instant,
}

impl DecisionTimer {
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
    
    pub fn observe(self) {
        let duration = self.start.elapsed().as_secs_f64();
        metrics().decision_latency.observe(duration);
    }
}

/// Timer for measuring advice processing latency
pub struct AdviceTimer {
    start: std::time::Instant,
}

impl AdviceTimer {
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
    
    pub fn observe(self) {
        let duration = self.start.elapsed().as_secs_f64();
        metrics().advice_processing_latency.observe(duration);
    }
}

/// Timer for measuring database query duration
pub struct DbQueryTimer {
    start: std::time::Instant,
}

impl DbQueryTimer {
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
    
    pub fn observe(self) {
        let duration = self.start.elapsed().as_secs_f64();
        metrics().db_query_duration.observe(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_initialization() {
        init_metrics();
        let m = metrics();
        
        // Test counter increments
        m.decisions_approved.inc();
        assert!(m.decisions_approved.get() > 0);
    }
    
    #[test]
    fn test_helper_functions() {
        record_decision_approved();
        record_decision_rejected(RejectionReason::LowConfidence);
        record_decision_pathway(DecisionPathway::CopyTrade);
        record_guardrail_block(GuardrailType::PositionLimit);
        record_cache_access(CacheType::Mint, true);
        update_sol_price(195.50);
        update_active_positions(3);
    }
}
