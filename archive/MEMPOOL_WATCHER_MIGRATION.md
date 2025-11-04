# Mempool-Watcher Migration (Archived 2025-11-04)

## Why Removed

The `mempool-watcher` tool has been **deprecated and archived** as part of the architectural simplification (Tasks #1-9).

## Reason for Deprecation

**Original Architecture (4 tools, 8+ UDP ports):**

```
Data-Mining → UDP → Brain
Mempool-Watcher → UDP → Brain (hot signals)
Mempool-Watcher → UDP → Executor (confirmations)
Brain → UDP → Executor
Executor → UDP → Mempool-Watcher (watch signatures)
```

**Problem:**

- Complex message routing
- Stale data in Brain (UDP signals filtered out IN_POSITION tokens)
- Exit conditions never triggered
- Manual sells required

**New Architecture (3 tools, 3 UDP ports):**

```
Data-Mining → UDP 45100 → Brain
Brain (with direct gRPC) monitors bonding curves in real-time
Brain → UDP 45110 → Executor
Executor → UDP 45115 → Brain
```

**Solution:**

- Brain now has **direct Yellowstone gRPC connection**
- Real-time bonding curve monitoring (no UDP relay)
- Fresh prices → exit conditions trigger immediately
- No mempool-watcher intermediary needed

## What Mempool-Watcher Did

1. **Transaction Monitoring**: Subscribed to pending Solana transactions
2. **Hot Signals**: Identified alpha wallet trades, sent urgency signals
3. **Confirmation Tracking**: Monitored transaction confirmations
4. **Manual Exit Detection**: Detected when user manually sold

## What Replaced It

### Brain Now Handles:

- **gRPC Monitoring** (`brain/src/grpc_monitor.rs`): Direct Yellowstone subscription
- **Signature Tracking** (`brain/src/signature_tracker.rs`): Maps signatures to positions
- **Bonding Curve Updates** (`brain/src/bonding_curve.rs`): Parses price updates
- **Position Mapping** (`brain/src/position_mapping.rs`): Maps accounts to mints
- **Telegram Notifications** (`brain/src/telegram.rs`): User notifications

### Benefits:

1. **Simpler**: 3 tools instead of 4
2. **Faster**: No UDP relay latency
3. **Reliable**: No stale data issues
4. **Maintainable**: Single source of truth (Brain)

## Deprecated UDP Ports

These ports are **no longer used**:

- **45130**: mempool-watcher → Brain (mempool intelligence)
- **45131**: mempool-watcher → Brain (hot signals)
- **45132**: mempool-watcher → Executor (TxConfirmed)
- **45134**: mempool-watcher → Executor (ManualExit)
- **45135**: mempool-watcher → Brain (ManualExit)

## Migration Steps Completed

✅ Added Yellowstone gRPC to Brain (Tasks #2-6)
✅ Moved Telegram to Brain (Task #7)
✅ Simplified Executor (Task #8)
✅ Documented new architecture (Task #9)
✅ Archived mempool-watcher (Task #11)

## If You Need It

The complete mempool-watcher source code is preserved in:

```
archive/mempool-watcher-20251104/
```

You can restore it if needed, but the new architecture is:

- More reliable (auto-exit works)
- Simpler (fewer moving parts)
- Faster (no UDP intermediary)

## References

- `UDP_PORT_ARCHITECTURE.md` - New 3-port system
- `LIVE_TESTING_PLAN.md` - Testing the new architecture
- `brain/GRPC_INTEGRATION_COMPLETE.md` - gRPC implementation details
- `execution/TASK8_ANALYSIS.md` - Executor simplification
