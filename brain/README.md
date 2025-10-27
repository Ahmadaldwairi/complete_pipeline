# 🧠 Brain Service (Decision Engine)

Intelligent decision-making layer for the Solana trading bot ecosystem. The Brain receives live market data and wallet intelligence, applies sophisticated scoring and validation, then produces validated trade decisions for execution.

## 📋 Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Features](#features)
- [Message Flow](#message-flow)
- [Module Documentation](#module-documentation)
- [Configuration](#configuration)
- [Getting Started](#getting-started)
- [Usage](#usage)
- [Testing](#testing)
- [Performance](#performance)
- [Dependencies](#dependencies)
- [Troubleshooting](#troubleshooting)

## Overview

The Brain service is the **central intelligence** of the trading bot system. It sits between the analysis bots (RankBot, AdvisorBot) and the execution bot, making the critical go/no-go decisions for each trading opportunity.

### Key Responsibilities

1. **Receive advice messages** via UDP from RankBot and AdvisorBot
2. **Cache token and wallet features** from PostgreSQL and SQLite databases
3. **Score opportunities** using follow-through algorithm (0-100)
4. **Validate trades** against 9 pre-trade checks
5. **Apply guardrails** to prevent overtrading and loss spirals
6. **Log all decisions** to CSV for post-analysis
7. **Send trade decisions** via UDP to ExecutionBot

### Why "Brain"?

The Brain makes **informed decisions** rather than blindly executing every signal. It combines multiple data sources, applies risk management, and learns from patterns to maximize profitability while minimizing losses.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        BRAIN SERVICE                             │
│                                                                   │
│  ┌──────────────────┐         ┌──────────────────┐             │
│  │  UDP Bus Layer   │         │  Feature Caches   │             │
│  │                  │         │                   │             │
│  │  • Advice Bus    │◄────────┤  • Mint Cache    │             │
│  │    (port 45100)  │         │  • Wallet Cache  │             │
│  │  • Decision Bus  │         │                   │             │
│  │    (port 45110)  │         └────────┬──────────┘             │
│  └────────┬─────────┘                  │                        │
│           │                            │                        │
│           ▼                            ▼                        │
│  ┌─────────────────────────────────────────────┐               │
│  │         Decision Engine Core                 │               │
│  │                                               │               │
│  │  1. Trigger Detection (4 types)              │               │
│  │     • Rank-based (top 2, score ≥60)         │               │
│  │     • Momentum (buyers ≥5, vol ≥8 SOL)      │               │
│  │     • Copy Trade (tier ≥C, size ≥0.25 SOL)  │               │
│  │     • Late Opportunity (age >20min)          │               │
│  │                                               │               │
│  │  2. Follow-Through Scoring (0-100)           │               │
│  │     • Buyer count (40% weight)               │               │
│  │     • Volume depth (35% weight)              │               │
│  │     • Time decay (25% weight)                │               │
│  │                                               │               │
│  │  3. Pre-Trade Validation (9 checks)          │               │
│  │     ✓ Launch not too young/old               │               │
│  │     ✓ Sufficient liquidity                   │               │
│  │     ✓ Fees under threshold                   │               │
│  │     ✓ Impact acceptable                      │               │
│  │     ✓ Confidence meets minimum               │               │
│  │     ✓ Follow-through score adequate          │               │
│  │     ✓ Position size reasonable               │               │
│  │     ✓ Wallet tier sufficient (copytrading)   │               │
│  │     ✓ Expected value positive                │               │
│  │                                               │               │
│  │  4. Guardrails (Anti-Churn)                  │               │
│  │     • Position limits (max 3, max 2 advisor) │               │
│  │     • Rate limiting (100ms general, 30s adv) │               │
│  │     • Loss backoff (3 losses → 2min pause)   │               │
│  │     • Wallet cooling (90s between copies)    │               │
│  │                                               │               │
│  │  5. Decision Logging (CSV)                   │               │
│  │     • 17 fields per decision                 │               │
│  │     • Timestamp, mint, trigger type          │               │
│  │     • Validation metrics, EV calculation     │               │
│  └─────────────────────────────────────────────┘               │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘

         ▲                                    ▼
         │                                    │
    UDP port 45100                       UDP port 45110
         │                                    │
         │                                    │
┌────────┴────────┐                  ┌────────┴────────┐
│   RankBot /     │                  │  ExecutionBot   │
│   AdvisorBot    │                  │                 │
│                 │                  │  • Executes     │
│  • Rank advice  │                  │    trades       │
│  • Copy signals │                  │  • Manages      │
│  • Late opps    │                  │    positions    │
└─────────────────┘                  └─────────────────┘
```

## Features

### 🎯 Entry Trigger Detection

Four distinct trigger pathways for entering trades:

| Trigger Type | Criteria | Use Case |
|-------------|----------|----------|
| **Rank** | Rank ≤ 2, Score ≥ 60 | Top-ranked opportunities |
| **Momentum** | Buyers ≥ 5, Volume ≥ 8 SOL | High activity launches |
| **Copy Trade** | Tier ≥ C, Size ≥ 0.25 SOL | Follow profitable wallets |
| **Late Opportunity** | Age > 20min, Sustained activity | Mature launches |

### 📊 Follow-Through Scoring

Proprietary algorithm scoring tokens 0-100 based on:
- **Buyer Count** (40%): More unique buyers = stronger interest
- **Volume Depth** (35%): Higher volume = better liquidity
- **Time Decay** (25%): Recent activity weighted higher

### ✅ Pre-Trade Validation

9 comprehensive checks before approving any trade:

1. **Launch Age**: Not too young (<30s) or too old (>4h)
2. **Liquidity**: Minimum $5,000 USD depth
3. **Fees**: Below 2.2x multiplier of TP target
4. **Impact**: Price impact < 45% of take-profit
5. **Confidence**: Score meets minimum threshold
6. **Follow-Through**: Sustained buyer activity
7. **Position Size**: Within reasonable bounds
8. **Wallet Tier**: Sufficient for copy trades
9. **Expected Value**: Positive EV calculation

### 🛡️ Guardrails (Anti-Churn Protection)

Prevents overtrading and loss spirals:

- **Position Limits**: Max 3 concurrent, max 2 from advisors
- **Rate Limiting**: 100ms between decisions, 30s for copy trades
- **Loss Backoff**: 3 losses in 3 minutes triggers 2-minute pause
- **Wallet Cooling**: 90 seconds between copying same wallet (bypassed for Tier A if profitable)

### 📝 Decision Logging

Every decision logged to CSV with 17 fields:
```csv
decision_id,timestamp,mint,trigger_type,side,predicted_fees_usd,predicted_impact_usd,tp_usd,follow_through_score,size_sol,size_usd,confidence,expected_ev_usd,success_probability,rank,wallet,wallet_tier,datetime
```

Perfect for post-analysis, backtesting, and strategy optimization.

### ⚡ Feature Caches

Lightning-fast in-memory caches with DashMap:

- **Mint Cache**: Token launch data, volume, buyers, age
- **Wallet Cache**: Wallet performance, win rate, tier classification
- **Target Performance**: <50µs read times, <5 seconds refresh

## Message Flow

### 1. Receiving Advice (UDP Port 45100)

Brain listens for 5 advice message types:

```rust
pub enum AdviceMessage {
    ExtendHold,         // RankBot: hold position longer
    WidenExit,          // RankBot: adjust exit strategy
    LateOpportunity,    // RankBot: mature launch opportunity
    CopyTrade,          // AdvisorBot: copy profitable wallet
    SolPriceUpdate,     // Price oracle: SOL/USD rate
}
```

### 2. Processing Pipeline

```
Advice Message → Trigger Detection → Feature Lookup → Scoring → Validation → Guardrails → Decision
```

### 3. Sending Decisions (UDP Port 45110)

Brain sends `TradeDecision` messages to ExecutionBot:

```rust
pub struct TradeDecision {
    pub mint: [u8; 32],           // Token to trade
    pub side: u8,                 // 0=BUY, 1=SELL
    pub size_lamports: u64,       // Position size
    pub slippage_bps: u16,        // Slippage tolerance
    pub confidence: u8,           // 0-100 confidence
    pub trigger: u8,              // Entry trigger type
    pub timestamp: u64,           // Decision timestamp
}
```

## Module Documentation

### `udp_bus/`

UDP communication layer for inter-bot messaging.

**Files:**
- `messages.rs` (587 lines): Message struct definitions
  - `TradeDecision` (52 bytes)
  - `HeatPulse` (64 bytes)
  - 5 advice message types
- `sender.rs` (253 lines): Decision Bus sender with retry logic
- `receiver.rs` (240 lines): Advice Bus receiver with statistics

**Key Features:**
- Fixed-size binary messages for performance
- Retry logic with exponential backoff
- Message statistics tracking
- Thread-safe UDP sockets

### `feature_cache/`

In-memory caching layer for database features.

**Files:**
- `mint_cache.rs`: Token launch data cache
  - Buyers count, volume, liquidity
  - Launch timestamp, age calculation
  - Auto-refresh from SQLite
- `wallet_cache.rs`: Wallet performance cache
  - Win rate (7d), realized PnL
  - Tier classification (A/B/C)
  - Last trade tracking

**Key Features:**
- DashMap for lock-free concurrent access
- Configurable capacity (default 10k mints, 5k wallets)
- Background refresh every 30 seconds
- <50µs read performance

### `decision_engine/`

Core decision-making logic.

**Files:**
- `scoring.rs`: Follow-through scoring algorithm
  - Weighted 3-component score (0-100)
  - Configurable thresholds
- `validation.rs` (598 lines): Pre-trade validation
  - 9 comprehensive checks
  - Clear error messages
  - Fee/impact calculations
- `triggers.rs` (674 lines): Entry trigger detection
  - 4 trigger pathways
  - Trigger-specific logic
- `guardrails.rs` (462 lines): Anti-churn protection
  - Position tracking
  - Rate limiting
  - Loss backoff
  - Wallet cooling
- `logging.rs` (462 lines): Decision logging
  - CSV file writer
  - 17-field records
  - Builder pattern API

**Key Features:**
- Modular, testable design
- Comprehensive unit tests (77 total)
- Type-safe validation errors
- Thread-safe state management

### `config.rs`

Configuration management system.

**Features:**
- Environment variable loading (.env file)
- Type-safe parsing for all parameters
- Validation with clear error messages
- Default values for all settings
- 8 configuration sections

See [CONFIG.md](CONFIG.md) for complete configuration guide.

## Configuration

### Quick Start

```bash
# Copy example configuration
cp .env.example .env

# Edit with your values
nano .env

# Key settings to change:
# - POSTGRES_PASSWORD: Your PostgreSQL password
# - POSTGRES_HOST: Database host (if not localhost)
# - MIN_DECISION_CONF: Confidence threshold (default 75)
```

### Key Parameters

```env
# Decision thresholds
MIN_DECISION_CONF=75              # Minimum confidence for trades
MIN_COPYTRADE_CONFIDENCE=70       # Minimum for copy trades
MIN_FOLLOW_THROUGH_SCORE=55       # Minimum activity score

# Validation
FEE_MULTIPLIER=2.2                # Fee estimation multiplier
IMPACT_CAP_MULTIPLIER=0.45        # Max impact as fraction of TP

# Guardrails
MAX_CONCURRENT_POSITIONS=3        # Total position limit
RATE_LIMIT_MS=100                 # Milliseconds between decisions

# Network
ADVICE_BUS_PORT=45100             # Receives advice
DECISION_BUS_PORT=45110           # Sends decisions
```

See [CONFIG.md](CONFIG.md) for complete documentation.

## Getting Started

### Prerequisites

- **Rust** 1.70+ (`rustc --version`)
- **PostgreSQL** 13+ (for WalletTracker data)
- **SQLite** 3.35+ (for LaunchTracker data)

### Installation

```bash
# Clone repository
cd /path/to/scalper-bot/brain

# Install dependencies
cargo build

# Run tests
cargo test

# Build release version
cargo build --release
```

### Database Setup

#### PostgreSQL (WalletTracker)

```bash
# Create database and user
createdb wallet_tracker
psql -d wallet_tracker << SQL
CREATE USER trader WITH PASSWORD 'your_password';
GRANT ALL PRIVILEGES ON DATABASE wallet_tracker TO trader;
SQL

# Update .env
POSTGRES_HOST=localhost
POSTGRES_PORT=5432
POSTGRES_USER=trader
POSTGRES_PASSWORD=your_password
POSTGRES_DB=wallet_tracker
```

#### SQLite (LaunchTracker)

```bash
# Ensure data directory exists
mkdir -p ./data

# LaunchTracker bot will create the database automatically
# Just set the path in .env:
SQLITE_PATH=./data/launch_tracker.db
```

### Running

```bash
# Development mode (with logs)
cargo run

# Release mode (optimized)
cargo run --release

# With custom log level
LOG_LEVEL=debug cargo run

# Background process
nohup cargo run --release > brain.log 2>&1 &
```

## Usage

### Basic Example

```rust
use decision_engine::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = Config::from_env()?;
    config.validate()?;
    
    println!("🧠 Brain Service starting...");
    println!("Min confidence: {}", config.decision.min_decision_conf);
    println!("Max positions: {}", config.guardrails.max_concurrent_positions);
    
    // Initialize components
    // ... (see main.rs for full implementation)
    
    Ok(())
}
```

### Logging Decisions

```rust
use decision_engine::logging::{DecisionLogger, DecisionLogBuilder, TriggerType};

// Initialize logger
let logger = DecisionLogger::new("./data/decisions.csv")?;

// Log a decision
let entry = DecisionLogBuilder::new(mint, TriggerType::Rank, 0)
    .validation(0.52, 0.31, 2.15)  // fees, impact, TP
    .score(78)                      // follow-through score
    .position(0.75, 150.0, 85)     // size, USD value, confidence
    .ev(1.63, 0.68)                // expected value, probability
    .rank(1)                        // rank #1
    .build();

logger.log_decision(entry)?;
```

### Analyzing Logs

```python
import pandas as pd

# Load decision log
df = pd.read_csv('data/decisions.csv')

# Success rate by trigger type
df.groupby('trigger_type')['success_probability'].mean()

# Average EV by confidence level
df.groupby(pd.cut(df['confidence'], bins=[0,70,85,100]))['expected_ev_usd'].mean()

# Top performing copy trades
df[df['trigger_type']=='copy'].groupby('wallet')['expected_ev_usd'].sum().sort_values(ascending=False)
```

## Testing

### Run All Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific module
cargo test decision_engine::

# Run sequentially (avoids env var conflicts)
cargo test -- --test-threads=1
```

### Test Coverage

**77 tests total:**
- Config: 8 tests
- Validation: 31 tests
- Triggers: 9 tests
- Guardrails: 4 tests
- Logging: 7 tests
- Scoring: 10 tests
- Messages: 4 tests
- Sender: 6 tests
- Receiver: 2 tests

### Integration Testing

```bash
# Test with actual databases (requires setup)
cargo test --features integration

# Test UDP communication
cargo test udp_bus::

# Performance tests
cargo test --release -- --ignored
```

## Performance

### Target Metrics

| Metric | Target | Typical |
|--------|--------|---------|
| Cache Read | <50µs | 15-30µs |
| Validation | <1ms | 200-500µs |
| Decision Latency | <5ms | 1-3ms |
| Throughput | >100 decisions/sec | 200-300/sec |
| Memory Usage | <100MB | 50-80MB |

### Optimization Tips

1. **Increase cache capacity** if hit rate < 90%
2. **Adjust refresh interval** based on data staleness tolerance
3. **Monitor validation times** - most expensive checks first
4. **Use release build** for production (10x faster than debug)
5. **Tune worker threads** based on CPU cores

### Monitoring

```bash
# Check decision log size
wc -l data/brain_decisions.csv

# Monitor memory usage
ps aux | grep decision_engine

# Check UDP ports
netstat -tulpn | grep -E "45100|45110"

# Real-time logs
tail -f brain.log
```

## Dependencies

### Databases

**PostgreSQL (WalletTracker)**
- Stores wallet performance history
- Win rates, PnL, tier classifications
- Must be running and accessible

**SQLite (LaunchTracker)**
- Stores token launch data
- Volume, buyers, liquidity metrics
- File-based, no separate server needed

### Other Services

**RankBot** (port 45100)
- Sends rank-based opportunities
- Late opportunity signals
- Position adjustment advice

**AdvisorBot** (port 45100)
- Sends copy trade signals
- Wallet tier classifications
- Real-time wallet tracking

**ExecutionBot** (port 45110)
- Receives trade decisions
- Executes on Solana blockchain
- Manages position lifecycle

### Network Topology

```
[PostgreSQL]     [SQLite]
     ▲              ▲
     │              │
     └──────┬───────┘
            │
         [Brain]
            │
     ┌──────┴──────┐
     │             │
[RankBot]    [ExecutionBot]
[AdvisorBot]
```

## Troubleshooting

### "Failed to bind UDP socket"

```bash
# Check if port is in use
netstat -tulpn | grep 45100

# Kill process using port
lsof -ti:45100 | xargs kill -9

# Change port in .env
ADVICE_BUS_PORT=45200
```

### "PostgreSQL connection failed"

```bash
# Verify PostgreSQL is running
systemctl status postgresql

# Test connection
psql -h localhost -U trader -d wallet_tracker

# Check credentials in .env
POSTGRES_PASSWORD=your_actual_password
```

### "SQLite database not found"

```bash
# Create data directory
mkdir -p ./data

# Check path in .env
SQLITE_PATH=./data/launch_tracker.db

# Verify LaunchTracker is running and creating DB
```

### "Too many validation failures"

```bash
# Check thresholds in .env
MIN_DECISION_CONF=75  # Lower for more trades
FEE_MULTIPLIER=2.2    # Increase if fees underestimated
IMPACT_CAP_MULTIPLIER=0.45  # Increase if too restrictive

# Check decision log for specific errors
tail -100 data/brain_decisions.csv
```

### "Loss backoff triggered too often"

```bash
# Adjust guardrails in .env
LOSS_BACKOFF_THRESHOLD=5      # More losses before pause
LOSS_BACKOFF_WINDOW_SECS=300  # Longer time window
LOSS_BACKOFF_PAUSE_SECS=60    # Shorter pause

# Or disable temporarily for testing
LOSS_BACKOFF_THRESHOLD=999
```

## Contributing

### Code Style

- Follow Rust standard style (`cargo fmt`)
- Run Clippy before committing (`cargo clippy`)
- Add tests for new features
- Document public APIs

### Testing

```bash
# Format code
cargo fmt

# Check for issues
cargo clippy

# Run tests
cargo test

# Check test coverage
cargo tarpaulin --out Html
```

### Pull Requests

1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Ensure all tests pass
5. Submit PR with clear description

## License

Proprietary - All rights reserved

## Contact

For questions or support, contact the development team.

---

Built with ⚡ by the Solana Trading Bot Team
