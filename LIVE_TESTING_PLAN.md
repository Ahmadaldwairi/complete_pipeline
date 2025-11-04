# Live Testing Plan - 3-Tool Architecture

## ✅ Tasks Complete (1-9)

- Brain has Yellowstone gRPC monitoring
- Brain has Telegram notifications
- Executor simplified to stateless worker
- UDP port architecture documented

## 🎯 Goal: Verify Auto-Exit Works

The original issue was: "Bot entered $6 profit trade but never auto-exited, had to manual sell after 1+ min"

**Root cause**: Brain had stale data (only UDP signals, which filtered out IN_POSITION tokens)

**Solution**: Brain now has direct gRPC monitoring → real-time prices → exit conditions trigger immediately

## Pre-Test Checklist

### 1. Compilation

```bash
# Brain
cd brain && cargo build --release

# Executor
cd execution && cargo build --release

# Data-Mining
cd data-mining && cargo build --release
```

### 2. Configuration

- [ ] Yellowstone gRPC endpoint configured in brain config
- [ ] RPC endpoint set correctly
- [ ] Telegram bot token set (for notifications)
- [ ] Wallet with test SOL (~0.5 SOL for testing)

### 3. Architecture Verification

```
data-mining → UDP 45100 → Brain
Brain → UDP 45110 → Executor
Executor → UDP 45115 → Brain
Brain → Yellowstone gRPC → Real-time bonding curve updates
```

## Test Procedure

### Start Sequence

```bash
# Terminal 1: Data-Mining
cd data-mining
RUST_LOG=info cargo run --release

# Terminal 2: Brain
cd brain
RUST_LOG=info cargo run --release

# Terminal 3: Executor
cd execution
RUST_LOG=info cargo run --release
```

### What to Watch For

#### 1. Data-Mining Logs

```
✅ Connected to Yellowstone gRPC
📡 NEW_TOKEN detected: <mint>
📤 Sent signal to Brain (UDP 45100)
```

#### 2. Brain Logs

```
✅ Yellowstone gRPC connected
✅ UDP receiver bound to 45100
✅ UDP sender ready for 45110
✅ Confirmation receiver bound to 45115
✅ Telegram client initialized

📨 Received NEW_TOKEN signal
💭 Evaluating: <mint>
🟢 BUY DECISION: <mint> | size: 0.01 SOL
📤 Sent TradeDecision to Executor

✅ ExecutionConfirmation received: BUY <mint>
🔔 Telegram: "🟢 BUY EXECUTED..."

📊 gRPC update: <mint> bonding curve
💰 Price update: <old> → <new> SOL/token
📈 P&L: +$X.XX (+X%)

🚨 EXIT CONDITION MET: Target profit reached
🔴 SELL DECISION: <mint>
📤 Sent TradeDecision to Executor

✅ ExecutionConfirmation received: SELL <mint>
🔔 Telegram: "🔴 SELL EXECUTED..."
✅ Position closed
```

#### 3. Executor Logs

```
✅ Listening for TradeDecisions on port 45110

📨 Received BUY decision: <mint>
🔨 Building transaction...
📡 Transaction sent: <signature>
✅ Sent ExecutionConfirmation to Brain

📨 Received SELL decision: <mint>
🔨 Building transaction...
📡 Transaction sent: <signature>
✅ Sent ExecutionConfirmation to Brain
```

## Success Criteria

### Must Have ✅

1. **BUY executes** when data-mining detects new token
2. **Brain receives gRPC updates** for bonding curve (every ~400ms)
3. **Exit condition triggers** when price moves
4. **SELL executes automatically** (no manual intervention)
5. **Telegram notifications** sent for BUY and SELL
6. **Total time** from BUY → price update → SELL < 5 seconds

### Nice to Have 🎁

