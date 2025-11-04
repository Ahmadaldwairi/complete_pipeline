# Solana Trading Bot - 3-Tool Architecture

A high-performance Solana trading bot for Pump.fun tokens with real-time monitoring and automated exit strategies.

## 🏗️ Architecture Overview

The bot consists of **3 independent tools** communicating via UDP:

```
┌─────────────────┐
│  Data-Mining    │  Yellowstone gRPC → Detects new tokens
│                 │  Sends signals via UDP 45100
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│     Brain       │  Decision Engine + gRPC Position Monitoring
│  - Evaluates    │  - Receives signals (UDP 45100)
│  - Decides      │  - Monitors bonding curves (Yellowstone gRPC)
│  - Tracks       │  - Sends decisions (UDP 45110)
│  - Notifies     │  - Receives confirmations (UDP 45115)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│    Executor     │  Stateless Transaction Builder
│  - Builds TX    │  - Receives decisions (UDP 45110)
│  - Sends TX     │  - Sends confirmations (UDP 45115)
│  - Returns      │
└─────────────────┘
```

## 📁 Project Structure

```
scalper-bot/
├── data-mining/          # Token discovery (Yellowstone gRPC)
│   ├── src/
│   │   ├── grpc/        # gRPC client
│   │   ├── parser/      # Transaction parsing
│   │   ├── udp/         # Signal sender (port 45100)
│   │   └── main.rs
│   └── Cargo.toml
│
├── brain/               # Decision engine + position tracking
│   ├── src/
│   │   ├── decision_engine/  # Entry/exit logic
│   │   ├── feature_cache/    # Token/wallet caching
│   │   ├── udp_bus/          # UDP receiver (45100) & sender (45110)
│   │   ├── grpc_monitor.rs   # Real-time bonding curve monitoring
│   │   ├── signature_tracker.rs  # Confirmation tracking
│   │   ├── telegram.rs       # User notifications
│   │   └── main.rs
│   └── Cargo.toml
│
├── execution/           # Transaction execution
│   ├── src/
│   │   ├── trading.rs       # Buy/sell logic
│   │   ├── advice_bus.rs    # Decision receiver (port 45110)
│   │   ├── execution_confirmation.rs  # Confirmation sender (45115)
│   │   └── main.rs
│   └── Cargo.toml
│
├── UDP_PORT_ARCHITECTURE.md    # Port documentation
├── LIVE_TESTING_PLAN.md        # Testing guide
└── archive/                    # Deprecated code
    └── mempool-watcher-20251104/  # Archived (obsolete)
```

## 🔌 UDP Port Architecture

| Port      | Direction           | Purpose                 | Status    |
| --------- | ------------------- | ----------------------- | --------- |
| **45100** | data-mining → Brain | Token signals           | ✅ Active |
| **45110** | Brain → Executor    | Trade decisions         | ✅ Active |
| **45115** | Executor → Brain    | Execution confirmations | ✅ Active |

**Deprecated ports** (45130-45135): Removed with mempool-watcher (see `archive/`)

## 🚀 Quick Start

### Prerequisites

- Rust 1.70+
- Solana CLI tools
- Yellowstone gRPC endpoint
- Telegram bot (optional, for notifications)

### 1. Configuration

**Data-Mining** (`data-mining/config.toml`):

```toml
grpc_endpoint = "your-yellowstone-endpoint"
udp_target = "127.0.0.1:45100"
```

**Brain** (`brain/config.toml`):

```toml
advice_port = 45100        # Receive signals
decision_port = 45110      # Send decisions
confirmation_port = 45115  # Receive confirmations
grpc_endpoint = "your-yellowstone-endpoint"
telegram_token = "your-bot-token"
```

**Executor** (`execution/config.toml`):

```toml
advice_bus_port = 45110    # Receive decisions
brain_port = 45115         # Send confirmations
rpc_url = "your-rpc-endpoint"
```

### 2. Build

```bash
# Build all components
cd data-mining && cargo build --release
cd ../brain && cargo build --release
cd ../execution && cargo build --release
```

### 3. Run

Open 3 terminals:

```bash
# Terminal 1: Data-Mining
cd data-mining
RUST_LOG=info cargo run --release

# Terminal 2: Brain
cd brain
RUST_LOG=info cargo run --release

# Terminal 3: Executor
cd execution
RUST_LOG=info cargo run --release
```

