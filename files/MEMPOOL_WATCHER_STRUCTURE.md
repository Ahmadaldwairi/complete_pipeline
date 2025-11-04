# Mempool Watcher Service - Complete Structure

## Overview

The **mempool-watcher** service monitors Solana blockchain transactions in real-time via WebSocket connections, focusing on Pump.fun trading activity. It identifies "alpha wallets" (profitable traders), calculates mempool "heat" (trading activity intensity), and publishes signals to the Brain service via UDP.

**Total Code**: ~2,207 lines across 10 Rust source files

---

## Directory Structure

```
mempool-watcher/
├── Cargo.toml                              # Dependencies: tokio, solana-client, rusqlite, websockets
├── Cargo.lock                              # Dependency lock file
├── .env                                    # Environment configuration (RPC URLs, ports)
├── .env.example                            # Example environment configuration
├── AUDIT_IMPLEMENTATION_COMPLETE.md        # Audit feature implementation docs
├── mempool.log                             # Runtime logs
├── logs/                                   # Log directory
│   └── (runtime logs)
├── src/
│   ├── main.rs                            # 257 lines - Entry point, orchestrates all components
│   ├── config.rs                          # 177 lines - Configuration management (.env loading)
│   ├── decoder.rs                         # 306 lines - Transaction decoder (Pump.fun buy/sell)
│   ├── transaction_monitor.rs             # 278 lines - WebSocket transaction monitoring
│   ├── alpha_wallet_manager.rs            # 242 lines - Tracks profitable wallets (SQLite DB)
│   ├── heat_calculator.rs                 # 401 lines - Calculates mempool heat signals
│   ├── udp_publisher.rs                   # 168 lines - UDP message sender to Brain
│   ├── tx_confirmed.rs                    # 167 lines - Transaction confirmation tracking
│   ├── watch_signature.rs                 # 146 lines - Signature monitoring via RPC
│   ├── watch_listener.rs                  # 65 lines - WebSocket listener setup
│   └── (no subdirectories)
└── target/                                 # Build artifacts
    ├── debug/
    └── release/
```

---

## File Descriptions

### Core Entry Point

#### `src/main.rs` (257 lines)

**Purpose**: Main entry point that orchestrates all mempool-watcher components.

**Key Components**:

- Loads configuration from `.env`
- Initializes SQLite database for alpha wallet tracking
- Spawns multiple async tasks:
  - `TransactionMonitor`: WebSocket transaction stream
  - `AlphaWalletManager`: Wallet performance tracking
  - `HeatCalculator`: Heat signal generation
  - `TxConfirmed`: Transaction confirmation tracking
  - `UdpPublisher`: Message sending to Brain
- Handles graceful shutdown on Ctrl+C

**Recent Changes**: Integrated audit logging for alpha wallet tracking.

---

### Configuration

#### `src/config.rs` (177 lines)

**Purpose**: Configuration management, loads settings from `.env` file.

**Key Settings**:

- `SOLANA_RPC_URL`: RPC endpoint for transaction queries
- `SOLANA_WS_URL`: WebSocket endpoint for real-time monitoring
- `PUMP_PROGRAM_ID`: Pump.fun program address
- `UDP_BRAIN_ADDRESS`: Brain service UDP endpoint (default: `127.0.0.1:8888`)
- `HEAT_WINDOW_SECONDS`: Time window for heat calculation (default: 60s)
- `ALPHA_THRESHOLD_SOL`: Minimum profit for alpha wallet classification (default: 50 SOL)
- Database path for SQLite

**Environment Variables**:

```
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_WS_URL=wss://api.mainnet-beta.solana.com
PUMP_PROGRAM_ID=6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
UDP_BRAIN_ADDRESS=127.0.0.1:8888
HEAT_WINDOW_SECONDS=60
ALPHA_THRESHOLD_SOL=50.0
```

---

### Transaction Decoding

#### `src/decoder.rs` (306 lines)

**Purpose**: Decodes Solana transactions to identify Pump.fun buy/sell actions.

**Key Structures**:

- `DecodedTransaction`: Parsed transaction with action, wallet, token, amount
- `TransactionAction`: Buy, Sell, Unknown
- `WalletType`: Alpha, Regular, Unknown
- `ProgramType`: Pump, Raydium, Jupiter, Unknown
- `PumpBuyInstruction`: Discriminator `66063d1201daebea` (8 bytes)
- `PumpSellInstruction`: Discriminator `33e685a4017f83ad` (8 bytes)

