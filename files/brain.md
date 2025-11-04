# Brain (Decision Engine) - Comprehensive Reference

**Version**: 2.0 (Updated Nov 1, 2025)  
**Purpose**: Trading decision engine - receives advisories, evaluates opportunities, sends trade decisions  
**Language**: Rust  
**Dependencies**: SQLite (read-only), UDP networking

---

## Directory Structure

```
brain/
├── Cargo.toml                          # Rust dependencies and project config
├── data/
│   └── brain_decisions.csv             # Decision log (optional CSV output)
├── src/
│   ├── main.rs                         # Main entry point, UDP loop, message routing
│   ├── config.rs                       # Configuration loading (DB path, ports, thresholds)
│   ├── metrics.rs                      # Prometheus metrics definitions
│   ├── mint_reservation.rs             # Mint reservation system (duplicate prevention)
│   ├── trade_state.rs                  # Trade state tracking (Enter/Ack/Confirmed/Closed)
│   │
│   ├── decision_engine/
│   │   ├── mod.rs                      # Module exports and public API
│   │   ├── scoring.rs                  # Multi-factor opportunity scoring
│   │   ├── validation.rs               # Trade validation (price, liquidity, quality)
│   │   ├── guardrails.rs               # Risk limits (max positions, cooling, caps)
│   │   ├── position_sizer.rs           # Position sizing based on confidence
│   │   ├── position_tracker.rs         # Active position tracking and management
│   │   ├── triggers.rs                 # Entry/exit trigger logic
│   │   └── logging.rs                  # Structured decision logging
│   │
│   ├── feature_cache/
│   │   ├── mod.rs                      # Cache module exports
│   │   ├── mint_cache.rs               # Token feature caching (LRU, 1000 entries)
│   │   └── wallet_cache.rs             # Wallet stats caching (LRU, 500 entries)
│   │
│   └── udp_bus/
│       ├── mod.rs                      # UDP bus module exports
│       ├── messages.rs                 # Message protocol definitions (29 types)
│       ├── receiver.rs                 # UDP receiver (port 45100, advice bus)
│       └── sender.rs                   # UDP sender (port 45110, decision bus)
│
├── target/                             # Cargo build artifacts (ignored in version control)
│
└── Documentation:
    ├── BRAIN_COMPLETION_SUMMARY.md     # Historical completion summary
    ├── IMPLEMENTATION_COMPLETE.md      # Implementation notes
    └── POSITION_TRACKING_FIX.md        # Position tracking bug fixes
```
