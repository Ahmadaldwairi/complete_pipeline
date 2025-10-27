# Metrics Integration Guide for Brain Service

## 📊 Overview

This metrics module adds Prometheus monitoring to the Brain service, tracking:

- Decision rates and outcomes
- Cache performance
- Guardrail effectiveness
- System performance (latency, throughput)
- SOL price tracking
- UDP communication stats

## 🚀 Installation

### 1. Add Dependencies to Brain's Cargo.toml

```toml
[dependencies]
# ... existing dependencies ...

# Metrics
prometheus = "0.13"
once_cell = "1.19"

# HTTP server for metrics endpoint
axum = "0.7"
```

### 2. Copy Metrics Module

```bash
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution/metrics.rs \
   /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain/src/
```

### 3. Register Module in main.rs

Add to `brain/src/main.rs`:

```rust
mod metrics;

use metrics::{
    init_metrics, start_metrics_server,
    record_decision_approved, record_decision_rejected,
    // ... other metrics functions
};
```

### 4. Initialize Metrics in main()

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // ... existing initialization ...

    // Initialize metrics system
    init_metrics();

    // Start metrics HTTP server in background
    tokio::spawn(async {
        if let Err(e) = start_metrics_server(9090).await {
            error!("Metrics server error: {}", e);
        }
    });

    // ... rest of main loop ...
}
```

## 📝 Usage Examples

### Recording Decisions

```rust
// When a decision is approved
use metrics::{record_decision_approved, record_decision_sent};

if decision_approved {
    record_decision_approved();
    send_decision_to_executor(&decision)?;
    record_decision_sent();
}
```

### Recording Rejections

```rust
use metrics::{record_decision_rejected, RejectionReason};

if confidence < min_confidence {
    record_decision_rejected(RejectionReason::LowConfidence);
    return Ok(());
}

if !guardrails.check(&decision) {
    record_decision_rejected(RejectionReason::Guardrails);
    return Ok(());
}
```

### Recording Decision Pathways

```rust
use metrics::{record_decision_pathway, DecisionPathway};

match advice_type {
    AdviceType::CopyTrade => {
        record_decision_pathway(DecisionPathway::CopyTrade);
        // ... process copytrade ...
    }
    AdviceType::NewLaunch => {
        record_decision_pathway(DecisionPathway::NewLaunch);
        // ... process new launch ...
    }
}
```

### Recording Guardrail Blocks

```rust
use metrics::{record_guardrail_block, GuardrailType};

if position_count >= max_positions {
    record_guardrail_block(GuardrailType::PositionLimit);
    return Err(anyhow!("Position limit reached"));
}

if in_loss_backoff {
    record_guardrail_block(GuardrailType::LossBackoff);
    return Err(anyhow!("Loss backoff active"));
}
```

### Timing Operations

```rust
use metrics::DecisionTimer;

// Start timer
let timer = DecisionTimer::start();

// ... do decision processing ...

// Record duration
timer.observe();  // Automatically records to histogram
```

### Cache Metrics

```rust
use metrics::{record_cache_access, CacheType};

match mint_cache.get(&mint) {
    Some(features) => {
        record_cache_access(CacheType::Mint, true);  // Hit
        // ... use features ...
    }
    None => {
        record_cache_access(CacheType::Mint, false); // Miss
        // ... fetch from database ...
    }
}
```

### Updating Gauges

```rust
use metrics::{update_sol_price, update_active_positions};

// When SOL price updates
fn handle_price_update(price: f32) {
    update_sol_price(price);
    // ... rest of handling ...
}

// When position count changes
fn update_positions(count: usize) {
    update_active_positions(count as i64);
}
```

### Recording UDP Events

```rust
use metrics::{record_advice_received, record_udp_parse_error};

// When advice message arrives
record_advice_received();

match parse_advice_message(&bytes) {
    Ok(advice) => {
        // ... process advice ...
    }
    Err(e) => {
        record_udp_parse_error();
        warn!("Failed to parse advice: {}", e);
    }
}
```

## 🔍 Accessing Metrics

### Metrics Endpoint

```bash
# View all metrics
curl http://localhost:9090/metrics

# Check health
curl http://localhost:9090/health
```

### Sample Output

```
# HELP brain_decisions_total Total number of trading decisions made
# TYPE brain_decisions_total counter
brain_decisions_total 142

# HELP brain_decisions_approved Number of approved trading decisions
# TYPE brain_decisions_approved counter
brain_decisions_approved 98

# HELP brain_decisions_rejected Number of rejected trading decisions
# TYPE brain_decisions_rejected counter
brain_decisions_rejected 44

