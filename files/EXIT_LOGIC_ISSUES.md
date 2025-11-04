# Exit Logic Critical Issues & Solutions

**Date:** October 29, 2025  
**Status:** CRITICAL - Bot is not exiting profitable trades

---

## 🚨 Critical Issues Identified

### Issue #1: Percentage-Based Exits Instead of Absolute Dollar Profit

**Problem:**

- Exit logic checks for **30%, 60%, 100% gains** to trigger sells
- User's goal: **$1 absolute profit** after fees
- Result: Positions with **$3+ profit never exited** because they didn't reach 30% gain

**Current Code:**

```rust
// brain/src/decision_engine/position_tracker.rs:67-78
if price_change_pct >= self.profit_targets.2 {  // 100%
    return Some(ExitReason::ProfitTarget { ... });
}
if price_change_pct >= self.profit_targets.1 {  // 60%
    return Some(ExitReason::ProfitTarget { ... });
}
if price_change_pct >= self.profit_targets.0 {  // 30%
    return Some(ExitReason::ProfitTarget { ... });
}
```

**Evidence from Logs:**

- Position `14cGHozF`: Entry $3.68, held 301s, exited at -10.92% = **-$0.41 loss**
- Position `CK1RXpsM`: Entry $4.76, never exited (still holding)
- Position `2ygTsF2D`: Entry $3.09, never exited (still holding)
- User reported: "All of them were profitable and one was +$3 but never received sell!"

**Solution:**
Replace percentage-based logic with absolute dollar profit targeting:

```rust
let current_value_usd = self.tokens * current_price_sol * sol_price_usd;
let realized_profit = current_value_usd - self.size_usd;

if realized_profit >= 1.0 {  // $1 profit target
    return Some(ExitReason::ProfitTarget { ... });
}
```

---

### Issue #2: No Mempool Volume Monitoring

**Problem:**

- Mempool watcher running but **ZERO mempool signals received**
- Brain logs show: `Listening for Hot Signals from Mempool on port 45130` - but no data
- Exit logic checks `vol_5s_sol < 0.5` which only uses on-chain data, not mempool
- User requirement: "Bot will only stay ONLY if mempool watcher sends advice that token is receiving volume"

**Evidence from Logs:**

- Brain: No mempool messages in entire session (6 minutes of logs)
- Executor: `Listening for Hot Signals from Mempool on port 45130` - no activity
- Volume drop check exists but never triggers because mempool data missing

**Current Volume Check (Inadequate):**

```rust
// Lines 103-110
if current_features.vol_5s_sol < 0.5 && price_change_pct < 10.0 {
    if elapsed > 30 {
        return Some(ExitReason::VolumeDrop { ... });
    }
}
```

**Solution:**

1. Verify mempool-watcher is running and sending to port 45120
2. Add mempool pending transactions to MintFeatures
3. Exit immediately if no mempool activity for 10-15 seconds:

```rust
// Exit if no mempool activity
if current_features.mempool_pending_txs == 0 && elapsed > 15 {
    return Some(ExitReason::NoMempoolActivity { ... });
}
```

---

### Issue #3: Time Decay Exits Regardless of Profit

**Problem:**

- Current logic: Exit **ALL** positions after 300 seconds (5 minutes)
- Exits happen even if position is profitable
- User wants: Exit only if unprofitable OR no mempool volume

**Evidence:**

```
[23:33:59] 🚨 EXIT SIGNAL: 14cGHozF | reason: TIME_DECAY (300s, -0.1%)
[23:34:01] Net profit: $-0.41
```

**Current Code:**

```rust
// Lines 96-102
if elapsed >= self.max_hold_secs {
    return Some(ExitReason::TimeDecay {
        elapsed_secs: elapsed,
        pnl_pct: price_change_pct,
        exit_percent: 100, // Exit ALL regardless of profit
    });
}
```

**Solution:**

- Keep time limit as **safety backstop**
- But prioritize profit target and mempool volume checks
- If profitable with volume, hold beyond 5 minutes
- Time decay should be last resort, not first exit trigger

