# Critical Bugs Fixed - Nov 3, 2025

## Bug 1: mempool_pending_buys Always 0 (CRITICAL)

**Symptom:** Bot stayed in trade for 2+ minutes with $6 profit, never auto-exited

**Root Cause:**

- `MintFeatures.mempool_pending_buys` was hardcoded to 0
- Brain's exit logic: "Exit if mempool_pending_buys == 0 && elapsed > 15s"
- Since it was always 0, condition `== 0` was TRUE
- BUT Brain was also checking other exit conditions FIRST (profit target, stop loss, etc.)
- The "no mempool activity" exit only triggered if:
  1. No other exit condition was met
  2. Position held for >15s
  3. mempool_pending_buys == 0

**Why It Failed:**

- Yellowstone RPC only sends CONFIRMED transactions (not pending)
- We can't detect "pending" mempool transactions
- Need to use recent buyer activity as proxy

**Fix Applied:**

- Use `buyers_2s` (unique buyers in last 2 seconds) as proxy for mempool activity
- Updated 3 locations in `brain/src/feature_cache/mint_cache.rs`:
  1. Line 168: `existing.mempool_pending_buys = buyers`
  2. Line 186: `mempool_pending_buys: buyers_2s.unwrap_or(0)`
  3. Line 377: `mempool_pending_buys: buyers_2s`

**Expected Behavior After Fix:**

- If buyers_2s > 0 → Brain sees "activity", stays in trade
- If buyers_2s == 0 for >15s → Brain exits automatically
- Log will show: `"✅ EXIT TRIGGER: No mempool activity (0 pending buys after {}s)"`

---

## Bug 2: Manual Exit Not Freeing Position Slot (NEEDS INVESTIGATION)

**Symptom:** User manually sold but couldn't enter new trades (concurrent limit = 1)

**Expected Flow:**

1. User sells manually via Phantom wallet
2. Mempool-watcher sees SELL transaction
3. Log: `"🔍 Manual SELL detected for tracked mint: {}"`
4. Log: `"🚨 MANUAL EXIT DETECTED: {} | P&L: ${:.2}"`
5. Sends ManualExitNotification to Brain port 45135
6. Brain removes position from tracker
7. Slot freed, bot can enter new trades

**Code Review:**

- ✅ Mempool-watcher has manual exit detection (main.rs:230-295)
- ✅ Brain has ManualExit listener on port 45135 (main.rs:1543-1565)
- ✅ Position tracker removal called (main.rs:1555)

**NEEDS USER LOGS:**
To diagnose, check mempool-watcher logs for:

- `"🔍 Manual SELL detected for tracked mint: {}"`
- `"🚨 MANUAL EXIT DETECTED: {} | P&L: ${:.2}"`
- `"📤 Sent ManualExitNotification to Brain"`

If these logs are MISSING:

- Mempool-watcher is not detecting the SELL transaction at all
- Possible causes:
  1. Transaction not confirmed yet when checked
  2. Mint filter not matching
  3. Instruction decoder failing

If logs are PRESENT but slot not freed:

- Brain listener may not be receiving/processing the message
- Check Brain logs for: `"💰 Processing manual exit cleanup for mint: {}"`

---

## Phantom Wallet Integration (User Question)

**Current Setup:**

- Bot uses `execution/keypair.json` wallet
- User manually trades with Phantom wallet (different address)

**Options:**

### Option A: Export Phantom Private Key (NOT RECOMMENDED)

- Export seed phrase from Phantom
- Create keypair.json from seed
- **RISK:** Exposes seed phrase to filesystem
- **RISK:** Bot has full control of main wallet

### Option B: Monitor Both Wallets (COMPLEX)

- Keep bot using execution wallet
- Add Phantom wallet address to mempool-watcher filter
- Requires tracking two separate wallets
- Manual sells from Phantom wouldn't affect bot's position tracking

### Option C: Current Setup (RECOMMENDED)

- Bot trades with execution wallet
- User monitors via Telegram notifications
- Manual sells only if needed (emergency)
- Mempool-watcher should detect manual sells and free slots

**Recommendation:** Fix Bug 2 first to ensure manual sell detection works, then test thoroughly before considering wallet changes.

---

## How to Test Fixes

### 1. Restart Services

```bash
# Kill existing
pkill -f decision_engine
pkill -f mempool-watcher

# Start Brain
cd brain && RUST_LOG=info ./target/release/decision_engine &

# Start Mempool Watcher
cd mempool-watcher && RUST_LOG=info ./target/release/mempool-watcher &
```

### 2. Watch for New Logs

**Brain position monitoring (every 2s):**

```
📊 Position Check: {mint} | ... | 📦 {N} mempool buys
```

- If N > 0: Bot sees activity, will hold
- If N = 0 for >15s: Bot should exit automatically

**On auto-exit:**

```
✅ EXIT TRIGGER: No mempool activity (0 pending buys after {elapsed}s)
✅ SELL DECISION SENT: {mint} ({size} SOL, {exit_percent}%)
```

**On manual sell:**

```
🔍 Manual SELL detected for tracked mint: {mint}
🚨 MANUAL EXIT DETECTED: {mint} | P&L: ${pnl}
📤 Sent ManualExitNotification to Brain
💰 Processing manual exit cleanup for mint: {mint}
🗑️  Position removed from tracker: {mint}
```

### 3. Test Scenarios

**Scenario 1: Auto-exit when volume dies**

1. Bot enters position
2. Trading stops (no more buyers)
3. After 15-20 seconds, should see: `"✅ EXIT TRIGGER: No mempool activity"`
4. Bot sends SELL automatically

**Scenario 2: Manual exit while holding**

1. Bot enters position
2. User manually sells via Phantom/UI
3. Should see: `"🔍 Manual SELL detected"` in mempool-watcher
4. Should see: `"💰 Processing manual exit"` in Brain
5. Position slot freed for next trade

---

## Files Modified

1. `brain/src/main.rs` - Added comment explaining buyers_2s proxy
2. `brain/src/feature_cache/mint_cache.rs` - Populate mempool_pending_buys from buyers_2s (3 locations)

## Next Steps

1. ✅ Restart Brain with new binary
2. ⏳ Monitor logs during next trade
3. ⏳ Verify auto-exit triggers after activity stops
4. ⏳ Test manual sell and verify slot freeing
5. ⏳ Collect logs if issues persist
