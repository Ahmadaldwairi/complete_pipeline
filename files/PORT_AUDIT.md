# 🔌 UDP Port Configuration Audit

## Correct Port Mapping

### Brain Service

- **Listens on 45100**: Advice from collectors (mempool-watcher, wallet-tracker, launch-tracker) ✅
- **Sends to 45110**: TradeDecisions to executor ✅
- **Listens on 45115**: ExecutionConfirmations from executor ✅

### Executor Service

- **Listens on 45110**: TradeDecisions from brain ✅
- **Sends to 45115**: ExecutionConfirmations to brain ✅
- **Listens on 45130**: Hot signals from mempool-watcher (for urgent frontrunning) ✅

### Mempool-Watcher Service

- **Should send to 45100**: Heat/volume data to brain ❌ **CURRENTLY SENDING TO 45120**
- **Sends to 45130**: Hot signals to executor (urgent frontrunning opportunities) ✅

## The Problem

**Mempool-watcher .env.example shows:**

```bash
BRAIN_UDP_PORT=45120  # ❌ WRONG - Brain listens on 45100
EXECUTOR_UDP_PORT=45130  # ✅ CORRECT
```

**This should be:**

```bash
BRAIN_UDP_PORT=45100  # Brain's advice bus
EXECUTOR_UDP_PORT=45130  # Executor's mempool bus
```

## Data Flow Architecture

### Volume/Heat Data (for exit decisions):

```
Mempool-Watcher (45120) ❌ → Brain (45100)
                 WRONG PORT   LISTENING

Should be:
Mempool-Watcher → Port 45100 → Brain
Brain updates MintFeatures.mempool_pending_buys
Position tracker checks: if mempool_pending_buys == 0 && elapsed > 15s → EXIT
```

### Hot Signals (for urgent frontrunning):

```
Mempool-Watcher → Port 45130 → Executor ✅ CORRECT
Executor processes immediately for time-sensitive opportunities
```

## Impact of Wrong Port

**Current State:**

- Mempool-watcher sends volume data to port 45120
- Nothing listens on port 45120
- Brain never receives mempool data
- `MintFeatures.mempool_pending_buys` always stays 0
- Position tracker ALWAYS sees "no mempool activity"
- Bot exits positions after 15s even if volume is high ❌

**After Fix:**

- Mempool-watcher sends to port 45100
- Brain receives and updates MintFeatures
- Position tracker sees real mempool activity
- Bot holds positions when volume is present ✅
- Bot exits quickly when volume dies ✅

## Required Changes

### 1. Mempool-Watcher .env.example

```bash
# Change from:
BRAIN_UDP_PORT=45120

# To:
BRAIN_UDP_PORT=45100
```

### 2. Mempool-Watcher .env (if exists)

User needs to update their local .env file with correct port.

### 3. Brain Message Handling

Currently brain has TODO placeholders for mempool data:

```rust
// brain/src/feature_cache/mint_cache.rs line 317
mempool_pending_buys: 0,  // TODO: Populate from mempool watcher
```

Need to:

- Add mempool message parsing in brain's advice bus receiver
- Update MintFeatures when mempool data arrives
- Cache mempool data per mint address

## Port Summary Table

| Service  | Port  | Direction | Purpose                     | Status                         |
| -------- | ----- | --------- | --------------------------- | ------------------------------ |
| Brain    | 45100 | LISTEN    | Advice from collectors      | ✅ Correct                     |
| Brain    | 45110 | SEND      | Decisions to executor       | ✅ Correct                     |
| Brain    | 45115 | LISTEN    | Confirmations from executor | ✅ Correct                     |
| Executor | 45110 | LISTEN    | Decisions from brain        | ✅ Correct                     |
| Executor | 45115 | SEND      | Confirmations to brain      | ✅ Correct                     |
| Executor | 45130 | LISTEN    | Hot signals from mempool    | ✅ Correct                     |
| Mempool  | 45120 | SEND      | Heat to brain               | ❌ **WRONG - Should be 45100** |
| Mempool  | 45130 | SEND      | Hot signals to executor     | ✅ Correct                     |