1. Multiple positions handled simultaneously
2. No duplicate trades (deduplication working)
3. Clean error handling (tx failures don't crash)
4. Performance metrics in logs

## Test Scenarios

### Scenario 1: Quick Profit Exit

```
1. New token detected
2. Brain buys 0.01 SOL
3. Price pumps +20% within 2 seconds
4. Brain auto-sells at profit
Expected: Auto-exit within 3-5s of BUY
```

### Scenario 2: Stop-Loss Exit

```
1. New token detected
2. Brain buys 0.01 SOL
3. Price dumps -10% within 2 seconds
4. Brain auto-sells at loss
Expected: Auto-exit triggers stop-loss
```

### Scenario 3: No Exit Conditions Met

```
1. New token detected
2. Brain buys 0.01 SOL
3. Price stays flat (±2%)
4. No exit triggered
Expected: Brain holds position, continues monitoring
```

## Troubleshooting

### Problem: Brain not receiving gRPC updates

**Check:**

- Brain logs show "✅ Yellowstone gRPC connected"
- Network connectivity to gRPC endpoint
- Subscriptions created for bonding curve accounts

**Fix:**

- Verify gRPC endpoint in config
- Check firewall/network rules
- Restart Brain

### Problem: Exit conditions not triggering

**Check:**

- Brain logs show "📊 gRPC update" messages
- Price updates being applied to mint_cache
- Exit condition logic in decision_engine

**Debug:**

```rust
// Add debug logs in brain/src/main.rs gRPC handler
info!("💰 Price update: {} | old: {:.10} | new: {:.10} | pnl: {:.2}%",
      mint_str, old_price, new_price, pnl_percent);
```

### Problem: Executor not receiving decisions

**Check:**

- Executor logs show "✅ Listening on port 45110"
- Brain logs show "📤 Sent TradeDecision"
- Firewall not blocking UDP 45110

**Fix:**

- Check port binding (ensure no other process using 45110)
- Test UDP connectivity: `nc -u 127.0.0.1 45110`

### Problem: No Telegram notifications

**Check:**

- Brain logs show "✅ Telegram client initialized"
- TELEGRAM_BOT_TOKEN set in environment
- Bot has permissions to send messages

**Fix:**

- Verify bot token with BotFather
- Check network connectivity to api.telegram.org

## Post-Test Analysis

### Metrics to Collect

1. **Latency**:

   - BUY decision → tx sent: < 200ms
   - gRPC update received → exit decision: < 100ms
   - SELL decision → tx sent: < 200ms

2. **Reliability**:

   - % of positions that auto-exited (target: 100%)
   - % of Telegram notifications sent (target: 100%)
   - % of duplicate trades (target: 0%)

3. **Profitability**:
   - Average hold time
   - Average P&L per trade
   - Win rate vs previous architecture

### Success Definition

**✅ PASS** if all 3 criteria met:

1. At least 3/3 test trades auto-exited without manual intervention
2. Brain received gRPC updates < 500ms after price changes
3. No crashes or critical errors in any component

**❌ FAIL** if any:

1. Manual sell required (original issue persists)
2. Brain not receiving gRPC updates
3. Crashes or unrecoverable errors

## Next Steps After Testing

### If PASS ✅

- Proceed to Task #11: Remove mempool-watcher
- Proceed to Task #12: Add position lifecycle logging
- Production deployment preparation

### If FAIL ❌

- Analyze logs to identify root cause
- Add debug logging to problematic component
- Fix issues and retest
- Do NOT proceed until PASS

## Emergency Stop

If testing causes issues:

```bash
# Stop all processes
pkill -f "target/release/decision_engine"
pkill -f "target/release/execution-bot"
pkill -f "target/release/data-mining"

# Check for stuck positions
# Manual sell if needed via Pump.fun UI
```

## Files to Monitor

- `brain/data/brain_decisions.csv` - Decision history
- `execution/data/trades.db` - Execution history
- Brain logs - gRPC updates, exit conditions
- Executor logs - Transaction confirmations
- Telegram - User-facing notifications
