# Data-Mining - Complete Directory Reference

**Version**: 2.0 (Updated Nov 1, 2025)
**Purpose**: Real-time Pump.fun transaction collection, parsing, and advisory generation
**Status**: Production-ready with sliding window analytics

## Directory Structure

```
data-mining/
├── Cargo.toml                          # Dependencies (yellowstone-grpc, sqlite, tokio)
├── config.toml                          # gRPC endpoint, database path, thresholds
├── config.example.toml                  # Example configuration
├── README.md                            # Getting started guide
├── data/                                # Database storage
│   └── collector.db                     # SQLite database (created at runtime)
├── src/
│   ├── main.rs                          # Main entry point, gRPC stream processor
│   ├── lib.rs                           # Library exports
│   ├── config.rs                        # Configuration loading
│   ├── checkpoint.rs                    # Stream checkpoint management
│   │
│   ├── db/
│   │   ├── mod.rs                       # Database module
│   │   ├── aggregator.rs                # Window aggregation (2s/5s/60s)
│   │   └── checkpoint.rs                # Checkpoint persistence
│   │
│   ├── decoder/
│   │   └── mod.rs                       # Transaction decoding
│   │
│   ├── grpc/
│   │   └── mod.rs                       # Yellowstone gRPC client
│   │
│   ├── parser/
│   │   ├── mod.rs                       # Transaction parser
│   │   └── raydium.rs                   # Raydium-specific parsing
│   │
│   ├── types/
│   │   └── mod.rs                       # Type definitions
│   │
│   ├── udp/
│   │   └── mod.rs                       # UDP advisory sender
│   │                                    # - send_momentum_opportunity()
│   │                                    # - send_sol_price()
│   │                                    # - send_window_metrics() [Task #15]
│   │
│   ├── momentum_tracker.rs              # Momentum detection logic
│   ├── window_tracker.rs                # Sliding window analytics [Task #15]
│   │                                    # - Real-time 1s/2s/10s windows
│   │                                    # - Throttled sending (500ms)
│   │                                    # - Activity threshold (3+ trades)
│   │
│   ├── pyth_http.rs                     # Pyth HTTP price fetching
│   ├── pyth_subscriber_rpc.rs           # Pyth RPC subscriber
│   └── pyth_subscriber.rs               # Pyth websocket subscriber
│
└── target/                              # Build artifacts
```

## File Descriptions

### Core Files

**main.rs** (~1,200 lines)
- Main gRPC stream processor
- Connects to Yellowstone validator
- Subscribes to Pump.fun program transactions
- Parses and stores trades
- Integrates momentum_tracker and window_tracker
- Sends UDP advisories to Brain
- **Task #15 additions**:
  * WindowTracker initialization (lines 116-122)
  * Integration in transaction processing (lines 572-603)
  * Calls window_tracker.add_trade()
  * Sends WindowMetrics via UDP

**lib.rs**
- Module exports
- **Task #15**: Added `pub mod window_tracker;`

**config.rs**
- Load config.toml
- Parse gRPC endpoint, database path
- Advisory thresholds

### Database Module (src/db/)

**aggregator.rs**
- Creates time windows (2s, 5s, 60s)
- Aggregates: volume, buyers, price change
- Updates windows_2s, windows_5s, windows_60s tables

**mod.rs**
- Database connection management
- Schema creation
- Tables: tokens, trades, wallet_stats, positions, windows_*

### UDP Module (src/udp/)

**mod.rs** (662 lines)
- BrainSignalSender struct
- Message sending functions:
  * send_momentum_opportunity() - Type 16
  * send_sol_price() - Type 14
  * send_window_metrics() - Type 29 [Task #15, lines 604-662]

### Analytics Modules

**momentum_tracker.rs**
- Detects momentum opportunities
- Criteria: volume spikes, buyer surges, price momentum
- Triggers MomentumOpportunity advisories

**window_tracker.rs** (322 lines) [Task #15 - NEW]
- Real-time sliding window calculations
- Per-mint tracking with VecDeque<TradeEvent>
- Metrics calculated:
  * volume_sol_1s: SOL volume in last 1 second
  * unique_buyers_1s: Unique buyers (HashSet)
  * price_change_bps_2s: Price change in basis points
  * alpha_wallet_hits_10s: Alpha wallet activity
- Features:
  * Automatic cleanup (events >10s old)
  * Throttling (500ms min between sends)
  * Activity threshold (3+ trades in 2s)
  * Memory-safe (prevents leaks)

### Price Feeds

**pyth_http.rs** - HTTP price fetching
**pyth_subscriber_rpc.rs** - RPC-based price updates
**pyth_subscriber.rs** - Websocket price feed

## Database Schema

### tables (collector.db)

1. **tokens**: Token launches
2. **trades**: All buy/sell trades
3. **wallet_stats**: Wallet performance (win rate, P&L)
4. **positions**: Active position tracking
5. **windows_2s**: 2-second aggregations
6. **windows_5s**: 5-second aggregations
7. **windows_60s**: 60-second aggregations

## UDP Messages Sent

To Brain on port 45100:

1. **SolPriceUpdate** (type 14) - SOL/USD price
2. **MomentumOpportunity** (type 16) - Entry signals
3. **WindowMetrics** (type 29) [Task #15] - Real-time analytics

## Recent Changes

### Task #15: Sliding Window Analytics ✅

**Added**:
- window_tracker.rs module (322 lines)
- WindowTracker initialization in main.rs
- Integration in transaction processing
- send_window_metrics() in udp/mod.rs
- Metrics: volume_sol_1s, unique_buyers_1s, price_change_bps_2s, alpha_wallet_hits_10s

**Benefits**:
- Real-time market intelligence
- Smart exit timing signals
- Alpha wallet activity tracking
- Momentum confirmation

## Total Code

- Total lines: ~5,000+ lines of Rust
- Build time: ~8s incremental
- Database: SQLite (single file)

## See Also

- test_data_mining.sh: End-to-end test script
- CLEANUP_RECOMMENDATIONS.md: Unused code analysis
