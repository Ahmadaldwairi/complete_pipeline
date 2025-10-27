# 🧪 End-to-End Integration Test

Complete system integration test for the Solana Scalper Bot.

## Test Overview

This test verifies the complete data flow across all services:

```
Data Collector → Brain → Executor → Brain (telemetry)
     (UDP)        (UDP)      (UDP)
   port 45100   port 45110  port 45115
```

## Prerequisites

### 1. Build All Services

```bash
# Brain service
cd brain/
cargo build --release

# Executor service
cd ../execution/
cargo build --release

# Mempool watcher (optional)
cd ../mempool-watcher/
cargo build --release
```

### 2. Configure Services

Ensure all `.env` files are properly configured:

- `brain/.env` - Brain service configuration
- `execution/.env` - Executor configuration
- `mempool-watcher/.env` - Mempool watcher configuration

## Running the Tests

### Test 1: Port Connectivity Check

Verify all UDP ports are available:

```bash
cd integration-test/
python3 test_ports.py
```

**Expected Output:**

```
Port 45100 (Brain Advice Bus): ✅ LISTENING
Port 45110 (Brain Decision Bus): ✅ LISTENING
Port 45120 (Mempool Brain Port): ✅ LISTENING
Port 45130 (Mempool Executor Port): ✅ LISTENING
```

### Test 2: End-to-End Latency Test

Test complete flow from Collector → Brain → Executor:

```bash
# Terminal 1: Start Brain service
cd brain/
cargo run --release

# Terminal 2: Start Executor service (optional, test can simulate)
cd execution/
cargo run --release

# Terminal 3: Run E2E test
cd integration-test/
python3 test_e2e.py
```

**Expected Output:**

```
📊 INTEGRATION TEST RESULTS
✅ Successful: 10/10 (100.0%)
❌ Failed: 0/10

⏱️  LATENCY STATISTICS:
   Min:     45.23ms
   Max:     180.45ms
   Mean:    95.67ms
   Median:  88.12ms

🎯 TARGET MET: Mean latency 95.67ms < 250ms ✅
```

## Test Scenarios

### Scenario 1: Basic Flow (No Executor)

Tests Brain's ability to receive advice and generate decisions:

1. Start Brain service only
2. Run `test_e2e.py`
3. Test simulates Collector sending advice
4. Verifies Brain receives and processes messages
5. Measures Brain's response time

**Pass Criteria:**

- ✅ Brain receives 100% of advice messages
- ✅ Brain generates decisions within 250ms
- ✅ No crashes or errors

### Scenario 2: Full Flow (With Executor)

Tests complete system including execution:

1. Start Brain service
2. Start Executor service
3. Run `test_e2e.py`
4. Verifies full Collector → Brain → Executor flow
5. Checks for telemetry feedback (if implemented)

**Pass Criteria:**

- ✅ End-to-end latency < 250ms
- ✅ 100% message delivery
- ✅ Executor receives and logs decisions

### Scenario 3: Stress Test (High Load)

Tests system under load:

1. Modify `test_e2e.py` to send 100+ messages
2. Reduce delay between messages to 0.1s
3. Verify system handles burst traffic

**Pass Criteria:**

- ✅ No message loss
- ✅ Latency remains stable
- ✅ No memory leaks

## Monitoring During Tests

### Brain Service

Watch for:

- `📥 Received advice` - Incoming messages
- `🎯 DECISION:` - Decision generated
- `📤 Sent decision` - Outgoing to Executor

### Executor Service

Watch for:

- `📥 Received decision` - Decision from Brain
- `🔄 Processing BUY/SELL` - Execution logic
- `✅ Logged to database` - Persistence

## Troubleshooting

### No Services Detected

**Problem:** `test_ports.py` shows all ports as NOT LISTENING

**Solution:**

1. Start Brain service: `cd brain && cargo run --release`
2. Start Executor service: `cd execution && cargo run --release`
3. Check for errors in service startup logs

### Timeout (No Response)

**Problem:** E2E test shows "No decision received (timeout)"

**Possible Causes:**

1. Brain not running → Start Brain service
2. Wrong port configuration → Check `.env` files
3. Firewall blocking UDP → Check firewall settings
4. Brain crashed → Check Brain logs for errors

### High Latency (>250ms)

**Problem:** Mean latency exceeds target

**Possible Causes:**

1. System under load → Close other applications
2. Debug build (slow) → Use `--release` builds
3. Network issues → Test on localhost only
4. Database slow → Check PostgreSQL/SQLite performance

## Success Criteria

✅ **PASS** if all of the following are met:

1. **Port Connectivity**: All 4 UDP ports listening
2. **Message Delivery**: 100% success rate (10/10 messages)
3. **Latency Target**: Mean latency < 250ms
4. **No Crashes**: All services remain stable
5. **Logging**: Decisions logged to database (Executor)

## Performance Targets

| Metric       | Target | Acceptable | Poor   |
| ------------ | ------ | ---------- | ------ |
| Success Rate | 100%   | ≥95%       | <95%   |
| Mean Latency | <100ms | <250ms     | >250ms |
| Max Latency  | <200ms | <500ms     | >500ms |
| Memory Usage | <100MB | <200MB     | >200MB |

## Advanced Testing

### Custom Message Test

Send a custom advice message:

```python
import socket
import json

advice = {
    "type": "late_opportunity",
    "mint": "YourMintAddressHere...",
    "mint_features": {
        "volume_10s": 100000.0,
        "holders_10s": 50,
        "price_change_60s": 25.0,
    }
}

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.sendto(json.dumps(advice).encode(), ('127.0.0.1', 45100))
```

### Continuous Monitoring

Run test in loop to detect degradation:

```bash
while true; do
    python3 test_e2e.py
    sleep 10
done
```

## Next Steps After Testing

If all tests pass:

1. ✅ **Production Ready** - System is ready for live trading
2. 📊 **Monitor Metrics** - Set up Grafana dashboards
3. 🔔 **Set Alerts** - Configure Telegram alerts
4. 💰 **Start Small** - Begin with minimal position sizes
5. 📈 **Scale Up** - Gradually increase as confidence grows

## Test Reports

Test results are logged to:

- Console output (stdout)
- Service logs in each service's directory
- Database logs (Executor writes to DB)

Save test results for comparison:

```bash
python3 test_e2e.py | tee test_results_$(date +%Y%m%d_%H%M%S).log
```

---

**Status**: Integration test framework complete ✅  
**Last Updated**: October 26, 2025
