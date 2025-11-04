# Execution - Complete Directory Reference

**Version**: 2.0 (Updated Nov 1, 2025)
**Purpose**: Trade execution engine with Jito bundles and feedback loop
**Status**: Production-ready with TradeClosed message

## Directory Structure

```
execution/
├── Cargo.toml                          # Dependencies (solana-sdk, jito-sdk, postgres, etc.)
├── grafana-dashboard.json              # Monitoring dashboard
├── JITO_RACE_COMPLETE.md               # Jito implementation notes
├── metrics.rs                          # Metrics file (duplicate?)
├── scripts/
│   ├── emoji_map.toml                  # Telegram emoji mappings
│   └── ... (various test scripts)
├── data/
│   └── brain_decisions.csv             # Decision log (received from Brain)
├── src/
│   ├── main.rs                         # Main entry point, UDP listener
│   ├── main_failed.rs                  # Old/failed implementation (DELETE?)
│   │
│   ├── config.rs                       # Configuration loading
│   ├── metrics.rs                      # Prometheus metrics
│   ├── telemetry.rs                    # Telemetry setup
│   │
│   ├── advice_bus.rs                   # UDP receiver for TradeDecision (port 45110)
│   ├── advice_sender.rs                # Sends feedback to Brain (port 45100)
│   │                                   # - EnterAck (type 26)
│   │                                   # - TxConfirmed (type 27)
│   │
│   ├── trade_closed.rs                 # Sends TradeClosed [Task #14]
│   │                                   # - After TxConfirmed processing
│   │                                   # - Includes final status
│   │
│   ├── tx_confirmed.rs                 # TxConfirmed message handling
│   │
│   ├── trading.rs                      # Main trading logic
│   │                                   # - process_trade_decision()
│   │                                   # - Execute buy/sell
│   │                                   # - Track positions
│   │
│   ├── pump_bonding_curve.rs           # Pump.fun bonding curve logic
│   ├── pump_instructions.rs            # Pump.fun instruction building
│   │
│   ├── jito.rs                         # Jito bundle submission
│   ├── tpu_client.rs                   # TPU client (alternative to Jito)
│   │
│   ├── database.rs                     # PostgreSQL trade logging
│   ├── telegram.rs                     # Telegram notifications
│   │
│   ├── slippage.rs                     # Slippage calculations
│   ├── emoji.rs                        # Emoji formatting for logs
│   ├── grpc_client.rs                  # gRPC client (for confirmation?)
│   │
│   ├── mempool.rs                      # Mempool monitoring
│   ├── mempool_bus.rs                  # Mempool UDP receiver
│   │
│   ├── advisor_queue.rs                # Advisory queue management
│   ├── confirmation_task.rs            # Transaction confirmation polling
│   ├── execution_confirmation.rs       # Execution confirmation logic
│   ├── performance_log.rs              # Performance logging
│   │
│   └── data/
│       ├── mod.rs                      # Data module
│       └── strategy_loader.rs          # Strategy loading (if any)
│
├── backtesting/                        # Backtesting system (separate crate)
│   ├── Cargo.toml
│   ├── src/
│   └── ... (backtesting code)
│
└── target/                             # Build artifacts
```

## File Descriptions

### Core Files

**main.rs**
- Main entry point
- Binds UDP receiver to port 45110
- Listens for TradeDecision from Brain
- Dispatches to trading.rs

**trading.rs** (primary trading logic)
- process_trade_decision()
- Execute BUY via pump_bonding_curve
- Execute SELL via stored position data
- Track active positions (HashMap<mint, BuyResult>)
- Calculate P&L
- Send feedback to Brain
- **Task #14 integration**: Calls send_trade_closed()

**advice_bus.rs**
- UDP receiver for TradeDecision (port 45110)
- Deserializes messages
- Queues for processing

**advice_sender.rs**
- Sends feedback to Brain (port 45100)
- send_enter_ack() - Type 26
- send_tx_confirmed() - Type 27

**trade_closed.rs** [Task #14 - NEW]
- send_trade_closed() function
- Called after TxConfirmed processing
- Message type 28
- Includes final_status: CONFIRMED/FAILED/TIMEOUT
- Closes the feedback loop

**tx_confirmed.rs**
- Handles TxConfirmed messages
- Stores signature
- Updates position state

### Blockchain Integration

**pump_bonding_curve.rs**
- Pump.fun bonding curve calculations
- buy() function
- sell() function
- Calculates expected tokens/SOL

**pump_instructions.rs**
- Builds Pump.fun transaction instructions
- Handles accounts and data

**jito.rs**
- Jito bundle submission
- MEV protection
- Tip calculation

**tpu_client.rs**
- Direct TPU submission (alternative)
- Used if Jito unavailable

### Supporting Files

**database.rs**
- PostgreSQL connection
- Log trades to `trades` table
- Log P&L

**telegram.rs**
- Send notifications
- Entry/exit alerts
- P&L reports

**slippage.rs**
- Calculate slippage
- Validate trade prices

**mempool.rs** / **mempool_bus.rs**
- Mempool monitoring (optional feature)
- Not required for core operation

### Potentially Unused Files

**main_failed.rs** - Old implementation (DELETE CANDIDATE)
**metrics.rs** (root level) - Duplicate of src/metrics.rs? (REVIEW)

## Message Flow

```
Brain (port 45110)
    ↓ TradeDecision (type 17)
advice_bus.rs → trading.rs
    ↓
Execute BUY/SELL
    ↓
advice_sender.rs
    ↓ EnterAck (type 26)
    ↓ TxConfirmed (type 27)
    ↓ TradeClosed (type 28) [Task #14]
Brain (port 45100)
```

## Recent Changes

### Task #14: TradeClosed Message ✅

**Added**:
- trade_closed.rs module
- send_trade_closed() function
- Integration in trading.rs after TxConfirmed
- Closes feedback loop definitively

**Benefits**:
- Brain knows when trade is truly done
- Enables position cleanup
- Releases mint reservations
- Provides audit trail

## Total Code

- Total lines: ~8,000+ lines of Rust
- Build time: ~10s incremental
- Binary size: ~20 MB

## Backtesting

Separate crate in `backtesting/`:
- Replays historical data
- Tests strategies
- Reports performance
- See: backtesting/README.md

## See Also

- test_execution.sh: End-to-end test script
- CLEANUP_RECOMMENDATIONS.md: Unused code analysis