**Key Functions**:

- `decode_transaction()`: Parses transaction, identifies Pump.fun instructions
- `decode_pump_buy()`: Extracts buy instruction data (max SOL, token amount)
- `decode_pump_sell()`: Extracts sell instruction data (token amount, min SOL)
- `is_pump_program()`: Checks if transaction involves Pump.fun program

**Instruction Format**:

- First 8 bytes: Discriminator (identifies buy/sell)
- Remaining bytes: Instruction data (amounts, tokens)
- Accounts: Includes user wallet, token mint, bonding curve

**Tests**: 280+ lines of unit tests for decoder logic.

---

### Transaction Monitoring

#### `src/transaction_monitor.rs` (278 lines)

**Purpose**: WebSocket-based real-time transaction monitoring.

**Key Components**:

- `TransactionMonitor`: Main monitoring structure
- Connects to Solana WebSocket (`logsSubscribe`)
- Filters transactions by Pump.fun program ID
- Fetches full transaction details via RPC
- Decodes transactions and forwards to `AlphaWalletManager` and `HeatCalculator`

**Subscription Filter**:

```json
{
  "mentions": ["6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"]
}
```

**Key Functions**:

- `start()`: Begins WebSocket monitoring loop
- `process_transaction()`: Fetches and decodes transaction
- `handle_decoded_tx()`: Routes decoded transaction to other components

**Message Flow**:

1. WebSocket receives transaction signature
2. Fetch full transaction via RPC
3. Decode transaction (buy/sell/unknown)
4. Send to `AlphaWalletManager` for wallet tracking
5. Send to `HeatCalculator` for heat signal generation

---

### Alpha Wallet Tracking

#### `src/alpha_wallet_manager.rs` (242 lines)

**Purpose**: Tracks wallet performance, identifies profitable "alpha" wallets.

**Key Components**:

- `AlphaWalletManager`: Main wallet tracking structure
- SQLite database for persistent storage
- Calculates profit/loss for each wallet
- Classifies wallets as "alpha" when profit exceeds threshold

**Database Schema**:

```sql
CREATE TABLE wallets (
    address TEXT PRIMARY KEY,
    total_buys INTEGER,
    total_sells INTEGER,
    total_volume_sol REAL,
    total_profit_sol REAL,
    first_seen_at INTEGER,
    last_seen_at INTEGER,
    is_alpha INTEGER  -- 0 or 1
);

CREATE TABLE trades (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    wallet_address TEXT,
    token_mint TEXT,
    action TEXT,  -- 'buy' or 'sell'
    amount_sol REAL,
    amount_tokens REAL,
    timestamp INTEGER,
    signature TEXT
);
```

**Key Functions**:

- `record_transaction()`: Updates wallet stats on each trade
- `update_wallet_alpha_status()`: Classifies wallet as alpha if profit > threshold
- `get_alpha_wallets()`: Returns list of all alpha wallets
- `get_wallet_stats()`: Returns performance stats for a wallet

**Alpha Classification**:

- Tracks buy/sell pairs for profit calculation
- If `total_profit_sol >= ALPHA_THRESHOLD_SOL` (default 50 SOL), wallet becomes "alpha"
- Alpha wallets trigger special signals to Brain service

**Recent Changes**: Added audit logging for alpha wallet classification events.

---

### Heat Calculation

#### `src/heat_calculator.rs` (401 lines)

**Purpose**: Calculates mempool "heat" (trading activity intensity) and generates hot signals.

**Key Components**:

- `HeatCalculator`: Main heat calculation structure
- `HeatIndex`: Sliding window of recent transactions
- `HotSignal`: Represents a hot trading signal for a token

**Heat Calculation Logic**:

- Tracks transactions in sliding time window (default 60 seconds)
- Calculates metrics per token:
  - `tx_count`: Number of transactions
  - `buy_count` / `sell_count`: Buy/sell split
  - `total_volume_sol`: Total SOL volume
  - `unique_wallets`: Number of unique traders
  - `alpha_wallet_count`: Number of alpha wallets trading this token
- **Heat score formula**: `(tx_count * volume_weight) + (alpha_count * alpha_weight)`

**Hot Signal Generation**:

