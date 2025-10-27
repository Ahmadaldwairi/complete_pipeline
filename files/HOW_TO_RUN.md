# 🚀 How to Run - Solana Scalper Bot

Complete guide to building and running all services in the scalper bot system.

---

## 📋 Table of Contents

1. [Prerequisites](#prerequisites)
2. [Quick Start (All Services)](#quick-start-all-services)
3. [Individual Services](#individual-services)
   - [Data Mining Service](#1-data-mining-service)
   - [Brain Service (Decision Engine)](#2-brain-service-decision-engine)
   - [Execution Service](#3-execution-service)
4. [Optimized Production Build](#optimized-production-build)
5. [Testing & Verification](#testing--verification)
6. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### System Requirements

- **OS**: Linux (Ubuntu 20.04+ recommended)
- **CPU**: 4+ cores (8+ recommended for production)
- **RAM**: 8GB minimum (16GB+ recommended)
- **Disk**: 20GB free space

### Software Dependencies

```bash
# Rust toolchain (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# PostgreSQL (for data-mining)
sudo apt install postgresql postgresql-contrib

# System libraries
sudo apt install pkg-config libssl-dev build-essential
```

### Database Setup

```bash
# Create PostgreSQL database
sudo -u postgres psql
CREATE DATABASE wallet_tracker;
CREATE USER trader WITH PASSWORD 'trader123';
GRANT ALL PRIVILEGES ON DATABASE wallet_tracker TO trader;
\q

# Create execution database
sudo -u postgres psql
CREATE DATABASE pump_trading;
CREATE USER ahmad WITH PASSWORD 'Jadoo31991';
GRANT ALL PRIVILEGES ON DATABASE pump_trading TO ahmad;
\q
```

---

## Quick Start (All Services)

### Option 1: Automated Launcher (Recommended)

```bash
# Navigate to project root
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot

# Make launcher executable
chmod +x integration-test/start_services.sh

# Launch all services in separate terminals
./integration-test/start_services.sh
```

This will open 3 terminal windows:

1. **Brain Service** (decision engine)
2. **Execution Service** (trade executor)
3. **Data Mining Service** (market data collector)

### Option 2: Manual Start (All Services)

```bash
# Terminal 1: Data Mining
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/data-mining
cargo run --release

# Terminal 2: Brain Service
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain
cargo run --release

# Terminal 3: Execution Service
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution
cargo run --release
```

---

## Individual Services

### 1. Data Mining Service

**Purpose**: Collects market data from Solana blockchain via gRPC and broadcasts to Brain/Executor

**Location**: `data-mining/`

#### Development Mode

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/data-mining

# Build and run with debug output
cargo run

# Or with specific log level
RUST_LOG=info cargo run
```

#### Production Mode (Optimized)

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/data-mining

# Build optimized binary
cargo build --release

# Run optimized binary
./target/release/data-mining

# Or with custom config
./target/release/data-mining --config custom-config.toml
```

#### Key Features

- **gRPC Subscription**: Connects to Yellowstone gRPC (port 10000)
- **Pyth Oracle**: Subscribes to SOL/USD price feed
- **UDP Broadcasting**:
  - Port 45100: Brain service (trade decisions)
  - Port 45110: Executor service (price updates)
- **Database**: SQLite (`data/collector.db`)

#### Verification

```bash
# Check if service is listening
netstat -tuln | grep 10000  # gRPC connection

# Check UDP broadcasts
tcpdump -i lo udp port 45100  # Brain updates
tcpdump -i lo udp port 45110  # Executor updates

# Check logs
tail -f data-mining.log
```

---

### 2. Brain Service (Decision Engine)

**Purpose**: Analyzes market data and makes trading decisions, sends signals to executor

**Location**: `brain/`

#### Development Mode

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain

# Build and run
cargo run

# With debug logging
RUST_LOG=debug cargo run
```

#### Production Mode (Optimized)

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain

# Build with optimizations
cargo build --release

# Run optimized binary
./target/release/decision_engine

# Background mode with logging
nohup ./target/release/decision_engine > brain.log 2>&1 &
```

#### Configuration

Edit `brain/.env` before running:

```properties
# Decision Thresholds
MIN_DECISION_CONF=75
MIN_COPYTRADE_CONFIDENCE=70
MIN_FOLLOW_THROUGH_SCORE=55

# Validation
FEE_MULTIPLIER=2.2
IMPACT_CAP_MULTIPLIER=0.45
MIN_LIQUIDITY_USD=5000.0
MAX_SLIPPAGE=0.15

# Guardrails
MAX_CONCURRENT_POSITIONS=3
MAX_ADVISOR_POSITIONS=2
RATE_LIMIT_MS=100
```

#### Key Features

- **Decision Engine**: Analyzes trades, scores opportunities
- **Validation**: Fee impact, liquidity, slippage checks
- **UDP Communication**: Sends trade decisions to executor (port 45100)
- **Telemetry Receiver**: Receives execution results (port 45110)
- **Feature Cache**: Fast lookup for mints/wallets

#### Verification

```bash
# Check if listening for data
netstat -tuln | grep 45110  # Telemetry port

# Monitor decisions
tail -f data/brain_decisions.csv

# Check metrics
./quick_metrics_test.sh
```

---

### 3. Execution Service

**Purpose**: Receives trade decisions from Brain and executes transactions on Solana

**Location**: `execution/`

#### Development Mode

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution

# Build and run
cargo run

# With trace logging
RUST_LOG=trace cargo run
```

#### Production Mode (Optimized) ⚡

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution

# Build with maximum optimizations
cargo build --release

# Run optimized binary
./target/release/execution-bot

# Background mode with logging
nohup ./target/release/execution-bot > executor.log 2>&1 &
```

#### Optimized Production Script

Create `execution/run_optimized.sh`:

```bash
#!/bin/bash
# Optimized execution with performance tuning

set -e

echo "🚀 Starting Executor in OPTIMIZED mode"
echo "========================================"

# Set CPU governor to performance mode
echo "⚡ Setting CPU to performance mode..."
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Increase network buffer sizes
echo "📡 Tuning network buffers..."
sudo sysctl -w net.core.rmem_max=26214400
sudo sysctl -w net.core.wmem_max=26214400

# Build with optimizations
echo "🔨 Building optimized binary..."
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Set process priority
echo "⚡ Running with high priority..."
sudo nice -n -10 ./target/release/execution-bot

echo "✅ Executor stopped"
```

Make it executable:

```bash
chmod +x execution/run_optimized.sh
./run_optimized.sh
```

#### Configuration

Edit `execution/.env` before running:

```properties
# RPC Endpoints
RPC_ENDPOINT=http://127.0.0.1:8899
GRPC_ENDPOINT=http://127.0.0.1:10000

# Wallet
WALLET_PRIVATE_KEY=your_private_key_here
ACCOUNT_PATH=./keypair.json

# Execution Mode
USE_TPU=true
USE_JITO=false

# Advice Bus (receives decisions from Brain)
ADVISOR_ENABLED=true
ADVICE_MIN_CONFIDENCE=70
```

#### Key Features

- **Trade Execution**: Buys/sells tokens on Solana
- **TPU Client**: Direct transaction submission
- **Jito Integration**: MEV-protected bundle execution (optional)
- **Advice Bus Listener**: Port 45100 (receives decisions from Brain)
- **Slippage Calculator**: Measures execution quality
- **Performance Logging**: JSONL logs to `logs/performance.jsonl`
- **Telegram Notifications**: Async trade alerts

#### Verification

```bash
# Check if listening for decisions
netstat -tuln | grep 45100  # Advice bus port

# Monitor trades
tail -f logs/execution.log

# Check performance metrics
tail -f logs/performance.jsonl | jq .

# View executed trades in database
psql -d pump_trading -U ahmad -c "SELECT * FROM executions ORDER BY timestamp DESC LIMIT 10;"
```

---

## Optimized Production Build

### Full System Optimization

Create `build_all_optimized.sh` in project root:

```bash
#!/bin/bash
# Build all services with maximum optimizations

set -e

PROJECT_ROOT="/home/sol/Desktop/solana-dev/Bots/scalper-bot"

echo "🔨 Building all services with OPTIMIZATIONS"
echo "============================================="
echo ""

# Set optimization flags
export RUSTFLAGS="-C target-cpu=native -C opt-level=3"

# Build data-mining
echo "📊 Building data-mining service..."
cd "$PROJECT_ROOT/data-mining"
cargo build --release
echo "✅ data-mining built"
echo ""

# Build brain
echo "🧠 Building brain service..."
cd "$PROJECT_ROOT/brain"
cargo build --release
echo "✅ brain built"
echo ""

# Build execution
echo "⚡ Building execution service..."
cd "$PROJECT_ROOT/execution"
cargo build --release
echo "✅ execution built"
echo ""

echo "✅ ALL SERVICES BUILT"
echo ""
echo "Binary locations:"
echo "  data-mining: $PROJECT_ROOT/data-mining/target/release/data-mining"
echo "  brain:       $PROJECT_ROOT/brain/target/release/decision_engine"
echo "  execution:   $PROJECT_ROOT/execution/target/release/execution-bot"
```

Run it:

```bash
chmod +x build_all_optimized.sh
./build_all_optimized.sh
```

### Production Deployment

```bash
# Create deployment directory
mkdir -p ~/scalper-bot-prod
cd ~/scalper-bot-prod

# Copy optimized binaries
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/data-mining/target/release/data-mining .
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain/target/release/decision_engine .
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution/target/release/execution-bot .

# Copy config files
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain/.env brain.env
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/execution/.env execution.env
cp /home/sol/Desktop/solana-dev/Bots/scalper-bot/data-mining/config.toml data-mining.toml

# Run services
./data-mining --config data-mining.toml &
./decision_engine &
./execution-bot &
```

---

## Testing & Verification

### 1. Test Data Mining

```bash
cd data-mining

# Check gRPC connection
cargo run 2>&1 | grep "Connected to gRPC"

# Verify Pyth subscription
cargo run 2>&1 | grep "Pyth price"

# Test UDP broadcasts
python3 -c "
import socket
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(('127.0.0.1', 45100))
print('Listening for UDP messages...')
data, addr = sock.recvfrom(1024)
print(f'Received: {len(data)} bytes from {addr}')
"
```

### 2. Test Brain Service

```bash
cd brain

# Run metrics test
./quick_metrics_test.sh

# Check decision output
cargo run 2>&1 | grep "DECISION"

# Verify telemetry receiver
netstat -tuln | grep 45110
```

### 3. Test Execution Service

```bash
cd execution

# Test advice bus integration
./scripts/test_advice_integration.sh

# Verify listener
cargo run 2>&1 | grep "Listening for TradeDecisions"

# Check database connection
psql -d pump_trading -U ahmad -c "\dt"
```

### Integration Test

```bash
# Terminal 1: Start data-mining
cd data-mining && cargo run --release

# Terminal 2: Start brain
cd brain && cargo run --release

# Terminal 3: Start execution
cd execution && cargo run --release

# Terminal 4: Monitor logs
tail -f brain/data/brain_decisions.csv
tail -f execution/logs/performance.jsonl
```

---

## Troubleshooting

### Common Issues

#### 1. "Cannot connect to gRPC endpoint"

```bash
# Check if Yellowstone gRPC is running
curl http://127.0.0.1:10000

# If not, start your local Solana validator or use remote endpoint
# Edit data-mining/config.toml:
# endpoint = "http://your-grpc-endpoint:port"
```

#### 2. "Database connection failed"

```bash
# Check PostgreSQL status
sudo systemctl status postgresql

# Restart PostgreSQL
sudo systemctl restart postgresql

# Verify credentials in .env files match database users
```

#### 3. "UDP port already in use"

```bash
# Check what's using the port
sudo lsof -i :45100

# Kill the process
kill -9 <PID>
```

#### 4. "Compilation errors"

```bash
# Update Rust toolchain
rustup update

# Clean build cache
cargo clean

# Rebuild
cargo build --release
```

#### 5. "High CPU usage"

```bash
# Check if multiple instances running
ps aux | grep -E "data-mining|decision_engine|execution-bot"

# Kill duplicate processes
killall data-mining decision_engine execution-bot

# Restart one instance per service
```

#### 6. "No trades executing"

```bash
# Check Brain is sending decisions
tail -f brain/data/brain_decisions.csv

# Check Executor is receiving
cd execution && cargo run 2>&1 | grep "RECEIVED TradeDecision"

# Verify network connectivity
ping -c 3 127.0.0.1
```

### Performance Optimization

#### Low Latency Setup

```bash
# Set CPU governor to performance
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Disable CPU frequency scaling
sudo systemctl disable ondemand

# Increase network buffers
sudo sysctl -w net.core.rmem_max=26214400
sudo sysctl -w net.core.wmem_max=26214400
sudo sysctl -w net.ipv4.udp_mem="102400 873800 16777216"

# Disable swap (optional, if enough RAM)
sudo swapoff -a
```

#### Memory Optimization

```bash
# If running out of memory, reduce cache sizes in brain/.env:
MINT_CACHE_CAPACITY=5000
WALLET_CACHE_CAPACITY=2500
CACHE_REFRESH_INTERVAL_SECS=60
```

---

## Monitoring & Logs

### Log Locations

```bash
# Data Mining
tail -f data-mining/data-mining.log

# Brain
tail -f brain/data/brain_decisions.csv
tail -f brain/brain.log

# Execution
tail -f execution/logs/execution.log
tail -f execution/logs/performance.jsonl

# System logs
journalctl -u scalper-bot -f  # If running as systemd service
```

### Real-time Monitoring

```bash
# Watch all logs simultaneously
tmux new-session \; \
  split-window -h \; \
  split-window -v \; \
  send-keys 'tail -f data-mining/data-mining.log' C-m \; \
  select-pane -t 1 \; \
  send-keys 'tail -f brain/data/brain_decisions.csv' C-m \; \
  select-pane -t 2 \; \
  send-keys 'tail -f execution/logs/performance.jsonl | jq .' C-m
```

---

## Systemd Services (Production)

### Create systemd service files

**data-mining.service**:

```ini
[Unit]
Description=Scalper Bot Data Mining Service
After=network.target postgresql.service

[Service]
Type=simple
User=sol
WorkingDirectory=/home/sol/scalper-bot-prod
ExecStart=/home/sol/scalper-bot-prod/data-mining --config data-mining.toml
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**brain.service**:

```ini
[Unit]
Description=Scalper Bot Brain Service
After=network.target data-mining.service

[Service]
Type=simple
User=sol
WorkingDirectory=/home/sol/scalper-bot-prod
EnvironmentFile=/home/sol/scalper-bot-prod/brain.env
ExecStart=/home/sol/scalper-bot-prod/decision_engine
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**execution.service**:

```ini
[Unit]
Description=Scalper Bot Execution Service
After=network.target brain.service

[Service]
Type=simple
User=sol
WorkingDirectory=/home/sol/scalper-bot-prod
EnvironmentFile=/home/sol/scalper-bot-prod/execution.env
ExecStart=/home/sol/scalper-bot-prod/execution-bot
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

### Install and manage services

```bash
# Copy service files
sudo cp *.service /etc/systemd/system/

# Reload systemd
sudo systemctl daemon-reload

# Enable services (start on boot)
sudo systemctl enable data-mining brain execution

# Start services
sudo systemctl start data-mining
sudo systemctl start brain
sudo systemctl start execution

# Check status
sudo systemctl status data-mining brain execution

# View logs
journalctl -u data-mining -f
journalctl -u brain -f
journalctl -u execution -f

# Stop services
sudo systemctl stop data-mining brain execution
```

---

## Quick Reference

### Start All Services

```bash
# Automated
./integration-test/start_services.sh

# Manual
cd data-mining && cargo run --release &
cd brain && cargo run --release &
cd execution && cargo run --release &
```

### Stop All Services

```bash
killall data-mining decision_engine execution-bot
```

### Rebuild Everything

```bash
./build_all_optimized.sh
```

### Check Service Status

```bash
ps aux | grep -E "data-mining|decision_engine|execution-bot"
```

### View Performance Metrics

```bash
# Brain decisions
tail -f brain/data/brain_decisions.csv

# Execution performance
tail -f execution/logs/performance.jsonl | jq '.'

# Database trades
psql -d pump_trading -U ahmad -c "SELECT * FROM executions ORDER BY timestamp DESC LIMIT 5;"
```

---

## Additional Resources

- **Architecture**: See `FINAL_DOCUMENTATION.md`
- **Pyth Integration**: See `TASK5_PYTH_INTEGRATION.md`
- **Slippage Calculation**: See `TASK7_SLIPPAGE_CALCULATION.md`
- **Thread Pinning**: See `TASK19_THREAD_PINNING_GUIDE.md`
- **Mempool Integration**: See `TASK11_MEMPOOL_VERIFICATION.md`

---

**Last Updated**: October 27, 2025  
**System Status**: Production Ready ✅
