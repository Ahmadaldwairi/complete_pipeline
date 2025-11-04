# UDP Port Architecture (3-Tool System)

## Active Ports ✅

| Port      | Direction           | Purpose                                       | Listener | Status    |
| --------- | ------------------- | --------------------------------------------- | -------- | --------- |
| **45100** | data-mining → Brain | Token signals (NEW_TOKEN, VOLUME_SURGE, etc.) | Brain    | ✅ Active |
| **45110** | Brain → Executor    | TradeDecision (BUY/SELL commands)             | Executor | ✅ Active |
| **45115** | Executor → Brain    | ExecutionConfirmation (tx results)            | Brain    | ✅ Active |
| **45120** | data-mining → Brain | WindowMetrics (optional analytics)            | Brain    | ✅ Active |

## Deprecated Ports (To Remove) ❌

| Port      | Old Purpose                | Why Deprecated            |
| --------- | -------------------------- | ------------------------- | ------------------------------------ |
| **45130** | mempool-watcher → Brain    | Mempool intelligence      | Replaced by Brain gRPC monitoring    |
| **45131** | mempool-watcher → Brain    | Hot signals               | Replaced by Brain gRPC monitoring    |
| **45132** | mempool-watcher → Executor | TxConfirmed notifications | Brain handles confirmations via gRPC |
| **45134** | mempool-watcher → Executor | ManualExit notifications  | Brain handles manual exits           |
| **45135** | mempool-watcher → Brain    | ManualExit notifications  | Brain handles manual exits           |

## Architecture Overview

```
┌─────────────────┐
│  Data-Mining    │
│  (Yellowstone   │
│   gRPC Client)  │
└────────┬────────┘
         │ UDP 45100: Token signals
         │ UDP 45120: Window metrics
         ▼
┌─────────────────┐
│     Brain       │
│  (Yellowstone   │
│   gRPC Client)  │ ◄─── Direct gRPC monitoring of bonding curves
│  - Decision     │      (replaces mempool-watcher completely)
│    Engine       │
│  - Mint Cache   │
│  - Position     │
│    Tracking     │
│  - Telegram     │
└────────┬────────┘
         │ UDP 45110: TradeDecision
         ▼
┌─────────────────┐
│    Executor     │
│  (Stateless     │
│   TX Builder)   │
└────────┬────────┘
         │ UDP 45115: ExecutionConfirmation
         └──────────┘ (loops back to Brain)
```

## Message Flow

### 1. Entry (BUY)

```
1. data-mining detects NEW_TOKEN
   └─[UDP 45100]→ Brain receives signal

2. Brain decision_engine evaluates
   └─[UDP 45110]→ Executor receives TradeDecision(BUY)

3. Executor builds & sends transaction
   └─[UDP 45115]→ Brain receives ExecutionConfirmation

4. Brain gRPC monitors bonding curve for price updates
   └─ Real-time price → mint_cache → exit conditions
```

### 2. Exit (SELL)

```
1. Brain gRPC receives bonding curve update
   └─ Price update → exit condition triggered

2. Brain decision_engine decides SELL
   └─[UDP 45110]→ Executor receives TradeDecision(SELL)

3. Executor builds & sends transaction
   └─[UDP 45115]→ Brain receives ExecutionConfirmation

4. Brain closes position, sends Telegram notification
```

## Port Configuration Files

### Brain

- **Receives on 45100**: `brain/src/udp_bus/receiver.rs`
- **Receives on 45115**: `brain/src/main.rs` (confirmation handler)
- **Receives on 45120**: `brain/src/main.rs` (window metrics)
- **Sends to 45110**: `brain/src/udp_bus/sender.rs`

### Executor

- **Receives on 45110**: `execution/src/advice_bus.rs`
- **Sends to 45115**: `execution/src/main.rs` (ExecutionConfirmation)

### Data-Mining

- **Sends to 45100**: `data-mining/src/udp/mod.rs`
- **Sends to 45120**: `data-mining/src/main.rs` (window metrics)

## Cleanup Tasks

### Remove from Brain

- [ ] Remove HotSignal listener (port 45131)
- [ ] Remove ManualExit listener (port 45135)
- [ ] Remove manual_exit_listener.rs module
- [ ] Update comments mentioning mempool-watcher

### Remove from Executor

- [x] Removed TxConfirmed listener (port 45132) ✅
- [x] Removed ManualExit listener (port 45134) ✅
- [x] Removed WatchSig sender ✅
- [x] Removed Telegram client ✅

### Remove Tool

- [ ] Archive/delete `mempool-watcher/` directory entirely

## Benefits of New Architecture

1. **Simplicity**: 3 active ports instead of 8+
2. **Single Source of Truth**: Brain owns all state
3. **Real-time Updates**: Direct gRPC monitoring (no UDP relay)
4. **Stateless Executor**: Just builds/sends transactions
5. **No Confirmation Tracking**: Brain uses gRPC + RPC polling
6. **Immediate Notifications**: Telegram handled by Brain

## Notes

- Port 45120 (WindowMetrics) is optional - only for analytics
- Brain's gRPC connection replaces 4 obsolete UDP ports (45130-45135)
- Executor is now stateless - all confirmation tracking in Brain
- Total reduction: 8 ports → 4 ports (3 essential + 1 optional)
