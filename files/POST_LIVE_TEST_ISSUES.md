# 🚨 Post-Live Test Issues - October 29, 2025

## Issue 1: SELL Transaction Failures & Position Blocking 🔴 CRITICAL

### Problem

- Second SELL transaction shows "Failed app interaction" on pump.fun
- Brain keeps sending EXIT signals for failed position
- With `MAX_CONCURRENT_POSITIONS=1`, the slot stays occupied
- Bot cannot process new entries until restart

### Root Cause Analysis

**Brain Behavior (CORRECT)**:

```rust
// Line 608 in brain/src/main.rs
if confirmation.is_sell() {
    info!("🔄 SELL failed - position remains tracked for retry");
}
```

- Brain correctly KEEPS position tracked when SELL fails
- This is intentional for retry logic
- Position monitor will keep generating EXIT signals every 2s

**The Real Problem**:

1. SELL transaction fails on-chain (pump.fun rejects it)
2. Executor sends failure confirmation to brain
3. Brain keeps position tracked (correct for retry)
4. Brain generates new EXIT signal
5. Executor receives EXIT signal again
6. With MAX_POSITIONS=1, this position occupies the only slot
7. **No new BUY decisions can be processed**

### Why SELL Might Fail

Common reasons for "Failed app interaction":

1. **Slippage too tight** - Price moved beyond tolerance
2. **Insufficient tokens** - Token amount calculation error
3. **Bonding curve state changed** - Race condition
4. **Priority fee too low** - Transaction dropped
5. **Invalid instruction** - Encoding error

### Solution Strategy

**Option A: Remove Failed SELL Positions After N Retries**

```rust
// Track retry count in ActivePosition
pub struct ActivePosition {
    // ... existing fields
    sell_retry_count: u8,  // NEW
}

// In brain confirmation handler
if confirmation.is_sell() && !confirmation.success {
    if let Some(pos) = position_tracker.get_mut(&mint) {
        pos.sell_retry_count += 1;
        if pos.sell_retry_count >= 3 {
            warn!("❌ SELL failed 3 times, removing position: {}", mint);
            position_tracker.remove_position(&mint);
            guardrails.remove_confirmed_position(&mint_arr);
        }
    }
}
```

**Option B: Emergency Position Clear Command**
Add manual override to force-remove stuck positions without restart.

**Option C: Increase Slippage for Retries**

```rust
// In executor, track failed SELL attempts
if sell_failed && retry_count > 0 {
    let retry_slippage = base_slippage + (retry_count * 500); // +5% per retry
    info!("🔄 SELL retry #{}, widening slippage to {}bps", retry_count, retry_slippage);
}
```

### Recommended Fix

**Combination of A + C**:

1. Increase slippage by 5% on each SELL retry (max 3 retries)
2. After 3 failed retries, force-remove position from brain tracker
3. Log full failure details for debugging

---

## Issue 2: Slippage Shows "N/A" in Telegram Notifications

### Problem

Telegram SELL notification shows:

```
Slippage: N/A
```

### Root Cause

```rust
// execution/src/trading.rs line 996
Ok(ExitResult {
    // ...
    slippage_bps: None,  // Will be calculated after tx confirmation
})
```

The `calculate_sell_slippage()` function exists (line 1858) but **is never called**!

### Solution

Call `calculate_sell_slippage()` after successful SELL execution:

```rust
// In execution/src/main.rs after line 274
Ok(exit_result) => {
    // Calculate actual slippage from transaction
    let expected_sol = buy_result.token_amount * exit_result.exit_price;
    if let Err(e) = trading_clone.calculate_sell_slippage(
        &mut exit_result,
        expected_sol
    ).await {
        warn!("⚠️  Could not calculate slippage: {}", e);
    }

    info!("✅ SELL executed successfully!");
    // ... rest of code
}
```

This will:

1. Parse actual SOL received from transaction
2. Calculate slippage percentage
3. Update `exit_result.slippage_bps` with real value
4. Telegram will show actual slippage instead of "N/A"

---

## Issue 3: Mempool-Watcher Not Broadcasting Data

### Problem

Mempool-watcher only logs:

```
Updated alpha wallets
```

No volume data, no pending buys/sells, no activity signals.

### Investigation Needed

1. **Check mempool-watcher output ports**:

   - Should send to Brain on port 45100 (advice messages)
   - Should send to Executor on port 45130 (hot signals)

2. **Check if it's monitoring pump.fun transactions**:

   ```bash
   # Check mempool-watcher logs for:
   - "Detected pump.fun transaction"
   - "Broadcasting volume update"
   - "Pending buys: X"
   ```

3. **Verify UDP sending**:
   ```rust
   // mempool-watcher should have code like:
   udp_socket.send_to(&volume_message, "127.0.0.1:45100").await?;
   ```

### Likely Root Causes

- Mempool-watcher not subscribed to pump.fun program
- UDP publisher not initialized
- Volume threshold too high (only sends on significant activity)
- Not monitoring the correct Solana RPC endpoints

### Quick Test

```bash
# Check if mempool-watcher is sending UDP packets
netstat -tulpn | grep -E "45100|45130"
# Should show mempool-watcher process

# Check network activity
tcpdump -i lo -n port 45100 or port 45130
# Should see UDP packets if watcher is broadcasting
```

---

## Issue 4: Failed Transaction Root Cause Unknown

### Need to Investigate

Without seeing the actual failed transaction signature, can't determine:

1. Was it rejected by RPC?
2. Did it land on-chain but fail?
3. Was slippage exceeded?
4. Was token amount calculation wrong?

### Debug Steps

1. **Check executor logs for the failed SELL**:

   - Look for signature
   - Check error message
   - Review slippage used

2. **Inspect on-chain transaction**:

   ```bash
   solana confirm <signature> -v
   ```

3. **Common Issues**:
   - Slippage: Using 10% base + volatility buffer, might not be enough
   - Token amount: Double-check `buy_result.token_amount` is correct
   - Timing: Price moved significantly between decision and execution

---

## Priority Order

1. **CRITICAL**: Fix slippage calculation call (Issue #2) - Easy fix, improves debugging
2. **HIGH**: Add SELL retry with increasing slippage (Issue #1) - Prevents position blocking
3. **HIGH**: Add forced position removal after 3 failed SELLs (Issue #1) - Prevents bot freeze
4. **MEDIUM**: Investigate mempool-watcher broadcasting (Issue #3) - Needed for mempool exit logic
5. **LOW**: Debug specific transaction failure (Issue #4) - Need actual tx signature

---

## Quick Wins (Can implement immediately)

### 1. Fix Slippage Calculation (5 minutes)

Add one function call in executor after SELL success.

### 2. Emergency Position Clear (10 minutes)

Add to brain:

```rust
// Check for positions stuck >10 minutes
if position.entry_time.elapsed() > Duration::from_secs(600) {
    warn!("⏰ Position stuck for 10+ minutes, force removing: {}", mint);
    remove_position(&mint);
}
```

### 3. Increase Default SELL Slippage (2 minutes)

Change from 10% to 15% base in executor config.

---

## Testing Plan After Fixes

1. ✅ Enter position (BUY)
2. ✅ Verify Telegram shows entry with all details
3. ✅ Exit position (SELL)
4. ✅ Verify Telegram shows exit with **actual slippage** (not N/A)
5. ✅ If SELL fails, verify retry with wider slippage
6. ✅ If SELL fails 3x, verify position auto-removed
7. ✅ Verify new BUY can process after failed SELL
8. ✅ Check mempool-watcher sends activity data