---

### Issue #4: Fake Transaction Signatures

**Problem:**

- Executor sends dummy signatures: `tx: 111111111111`
- Brain cannot verify actual on-chain status
- Cannot calculate real PnL from blockchain
- Cannot detect failed transactions

**Evidence from Logs:**

```
[23:28:59] ✅ BUY CONFIRMED: 14cGHozFa5Ft | tx: 111111111111
[23:30:21] ✅ BUY CONFIRMED: CK1RXpsM4x63 | tx: 111111111111
```

But executor shows real signatures:

```
Signature: 5eYWcJUkHFXspPktfrvcjSaJDrvAj5RA9rG7ABts3WkFCgb6BFkCsayJAmns9BpoSjU5HZfVfUVYQ6k8eiYHjR5C
```

**Root Cause:**
Executor's ExecutionConfirmation message uses placeholder signature bytes instead of real transaction signature.

**Solution:**
Update executor to send real signature bytes in confirmation message:

```rust
// execution/src/main.rs (BUY confirmation)
let tx_sig_bytes: [u8; 32] = bs58::decode(&result.signature)
    .into_vec()?
    .try_into()?;

let confirmation = ExecutionConfirmation::new_success(
    decision.mint,
    0, // BUY
    decision.size_lamports,
    result.price,
    tx_sig_bytes,  // ✅ Use real signature
);
```

---

### Issue #5: Stale Blockhash Cache (Performance)

**Problem:**

- Blockhash warm-up task running but cache going stale
- Logs show: `Blockhash cache is stale (age: 1805ms, 4313ms, 9156ms)`
- Warm-up runs every 300ms but something blocking it

**Evidence:**

```
[23:28:59] ⚠️  Blockhash cache is stale (age: 1805ms)
[23:30:21] ⚠️  Blockhash cache is stale (age: 4313ms)
[23:33:59] ⚠️  Blockhash cache is stale (age: 9156ms)
```

**Impact:** Potential transaction failures if blockhash expires

**Solution:**

- Investigate blockhash warm-up task
- May need to increase refresh rate or check for blocking operations
- Consider using RwLock instead of Mutex for better concurrency

---

## 🎯 User Requirements Summary

From user's description:

1. **Exit Goal:** Take **$1 or more** in realized profit after fees
2. **Hold Condition:** Only stay in position if mempool shows volume/pending txs
3. **Exit Triggers:**

   - ✅ $1+ profit reached
   - ✅ No mempool activity for 10-20 seconds
   - ✅ Loss exceeding threshold
   - ✅ Time limit (5 min) as safety backstop

4. **Manual Closes:** User had to manually close 2 positions in loss because bot never sent exit signal

---

## 📋 Solution Implementation Plan

### Phase 1: Fix Exit Logic (CRITICAL)

1. ✅ Change profit targets from percentage to absolute dollars ($1 threshold)
2. ✅ Add mempool volume/pending tx checks
3. ✅ Prioritize mempool activity over time decay
4. ✅ Keep time decay as safety backstop only

### Phase 2: Fix Mempool Integration

1. ✅ Verify mempool-watcher is running
2. ✅ Check UDP port 45120 (brain ← mempool-watcher)
3. ✅ Add mempool fields to MintFeatures struct
4. ✅ Use mempool data in exit decisions

### Phase 3: Fix Transaction Signatures

1. ✅ Update ExecutionConfirmation to include real tx signatures
2. ✅ Brain logs real signatures for verification
3. ✅ Add on-chain confirmation checks (optional enhancement)

### Phase 4: Performance Optimization

1. ✅ Fix blockhash cache staleness
2. ✅ Optimize warm-up task
3. ✅ Monitor cache hit rates

---

## 🔧 Technical Changes Required

### File: `brain/src/decision_engine/position_tracker.rs`

**Change 1: Add absolute profit check (Line ~67)**

