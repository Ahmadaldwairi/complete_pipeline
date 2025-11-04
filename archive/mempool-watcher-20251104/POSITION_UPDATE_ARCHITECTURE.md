# Position Update Architecture - Mempool-Assisted Exit Signals

## Overview

Mempool-watcher now acts as a **real-time position monitor** that tracks P&L and sends updates to Brain for exit decisions.

## Message Flow

```
Entry (BUY):
Executor → WatchSigEnhanced (45130) → Mempool-watcher
  ├─ Includes: entry_price, size_sol, slippage, profit_target, stop_loss
  └─ Mempool stores this metadata

Position Monitoring:
Mempool-watcher continuously monitors:
  ├─ Current market price (from Yellowstone trades)
  ├─ Mempool activity (pending buys/sells)
  ├─ Price velocity
  └─ Calculates real-time P&L

Position Updates:
Mempool → PositionUpdate (45131) → Brain
  ├─ Sent every 5s OR when price moves >5%
  ├─ Includes: current_price, realized_pnl_usd, pnl_percent
  ├─ Flags: profit_target_hit, stop_loss_hit, no_mempool_activity
  └─ Brain makes exit decision based on strategy rules

Exit Decision:
Brain → SELL Decision (45110) → Executor
  └─ Brain decides WHEN to exit, mempool provides data
```

## Separation of Concerns

### Mempool-watcher Responsibilities:

✅ Track actual on-chain position value  
✅ Calculate real-time P&L in USD  
✅ Monitor mempool activity for this mint  
✅ Detect profit targets / stop losses hit  
✅ Send `PositionUpdate` messages to Brain

### Brain Responsibilities:

✅ Receive `PositionUpdate` messages  
✅ Apply strategy-specific exit logic:

- Path-specific profit targets
- MC velocity analysis (Momentum path)
- Time decay rules
- Percentage-based targets  
  ✅ Make final EXIT decision  
  ✅ Send SELL decision to Executor

## Benefits

1. **Real-time P&L**: No dependency on stale cache data
2. **Accurate pricing**: Mempool sees actual confirmed trades
3. **Mempool intelligence**: Brain knows if volume is still coming
4. **Clean separation**: Mempool monitors, Brain decides
5. **Low latency**: UDP messages, sub-second updates

## Implementation Status

✅ `PositionUpdate` message defined (MSG_TYPE 32)  
⚠️ Mempool position tracking logic (TODO)  
⚠️ Brain `PositionUpdate` receiver (TODO)  
⚠️ Integrate with existing exit logic (TODO)

## Next Steps

1. **Mempool**: Track WatchSigEnhanced entries in memory
2. **Mempool**: Calculate P&L when new trades arrive
3. **Mempool**: Send PositionUpdate to Brain (45131)
4. **Brain**: Add PositionUpdate to AdviceMessage enum
5. **Brain**: Use PositionUpdate data for exit decisions
6. **Brain**: Remove dependency on mint cache for exits