- Token is "hot" if:
  - `tx_count >= min_tx_threshold` (default: 10 txs)
  - `heat_score >= hot_threshold` (default: 100)
  - `alpha_wallet_count > 0` (at least one alpha wallet trading)
- Hot signals are sent to Brain via UDP

**Key Structures**:

```rust
pub struct HeatIndex {
    pub token_mint: String,
    pub tx_count: u64,
    pub buy_count: u64,
    pub sell_count: u64,
    pub total_volume_sol: f64,
    pub unique_wallets: HashSet<String>,
    pub alpha_wallet_count: u64,
    pub heat_score: f64,
    pub window_start: Instant,
}

pub struct HotSignal {
    pub token_mint: String,
    pub heat_score: f64,
    pub tx_count: u64,
    pub alpha_count: u64,
    pub volume_sol: f64,
    pub timestamp: u64,
}
```

**Key Functions**:

- `record_transaction()`: Adds transaction to heat index
- `calculate_heat()`: Computes heat score for all tokens
- `get_hot_signals()`: Returns tokens exceeding hot threshold
- `cleanup_old_windows()`: Removes expired data from sliding window

**UDP Message Sent**:

- Message type: `MempoolHeatAdvice` (type ID: varies by protocol)
- Payload: `HotSignal` serialized to JSON
- Sent to Brain at `UDP_BRAIN_ADDRESS`

---

### UDP Publishing

#### `src/udp_publisher.rs` (168 lines)

**Purpose**: Sends UDP messages to Brain service with heat signals.

**Key Components**:

- `UdpPublisher`: Main UDP sender structure
- Socket bound to local address
- Sends to Brain at configured address (default: `127.0.0.1:8888`)

**Message Types Sent**:

1. **MempoolHeatAdvice**: Hot signal for a token

   - Includes: token_mint, heat_score, tx_count, alpha_count, volume_sol
   - Frequency: When token becomes "hot" (exceeds thresholds)

2. **AlphaWalletSignal**: Notification when alpha wallet trades
   - Includes: wallet_address, token_mint, action (buy/sell), amount
   - Frequency: On every alpha wallet transaction

**Message Format** (Binary UDP):

```
[4 bytes: message_type] [payload_bytes: JSON or bincode]
```

**Key Functions**:

- `send_hot_signal()`: Sends hot signal for a token
- `send_alpha_wallet_signal()`: Sends alpha wallet activity notification
- `publish()`: Generic function to send any message type

**Error Handling**:

- Logs errors if UDP send fails
- Does not block on send failures (fire-and-forget)

---

### Transaction Confirmation

#### `src/tx_confirmed.rs` (167 lines)

**Purpose**: Tracks transaction confirmations via RPC polling.

**Key Components**:

- `TxConfirmed`: Main confirmation tracker
- Polls RPC endpoint for transaction status
- Notifies when transactions are confirmed on-chain

**Key Functions**:

- `track_signature()`: Adds signature to tracking list
- `check_confirmations()`: Polls RPC for confirmation status
- `on_confirmed()`: Callback when transaction is confirmed

**Confirmation Criteria**:

- Transaction must be finalized (not just processed)
- Checks for errors in transaction result
- Retries up to N times before giving up

**Use Case**: Used to verify that monitored transactions actually land on-chain (not just seen in mempool).

**Tests**: Unit tests for confirmation logic.

---

### Signature Monitoring

#### `src/watch_signature.rs` (146 lines)

**Purpose**: Monitors specific transaction signatures via RPC.

**Key Components**:

- `WatchSignature`: Signature watcher structure
- `SignatureTracker`: Tracks multiple signatures
- Polls RPC for signature status

**Key Functions**:

- `watch()`: Starts watching a signature
- `poll_status()`: Checks signature confirmation status
- `is_confirmed()`: Returns whether signature is confirmed

**Use Case**: Used by `tx_confirmed.rs` to track individual signatures.

---

### WebSocket Listener

#### `src/watch_listener.rs` (65 lines)

**Purpose**: WebSocket connection setup and management.

**Key Functions**:

- `connect_websocket()`: Establishes WebSocket connection
- `subscribe_logs()`: Subscribes to transaction logs
- `handle_message()`: Processes incoming WebSocket messages

**Error Handling**:

- Reconnects on disconnect
- Retries on connection failure

---

## Message Flow Diagram