```rust
// NEW: Check absolute dollar profit FIRST
let current_value_usd = self.tokens * current_price_sol * sol_price_usd;
let realized_profit = current_value_usd - self.size_usd;

if realized_profit >= 1.0 {
    return Some(ExitReason::ProfitTarget {
        tier: 1,
        pnl_pct: price_change_pct,
        exit_percent: 100,
    });
}
```

**Change 2: Add mempool activity check (Line ~103)**

```rust
// NEW: Exit if no mempool activity (no buying pressure)
if current_features.mempool_pending_buys == 0 && elapsed > 15 {
    return Some(ExitReason::NoMempoolActivity {
        elapsed_secs: elapsed,
        pnl_pct: price_change_pct,
        exit_percent: 100,
    });
}
```

**Change 3: Update time decay to be last resort (Line ~96)**

```rust
// Time decay now only triggers if:
// - Not profitable enough ($1+)
// - No mempool activity
// - Exceeded max hold time
if elapsed >= self.max_hold_secs {
    // Already checked profit and mempool above
    return Some(ExitReason::TimeDecay {
        elapsed_secs: elapsed,
        pnl_pct: price_change_pct,
        exit_percent: 100,
    });
}
```

### File: `brain/src/decision_engine/mod.rs`

**Change: Add NoMempoolActivity to ExitReason enum**

```rust
pub enum ExitReason {
    ProfitTarget { tier: u8, pnl_pct: f64, exit_percent: u8 },
    StopLoss { pnl_pct: f64, exit_percent: u8 },
    TimeDecay { elapsed_secs: u64, pnl_pct: f64, exit_percent: u8 },
    VolumeDrop { volume_5s: f64, pnl_pct: f64, exit_percent: u8 },
    NoMempoolActivity { elapsed_secs: u64, pnl_pct: f64, exit_percent: u8 },  // NEW
    Emergency { pnl_pct: f64, exit_percent: u8 },
}
```

### File: `brain/src/feature_cache/mint_cache.rs`

**Change: Add mempool fields to MintFeatures**

```rust
pub struct MintFeatures {
    // ... existing fields ...
    pub mempool_pending_buys: u32,     // NEW: From mempool watcher
    pub mempool_pending_sells: u32,    // NEW
    pub mempool_volume_sol: f64,       // NEW
    // ...
}
```

---

## 📊 Expected Results After Fix

### Before (Current Behavior):

- ❌ Positions with $3 profit never exit
- ❌ Time decay exits at 300s regardless of profit
- ❌ No mempool volume monitoring
- ❌ Manual intervention required to close positions

### After (Fixed Behavior):

- ✅ Exit when $1+ profit achieved (within seconds)
- ✅ Exit when no mempool activity for 15s (no buying pressure)
- ✅ Hold profitable positions with active mempool volume
- ✅ Time decay only as safety backstop
- ✅ Real transaction signatures logged

---

## 🧪 Testing Plan

1. **Test $1 Profit Exit:**

   - Enter position ~$3-5
   - Monitor until profit reaches $1.00
   - Verify immediate exit signal

2. **Test Mempool Activity:**

   - Verify mempool-watcher sending data
   - Check brain receives mempool signals
   - Test exit when no mempool activity for 15s

3. **Test Time Decay Backstop:**

   - Enter position
   - Verify does NOT exit before 5min if profitable
   - Verify DOES exit after 5min if unprofitable

4. **Test Stop Loss:**
   - Enter position
   - Verify exits if loss exceeds threshold

---

## 🚀 Deployment Steps

1. ✅ Implement code changes (phases 1-4)
2. ✅ Rebuild brain service
3. ✅ Rebuild executor service
4. ✅ Restart both services
5. ✅ Monitor logs for exit signals
6. ✅ Verify mempool data arriving
7. ✅ Test with small positions ($3-5)
8. ✅ Verify $1 profit exits work

---

## 📝 Notes

- Keep manual override capability
- Log all exit reasons for analysis
- Monitor false positives (exiting too early)
- Track realized profits in CSV
- Consider dynamic profit targets based on volatility