## 📊 How It Works

### 1. Token Discovery

```
data-mining → Yellowstone gRPC → New token detected
            → UDP 45100 → Brain receives signal
```

### 2. Entry Decision

```
Brain → Evaluates token (rank, volume, buyers)
      → Decides BUY
      → UDP 45110 → Executor receives decision
```

### 3. Transaction Execution

```
Executor → Builds transaction
         → Sends to Solana
         → UDP 45115 → Brain receives confirmation
```

### 4. Position Monitoring

```
Brain → Yellowstone gRPC → Bonding curve updates (every ~400ms)
      → Price updated in mint_cache
      → Exit condition evaluated
```

### 5. Exit Decision

```
Brain → Target profit reached OR stop-loss hit
      → Decides SELL
      → UDP 45110 → Executor receives decision
      → Transaction sent
      → Position closed
```

## 🎯 Key Features

### Brain (Decision Engine)

- ✅ **Real-time gRPC monitoring** of bonding curves
- ✅ **Automatic exit** when profit/loss targets hit
- ✅ **Position tracking** with complete lifecycle
- ✅ **Telegram notifications** on entry/exit
- ✅ **Multiple entry strategies** (rank, momentum, copy-trade)
- ✅ **Risk management** (stop-loss, max position size)

### Executor (Transaction Builder)

- ✅ **Stateless** - no position tracking
- ✅ **Fast execution** - cached blockhashes
- ✅ **Deduplication** - prevents double-trades (5s window)
- ✅ **Simple** - receive decision → build → send → confirm

### Data-Mining (Signal Generator)

- ✅ **Real-time** Yellowstone gRPC subscription
- ✅ **Transaction parsing** for new tokens
- ✅ **Signal filtering** (volume, buyers, momentum)
- ✅ **UDP broadcast** to Brain

## 📈 Performance

- **Entry latency**: < 500ms from token detection
- **gRPC update frequency**: ~400ms (bonding curve monitoring)
- **Exit latency**: < 300ms from condition trigger
- **Total BUY→SELL cycle**: Typically 2-10 seconds

## 🔧 Troubleshooting

### Brain not receiving signals

```bash
# Check UDP port
netstat -an | grep 45100

# Test data-mining output
cd data-mining && RUST_LOG=debug cargo run
```

### Exit conditions not triggering

```bash
# Check gRPC connection in Brain logs
grep "gRPC" brain/logs/brain.log

# Verify price updates
grep "Price update" brain/logs/brain.log
```

### Executor not executing

```bash
# Check port binding
netstat -an | grep 45110

# Verify decisions received
grep "TradeDecision" execution/logs/execution.log
```

See `LIVE_TESTING_PLAN.md` for comprehensive testing guide.

## 📚 Documentation

- `UDP_PORT_ARCHITECTURE.md` - Port mappings and message flow
- `LIVE_TESTING_PLAN.md` - Testing procedures
- `brain/GRPC_INTEGRATION_COMPLETE.md` - gRPC implementation
- `execution/TASK8_ANALYSIS.md` - Executor simplification
- `archive/MEMPOOL_WATCHER_MIGRATION.md` - What changed

## 🗂️ Migration from Old Architecture

The bot was simplified from 4 tools to 3:

**Removed**: `mempool-watcher` (archived in `archive/mempool-watcher-20251104/`)

**Why**:

- Brain now monitors positions directly via gRPC
- No need for UDP relay (faster, more reliable)
- Simpler architecture (3 tools instead of 4)
- Fixes auto-exit issues (stale data problem solved)

## 🔐 Security

- Private keys stored in `execution/.env`
- Telegram tokens in `brain/.env`
- Never commit `.env` files
- Use `.env.example` as template

## 🤝 Contributing

1. Test changes with `LIVE_TESTING_PLAN.md`
2. Ensure all 3 tools compile: `cargo build --release`
3. Document architecture changes in relevant `.md` files
4. Keep the 3-tool simplicity - don't add complexity

## 📝 License

Proprietary - Do not distribute

## ⚠️ Disclaimer

This bot trades real money. Use at your own risk. Test thoroughly on devnet first.
