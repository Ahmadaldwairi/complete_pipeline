//! 📊 Latency Tracker - 4-timestamp instrumentation for bottleneck detection
//!
//! Tracks: created_ns → enqueued_ns → flushed_ns → brain_recv_ns
//! Reports: p50/p90/p99 histograms every 5s

use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{interval, Duration};
use tracing::{info, warn};

/// Get current timestamp in nanoseconds
pub fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Simple histogram for latency tracking
pub struct LatencyHistogram {
    samples: Vec<u64>,
    name: String,
}

impl LatencyHistogram {
    pub fn new(name: &str) -> Self {
        Self {
            samples: Vec::with_capacity(10000),
            name: name.to_string(),
        }
    }

    /// Record a latency sample (in nanoseconds)
    pub fn record(&mut self, latency_ns: u64) {
        self.samples.push(latency_ns);
    }

    /// Get percentile (0.0 to 1.0)
    fn percentile(&mut self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        self.samples.sort_unstable();
        let idx = ((self.samples.len() as f64 * p) as usize).min(self.samples.len() - 1);
        self.samples[idx]
    }

    /// Report statistics and clear
    pub fn report_and_clear(&mut self) {
        if self.samples.is_empty() {
            return;
        }

        let count = self.samples.len();
        let p50 = self.percentile(0.5);
        let p90 = self.percentile(0.9);
        let p99 = self.percentile(0.99);
        let max = *self.samples.iter().max().unwrap();

        info!(
            "📊 {} Latency: count={} | p50={:.2}ms | p90={:.2}ms | p99={:.2}ms | max={:.2}ms",
            self.name,
            count,
            p50 as f64 / 1_000_000.0,
            p90 as f64 / 1_000_000.0,
            p99 as f64 / 1_000_000.0,
            max as f64 / 1_000_000.0
        );

        if p99 > 100_000_000 {
            // > 100ms
            warn!(
                "⚠️  {} p99 latency is high: {:.2}ms - possible bottleneck!",
                self.name,
                p99 as f64 / 1_000_000.0
            );
        }

        self.samples.clear();
    }
}

/// Global latency tracker
pub struct LatencyTracker {
    pub db_enqueue: LatencyHistogram,    // created_ns → enqueued_ns
    pub db_flush: LatencyHistogram,      // enqueued_ns → flushed_ns
    pub udp_enqueue: LatencyHistogram,   // created_ns → udp_enqueued_ns
    pub udp_flush: LatencyHistogram,     // udp_enqueued_ns → flushed_ns
    pub end_to_end: LatencyHistogram,    // created_ns → processing done
}

impl LatencyTracker {
    pub fn new() -> Self {
        Self {
            db_enqueue: LatencyHistogram::new("DB_Enqueue"),
            db_flush: LatencyHistogram::new("DB_Flush"),
            udp_enqueue: LatencyHistogram::new("UDP_Enqueue"),
            udp_flush: LatencyHistogram::new("UDP_Flush"),
            end_to_end: LatencyHistogram::new("End-to-End"),
        }
    }

    pub fn report_all(&mut self) {
        self.db_enqueue.report_and_clear();
        self.db_flush.report_and_clear();
        self.udp_enqueue.report_and_clear();
        self.udp_flush.report_and_clear();
        self.end_to_end.report_and_clear();
    }
}

/// Spawn latency reporter task
pub fn spawn_latency_reporter(tracker: std::sync::Arc<std::sync::Mutex<LatencyTracker>>) {
    tokio::spawn(async move {
        let mut timer = interval(Duration::from_secs(5));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            timer.tick().await;
            let mut tracker_guard = tracker.lock().unwrap();
            tracker_guard.report_all();
        }
    });
    info!("📊 Latency reporter spawned (reporting every 5s)");
}
