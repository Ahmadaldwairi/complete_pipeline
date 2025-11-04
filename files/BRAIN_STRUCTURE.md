# Brain (Decision Engine) - Complete Directory Reference

**Version**: 2.0 (Updated Nov 1, 2025)
**Purpose**: Trading decision engine with duplicate prevention and state tracking
**Status**: Production-ready (Tasks #1-17 complete)

## Directory Structure

```
brain/
│   ├── BRAIN_COMPLETION_SUMMARY.md
│   ├── Cargo.toml
│   ├── IMPLEMENTATION_COMPLETE.md
│   └── POSITION_TRACKING_FIX.md
│   └── brain_decisions.csv
│   ├── config.rs
│   ├── main.rs
│   ├── metrics.rs
│   ├── mint_reservation.rs
│   └── trade_state.rs
│   ├── decision_engine/
│   │   ├── guardrails.rs
│   │   ├── logging.rs
│   │   ├── mod.rs
│   │   ├── position_sizer.rs
│   │   ├── position_tracker.rs
│   │   ├── scoring.rs
│   │   ├── triggers.rs
│   │   └── validation.rs
│   ├── feature_cache/
│   │   ├── mint_cache.rs
│   │   ├── mod.rs
│   │   └── wallet_cache.rs
│   ├── udp_bus/
│   │   ├── messages.rs
│   │   ├── mod.rs
│   │   ├── receiver.rs
│   │   └── sender.rs
```

## File Descriptions

### Core Files (src/)

**main.rs** (2,467 lines)
- Main entry point and UDP message loop
- Binds to port 45100 (receives from data-mining)
- Routes 29 message types to handlers
- Implements message hash for deduplication
- Manages position tracking and trade states
- Integrates all subsystems

**config.rs** (98 lines)
- Configuration loading from environment
- Defaults for all parameters
- Validation logic

**metrics.rs** (156 lines)
- Prometheus metrics definitions
- Counters, histograms, gauges
- Exposed on port 9091

**mint_reservation.rs** (108 lines)
- Duplicate prevention system
- Reserve/release mint tracking
- Thread-safe HashMap with timeouts

**trade_state.rs** (89 lines)
- Trade lifecycle tracking
- States: Enter → EnterAck → TxConfirmed → TradeClosed
- Audit trail support

### Decision Engine (src/decision_engine/)

**scoring.rs** (456 lines)
- Multi-factor opportunity scoring
- Window metrics (50%), wallet quality (30%), token age (20%)
- Score normalization (0-100)

**validation.rs** (378 lines)
- Trade validation logic
- Price, liquidity, wallet quality checks
- Returns pass/fail with reasons

**guardrails.rs** (267 lines)
- Risk management
- Max positions, cooling periods, size caps
- Thread-safe enforcement

**position_sizer.rs** (134 lines)
- Calculate position size based on score and exposure
- Scales dynamically

**position_tracker.rs** (201 lines)
- Track all active positions
- Entry/exit management
- Exposure calculation

**triggers.rs** (189 lines)
- Entry/exit trigger logic
- Profit targets, stop losses, time-based

**logging.rs** (123 lines)
- Structured decision logging
- CSV output for analysis

### Feature Cache (src/feature_cache/)

**mint_cache.rs** (234 lines)
- LRU cache for token features (1000 entries)
- 5-minute TTL
- ~90% hit rate

**wallet_cache.rs** (189 lines)
- LRU cache for wallet stats (500 entries)
- 5-minute TTL

### UDP Bus (src/udp_bus/)

**messages.rs** (1,787 lines)
- 29 message type definitions
- Fixed-size packet serialization
- Types include:
  * SolPriceUpdate (14)
  * MomentumOpportunity (16)
  * TradeDecision (17)
  * EnterAck (26)
  * TxConfirmed (27)
  * TradeClosed (28) - Task #14
  * WindowMetrics (29) - Task #15

**receiver.rs** (410 lines)
- UDP receiver (port 45100)
- Message parsing and routing
- Logging for all message types

**sender.rs** (278 lines)
- UDP sender (port 45110)
- TradeDecision transmission to Executor

## Recent Additions

### Task #14: TradeClosed Message ✅
- Added TradeClosed (type 28) for definitive trade closure
- Triggers position cleanup and mint reservation release
- Provides audit trail

### Task #15: WindowMetrics ✅
- Added WindowMetrics (type 29) for real-time market analytics
- Sliding window metrics: volume_sol_1s, unique_buyers_1s, price_change_bps_2s, alpha_wallet_hits_10s
- Smart exit logic: ExtendHold/WidenExit advisories

## Total Code

- Total lines (excluding target/): ~7,564 lines of Rust
- Build time (release): ~4s incremental, ~60s clean
- Binary size: ~15 MB

## Dependencies (Key)

- tokio: Async runtime
- rusqlite: SQLite queries (read-only)
- prometheus: Metrics
- lru: Cache implementation
- serde: Serialization

## See Also

- test_brain.sh: End-to-end test script
- CLEANUP_RECOMMENDATIONS.md: Unused code analysis