```
Solana Blockchain
    │
    ├─> WebSocket (logsSubscribe) ────> TransactionMonitor
    │                                          │
    │                                          ├─> Decoder (decode_transaction)
    │                                          │        │
    │                                          │        ├─> AlphaWalletManager (record_transaction)
    │                                          │        │        │
    │                                          │        │        ├─> SQLite (update wallet stats)
    │                                          │        │        └─> Check alpha status
    │                                          │        │
    │                                          │        └─> HeatCalculator (record_transaction)
    │                                          │                 │
    │                                          │                 ├─> Calculate heat score
    │                                          │                 └─> Generate hot signals
    │                                          │
    │                                          └─> TxConfirmed (track_signature)
    │                                                   │
    └─> RPC (getTransaction) ──────────────────────────┘
                                                        │
                                                        └─> UdpPublisher
                                                                 │
                                                                 └─> Brain Service (UDP:8888)
                                                                      ├─> MempoolHeatAdvice
                                                                      └─> AlphaWalletSignal
```

---

## Key Dependencies (from Cargo.toml)

### Core

- `tokio` (v1): Async runtime
- `anyhow` (v1.0): Error handling

### Solana

- `solana-client` (v1.18): RPC client
- `solana-sdk` (v1.18): Solana primitives
- `solana-transaction-status` (v1.18): Transaction status types

### Database

- `rusqlite` (v0.31): SQLite database (alpha wallet tracking)

### Networking

- `tokio-tungstenite` (v0.21): WebSocket client
- `reqwest` (v0.12): HTTP client

### Data Structures

- `dashmap` (v5.5): Concurrent HashMap (for heat index)
- `serde` (v1.0): Serialization
- `bincode` (v1.3): Binary serialization

---

## Recent Changes

### Audit Feature (AUDIT_IMPLEMENTATION_COMPLETE.md)

- Added audit logging for alpha wallet classification events
- Logs when wallets cross alpha threshold
- Logs wallet performance stats (total profit, trade counts)
- Audit logs stored in `logs/audit.log`

---

## Integration with Brain Service

The mempool-watcher sends two main UDP message types to Brain:

### 1. MempoolHeatAdvice

**When**: Token becomes "hot" (high trading activity + alpha wallets)
**Payload**:

```json
{
  "token_mint": "TokenAddress...",
  "heat_score": 150.5,
  "tx_count": 25,
  "alpha_count": 3,
  "volume_sol": 1234.56,
  "timestamp": 1698765432
}
```

### 2. AlphaWalletSignal

**When**: Alpha wallet executes a trade
**Payload**:

```json
{
  "wallet_address": "WalletAddress...",
  "token_mint": "TokenAddress...",
  "action": "buy",
  "amount_sol": 10.5,
  "timestamp": 1698765432
}
```

Brain uses these signals to:

- Prioritize tokens with high heat (more likely to be profitable)
- Follow alpha wallet trades (copy trading signals)
- Adjust trading strategies based on mempool activity

---

## Configuration Example

`.env` file:

```bash
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_WS_URL=wss://api.mainnet-beta.solana.com
PUMP_PROGRAM_ID=6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
UDP_BRAIN_ADDRESS=127.0.0.1:8888
HEAT_WINDOW_SECONDS=60
ALPHA_THRESHOLD_SOL=50.0
DB_PATH=./alpha_wallets.db
LOG_LEVEL=info
```

---

## Build and Run

```bash
# Build release version
cd mempool-watcher
cargo build --release

# Run
./target/release/mempool-watcher

# Or with cargo
cargo run --release
```

---

## Key Metrics

- **Total Files**: 10 Rust source files
- **Total Lines**: ~2,207 lines
- **Largest Module**: `heat_calculator.rs` (401 lines)
- **Database**: SQLite for alpha wallet persistence
- **UDP Messages**: 2 types sent to Brain
- **WebSocket**: Real-time Solana transaction monitoring
- **RPC Calls**: Transaction fetching and confirmation checking

---

## Summary

The **mempool-watcher** service is the intelligence layer that monitors Solana blockchain activity, identifies profitable trading patterns, and provides real-time signals to the Brain service. It combines:

1. **Real-time monitoring** (WebSocket transaction stream)
2. **Pattern recognition** (alpha wallet identification)
3. **Activity analysis** (heat calculation)
4. **Signal generation** (hot tokens, alpha trades)

This enables the trading bot to make informed decisions based on actual on-chain activity, not just price data.