# HELP brain_decision_latency_seconds Decision processing latency
# TYPE brain_decision_latency_seconds histogram
brain_decision_latency_seconds_bucket{le="0.001"} 12
brain_decision_latency_seconds_bucket{le="0.005"} 45
brain_decision_latency_seconds_bucket{le="0.01"} 87
brain_decision_latency_seconds_bucket{le="+Inf"} 142
brain_decision_latency_seconds_sum 0.8234
brain_decision_latency_seconds_count 142

# HELP brain_sol_price_usd Current SOL price in USD
# TYPE brain_sol_price_usd gauge
brain_sol_price_usd 194.21

# HELP brain_mint_cache_hits Mint cache hits
# TYPE brain_mint_cache_hits counter
brain_mint_cache_hits 523

# HELP brain_mint_cache_misses Mint cache misses
# TYPE brain_mint_cache_misses counter
brain_mint_cache_misses 89
```

## 📊 Grafana Dashboard

### Prometheus Configuration

Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: "brain"
    static_configs:
      - targets: ["localhost:9090"]
    scrape_interval: 5s
```

### Key Metrics to Monitor

**Decision Rate:**

```promql
rate(brain_decisions_total[1m])
```

**Approval Rate:**

```promql
rate(brain_decisions_approved[1m]) / rate(brain_decisions_total[1m])
```

**Cache Hit Rate:**

```promql
brain_mint_cache_hits / (brain_mint_cache_hits + brain_mint_cache_misses)
```

**P95 Decision Latency:**

```promql
histogram_quantile(0.95, brain_decision_latency_seconds_bucket)
```

**Active Positions:**

```promql
brain_active_positions
```

**Guardrail Block Rate:**

```promql
rate(brain_rejected_guardrails[5m])
```

## 🧪 Testing

Run the test script to verify metrics integration:

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution
python3 test_metrics.py
```

This will:

1. Check if metrics endpoint is accessible
2. Verify all expected metrics are present
3. Test metric updates
4. Validate metric types and values

## 📈 Monitoring Best Practices

### Alerts to Set Up

1. **High Rejection Rate:**

   ```promql
   rate(brain_decisions_rejected[5m]) / rate(brain_decisions_total[5m]) > 0.8
   ```

2. **High Latency:**

   ```promql
   histogram_quantile(0.95, brain_decision_latency_seconds_bucket) > 0.1
   ```

3. **Low Cache Hit Rate:**

   ```promql
   brain_mint_cache_hits / (brain_mint_cache_hits + brain_mint_cache_misses) < 0.7
   ```

4. **Position Limit Near Max:**

   ```promql
   brain_active_positions >= 2  # if max is 3
   ```

5. **Frequent Guardrail Blocks:**
   ```promql
   rate(brain_rejected_guardrails[5m]) > 1
   ```

### Dashboard Panels

1. **Decision Rate** (graph over time)
2. **Approval vs Rejection** (pie chart)
3. **Decision Latency** (heatmap)
4. **Cache Performance** (hit rate gauge)
5. **Active Positions** (gauge)
6. **Guardrail Blocks** (stacked graph by type)
7. **SOL Price** (line graph)
8. **UDP Stats** (sent vs received)

## 🔧 Troubleshooting

### Metrics endpoint not accessible

```bash
# Check if metrics server started
curl http://localhost:9090/health

# Check Brain logs
tail -f /tmp/brain.log | grep metrics

# Verify port is not in use
lsof -i :9090
```

### Metrics not updating

```bash
# Verify metric recording calls are present
grep -r "record_decision" brain/src/

# Check for errors in metrics module
# Add debug logging to metrics.rs
```

### High memory usage

```bash
# Prometheus metrics use memory
# Consider reducing histogram bucket counts
# Or increase scrape interval
```

## 📝 Next Steps

1. Set up Prometheus server
2. Configure Grafana dashboards
3. Set up alerting rules
4. Monitor metrics in production
5. Tune based on observed patterns

## 🎯 Metrics Checklist

- [ ] Dependencies added to Cargo.toml
- [ ] metrics.rs copied to brain/src/
- [ ] Module registered in main.rs
- [ ] Metrics initialized at startup
- [ ] HTTP server started
- [ ] Decision metrics integrated
- [ ] Cache metrics integrated
- [ ] Guardrail metrics integrated
- [ ] Timing metrics integrated
- [ ] UDP metrics integrated
- [ ] Prometheus configured
- [ ] Grafana dashboard created
- [ ] Alerts configured

---

**Last Updated:** October 25, 2025  
**Module Version:** 1.0.0  
**Status:** Ready for integration
