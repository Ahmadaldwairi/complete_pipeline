# Data-Mining Bot - Comprehensive Reference

**Version**: 1.0  
**Purpose**: Real-time Pump.fun transaction collection, parsing, storage, and advisory generation  
**Language**: Rust  
**Dependencies**: gRPC (Yellowstone), SQLite, UDP networking

---

## High-Level Overview

The Data-Mining bot is the **data collection and initial analysis layer** of the trading system. It connects to a Solana validator's gRPC stream (Yellowstone/Geyser) to receive real-time transaction data, specifically monitoring Pump.fun trading activity.

**Core Responsibilities**:

1. **Listen to gRPC stream** from Solana validator for all Pump.fun transactions
2. **Parse transactions** to extract buy/sell trades, token launches, and wallet activity
3. **Store data** in SQLite database (trades, tokens, windows, wallet stats, positions)
4. **Aggregate metrics** into time windows (2s, 5s, 60s candles)
5. **Track wallet performance** (win rate, PnL, position tracking)
6. **Generate UDP advisories** to Brain when opportunities are detected
7. **Broadcast SOL/USD price** updates via UDP

**Data Flow**:

```
Solana Validator (gRPC)
    → Data-Mining (parse & store)
    → SQLite Database (collector.db)
    → Window Aggregation (2s/5s/60s)
    → Opportunity Detection
    → UDP Advisory → Brain (port 45100)
```

---

## Database Schema

### Primary Database: `collector.db` (SQLite)

#### 1. **tokens** table

Stores all discovered Pump.fun token launches.

```sql
CREATE TABLE tokens (
    mint TEXT PRIMARY KEY,                  -- Token mint address (32-byte pubkey)
    creator_wallet TEXT NOT NULL,           -- Wallet that created the token
    bonding_curve_addr TEXT,                -- Pump.fun bonding curve address
    name TEXT,                              -- Token name
    symbol TEXT,                            -- Token symbol/ticker
    uri TEXT,                               -- Metadata URI
    decimals INTEGER NOT NULL,              -- Token decimals (usually 6)
    launch_tx_sig TEXT NOT NULL,            -- Transaction signature of launch
    launch_slot INTEGER NOT NULL,           -- Slot number when launched
    launch_block_time INTEGER NOT NULL,     -- Unix timestamp of launch
    initial_price REAL,                     -- Price at launch (SOL per token)
    initial_liquidity_sol REAL,             -- Initial SOL liquidity
    initial_supply TEXT,                    -- Total supply (as string, large number)
    market_cap_init REAL,                   -- Initial market cap in SOL
    mint_authority TEXT,                    -- Mint authority address
    freeze_authority TEXT,                  -- Freeze authority address
    metadata_update_auth TEXT,              -- Metadata update authority
    migrated_to_raydium INTEGER DEFAULT 0,  -- 1 if migrated to Raydium DEX
    migration_slot INTEGER,                 -- Slot when migrated
    migration_block_time INTEGER,           -- Timestamp when migrated
    raydium_pool TEXT,                      -- Raydium pool address if migrated
    observed_at INTEGER NOT NULL            -- When data-mining first saw it
);

-- Indexes
CREATE INDEX idx_tokens_launch_time ON tokens(launch_block_time);
```

**Purpose**: Track all token launches, metadata, and migration events. Used by Brain to fetch token details.

---

#### 2. **trades** table

Stores every buy/sell transaction for Pump.fun tokens.

```sql
CREATE TABLE trades (
    sig TEXT PRIMARY KEY,                   -- Transaction signature (unique)
    slot INTEGER NOT NULL,                  -- Slot number
    block_time INTEGER NOT NULL,            -- Unix timestamp
    mint TEXT NOT NULL,                     -- Token being traded
    side TEXT CHECK(side IN ('buy', 'sell')) NOT NULL,  -- Trade direction
    trader TEXT NOT NULL,                   -- Wallet executing trade
    amount_tokens REAL NOT NULL,            -- Number of tokens traded
    amount_sol REAL NOT NULL,               -- SOL amount (spent for buy, received for sell)
    price REAL NOT NULL,                    -- Price per token (SOL/token)
    is_amm INTEGER DEFAULT 0,               -- 1 if AMM trade, 0 if bonding curve
    FOREIGN KEY(mint) REFERENCES tokens(mint)
);

-- Indexes
CREATE INDEX idx_trades_mint_time ON trades(mint, block_time, slot);
CREATE INDEX idx_trades_trader_time ON trades(trader, block_time, slot);
CREATE INDEX idx_trades_slot ON trades(slot);
```

**Purpose**: Historical record of all trades. Used for:

- Window aggregation (volume, buyers, price action)
- Wallet performance tracking
- Backfilling analysis

**Typical Row**:

```
sig: "5xK7mP9..."
slot: 267728522
block_time: 1730144050
mint: "2u1767RX2yPj..."
side: "buy"
trader: "9xZd4..."
amount_tokens: 1234567.89
amount_sol: 0.0202
price: 0.0000163500
```

---

#### 3. **windows** table

Time-windowed aggregated metrics per token.

```sql
CREATE TABLE windows (
    mint TEXT NOT NULL,                     -- Token mint address
    window_sec INTEGER NOT NULL,            -- Window size (2, 5, or 60 seconds)
    start_slot INTEGER NOT NULL,            -- Starting slot of window
    start_time INTEGER NOT NULL,            -- Unix timestamp start
    end_time INTEGER NOT NULL,              -- Unix timestamp end
    num_buys INTEGER DEFAULT 0,             -- Number of buy transactions
    num_sells INTEGER DEFAULT 0,            -- Number of sell transactions
    uniq_buyers INTEGER DEFAULT 0,          -- Unique buyer wallets
    vol_tokens REAL DEFAULT 0.0,            -- Total token volume
    vol_sol REAL DEFAULT 0.0,               -- Total SOL volume
    high REAL DEFAULT 0.0,                  -- Highest price in window
    low REAL DEFAULT 0.0,                   -- Lowest price in window
    close REAL DEFAULT 0.0,                 -- Closing price
    vwap REAL DEFAULT 0.0,                  -- Volume-weighted average price
    top1_share REAL DEFAULT 0.0,            -- Largest buyer's % of volume
    top3_share REAL DEFAULT 0.0,            -- Top 3 buyers' % of volume
    top5_share REAL DEFAULT 0.0,            -- Top 5 buyers' % of volume
    PRIMARY KEY(mint, window_sec, start_time)
);

-- Indexes
CREATE INDEX idx_windows_mint_start ON windows(mint, start_time);
```

**Purpose**: Candlestick/OHLC data for opportunity detection. Brain queries these for momentum analysis.

**Window Sizes**:

- **2s**: Ultra-short term momentum (buyer count, rapid price moves)
- **5s**: Short-term volume spikes
- **60s**: Medium-term trends, maturity detection

**Example Row**:

```
mint: "2u1767RX..."
window_sec: 5
start_time: 1730144050
end_time: 1730144055
num_buys: 12
num_sells: 3
uniq_buyers: 8
vol_sol: 3.45
close: 0.0000175200
vwap: 0.0000168500
```

---

#### 4. **wallet_stats** table

Aggregated performance metrics per wallet.

```sql
CREATE TABLE wallet_stats (
    wallet TEXT PRIMARY KEY,                -- Wallet address
    realized_wins INTEGER DEFAULT 0,        -- Number of profitable closes
    realized_losses INTEGER DEFAULT 0,      -- Number of losing closes
    net_pnl_sol REAL DEFAULT 0.0,          -- Total profit/loss in SOL
    total_trades INTEGER DEFAULT 0,         -- Total number of trades
    is_tracked INTEGER DEFAULT 0,           -- 1 if wallet is being monitored
    win_rate REAL DEFAULT 0.0,             -- Calculated win rate (0.0-1.0)
    last_seen INTEGER                       -- Last activity timestamp
);
```

**Purpose**: Track wallet performance for copy-trading. Brain filters wallets with high win rates (>0.7) and positive PnL.

**Tracked Wallets**: Manually flagged wallets of interest (alpha traders, known profitable wallets).

---

#### 5. **positions** table

Current open positions per wallet per token.

```sql
CREATE TABLE positions (
    wallet TEXT NOT NULL,
    mint TEXT NOT NULL,
    bought_at INTEGER NOT NULL,             -- Unix timestamp of entry
    entry_price REAL NOT NULL,              -- Price at entry
    amount_tokens REAL NOT NULL,            -- Position size in tokens
    amount_sol REAL NOT NULL,               -- SOL invested
    PRIMARY KEY(wallet, mint, bought_at),
    FOREIGN KEY(mint) REFERENCES tokens(mint)
);
```

**Purpose**: Track open positions to calculate PnL when wallet exits. Used to update wallet_stats when position closes.

---

#### 6. **pyth_prices** table

SOL/USD price history from Pyth oracle.

```sql
CREATE TABLE pyth_prices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    price REAL NOT NULL,                    -- SOL price in USD
    confidence REAL NOT NULL,               -- Price confidence interval
    confidence_ratio REAL NOT NULL,         -- confidence / price
    source TEXT NOT NULL,                   -- "pyth" or "http"
    created_at INTEGER DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_pyth_prices_timestamp ON pyth_prices(timestamp);
```

**Purpose**: Historical SOL price for USD calculations. Also broadcasts latest price to Brain via UDP.

---

## UDP Advisory System

### Target: Brain Service

**Protocol**: UDP (unreliable, fire-and-forget)  
**Host**: 127.0.0.1  
**Port**: 45100  
**Packet Size**: Fixed 64 bytes

### Advisory Message Types

#### Message Type Enum

```rust
pub enum AdviceType {
    ExtendHold = 10,         // Hold position longer (not implemented)
    WidenExit = 11,          // Increase exit slippage (not implemented)
    LateOpportunity = 12,    // Mature token with momentum
    CopyTrade = 13,          // Alpha wallet trade detected (not implemented)
    SolPriceUpdate = 14,     // SOL/USD price broadcast
    RankOpportunity = 15,    // Top-ranked new launch (not implemented)
    MomentumOpportunity = 16 // High momentum token
}
```

### 1. **MomentumOpportunity** (Type 16)

Sent when a token shows high trading activity in short timeframes.

**Packet Structure** (64 bytes):

```
[0]      msg_type: u8 = 16
[1-32]   mint: [u8; 32]           // Token mint address (raw bytes)
[33-36]  vol_5s: f32              // SOL volume in last 5 seconds
[37-38]  buyers_2s: u16           // Unique buyers in last 2 seconds
[39]     score: u8                // Calculated score 0-100
[40-63]  padding: [u8; 24]        // Reserved/unused
```

**Trigger Conditions**:

```rust
// Testing thresholds (low for development)
if vol_5s >= 2.0 && buyers_2s >= 2 {
    let vol_score = (vol_5s / 8.0 * 50.0).clamp(0.0, 50.0);
    let buyer_score = ((buyers_2s as f64 / 5.0) * 50.0).clamp(0.0, 50.0);
    let momentum_score = (vol_score + buyer_score) as u8;

    send_momentum_opportunity(mint, vol_5s, buyers_2s, momentum_score);
}
```

**Code Example**:

```rust
pub fn send_momentum_opportunity(
    &self,
    mint_b58: &str,
    vol_5s_sol: f64,
    buyers_2s: u32,
    score: u8
) -> Result<()> {
    let mint_bytes = bs58::decode(mint_b58).into_vec()?;
    let mut msg = vec![0u8; 64];

    msg[0] = 16; // MomentumOpportunity
    msg[1..33].copy_from_slice(&mint_bytes);
    msg[33..37].copy_from_slice(&(vol_5s_sol as f32).to_le_bytes());
    msg[37..39].copy_from_slice(&(buyers_2s as u16).to_le_bytes());
    msg[39] = score;

    self.send_advice(&msg)
}
```

---

### 2. **SolPriceUpdate** (Type 14)

Broadcasts SOL/USD price every 30 seconds.

**Packet Structure** (64 bytes):

```
[0]      msg_type: u8 = 14
[1-8]    price_usd: f64           // SOL price in USD
[9-16]   confidence: f64          // Price confidence
[17-24]  timestamp: i64           // Unix timestamp
[25-63]  padding: [u8; 39]
```

**Code Example**:

```rust
pub fn send_sol_price_update(
    &self,
    price_usd: f64,
    confidence: f64,
    timestamp: i64
) -> Result<()> {
    let mut msg = vec![0u8; 64];
    msg[0] = 14; // SolPriceUpdate
    msg[1..9].copy_from_slice(&price_usd.to_le_bytes());
    msg[9..17].copy_from_slice(&confidence.to_le_bytes());
    msg[17..25].copy_from_slice(&timestamp.to_le_bytes());

    self.send_advice(&msg)
}
```

---

## Configuration

### Environment Variables (.env)

```bash
# gRPC Connection
GRPC_ENDPOINT=http://127.0.0.1:10000  # Yellowstone gRPC server
GRPC_X_TOKEN=your_auth_token_here      # Authentication token

# Database
DATABASE_PATH=./data/collector.db       # SQLite database file
WAL_MODE=true                           # Enable Write-Ahead Logging

# UDP Advisory
ADVICE_BUS_HOST=127.0.0.1              # Target host for advisories
ADVICE_BUS_PORT=45100                   # Target port (Brain listens here)

# Pyth Oracle
PYTH_PRICE_FEED=H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG  # SOL/USD feed
PYTH_CLUSTER_HTTP=https://hermes.pyth.network  # Pyth HTTP endpoint
```

---

## Module Breakdown

### `/src/main.rs`

**Purpose**: Entry point and main event loop.

**Key Functions**:

- `main()`: Initialize database, gRPC client, UDP sender, Pyth subscriber
- `process_transaction()`: Parse incoming transaction, extract trades
- `update_wallet_stats()`: Update wallet performance after trade close
- `check_and_send_opportunities()`: Evaluate windows and send advisories

**Main Loop**:

```rust
loop {
    // 1. Receive gRPC transaction
    // 2. Parse Pump.fun instructions
    // 3. Insert trades into database
    // 4. Update windows
    // 5. Check for advisory triggers
    // 6. Send UDP advisory if conditions met
}
```

---

### `/src/grpc/` - gRPC Client

#### `client.rs`

**Purpose**: Connect to Yellowstone gRPC server and stream transactions.

**Key Components**:

```rust
pub struct GrpcClient {
    endpoint: String,
    token: Option<String>,
}

impl GrpcClient {
    pub async fn subscribe_transactions(&self) -> impl Stream<Item = Transaction>
}
```

**Subscription Filter**:

- **Account Filter**: Pump.fun program ID (`6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`)
- **Transaction Filter**: All transactions involving Pump.fun accounts
- **Commitment Level**: Confirmed or Finalized

---

#### `stream.rs`

**Purpose**: Handle gRPC stream reconnection and error recovery.

**Features**:

- Automatic reconnection on disconnect
- Checkpoint-based recovery (resume from last processed slot)
- Backpressure handling
- Rate limiting

---

### `/src/parser/` - Transaction Parsing

#### `mod.rs`

**Purpose**: Parse Solana transactions to extract Pump.fun instructions.

**Key Functions**:

```rust
pub fn parse_pump_transaction(tx: &Transaction) -> Vec<TradeEvent>
pub fn extract_buy_instruction(ix_data: &[u8]) -> Option<BuyInfo>
pub fn extract_sell_instruction(ix_data: &[u8]) -> Option<SellInfo>
```

**Pump.fun Instruction Discriminators**:

- **Buy**: `0x66063d1201daebea` (first 8 bytes)
- **Sell**: `0x33e685a4017f83ad`

**Parsing Steps**:

1. Check if transaction involves Pump.fun program
2. Parse inner instructions (actual token transfer happens here)
3. Extract mint, trader, amount_tokens, amount_sol
4. Calculate price = amount_sol / amount_tokens
5. Return TradeEvent struct

---

#### `decoder.rs`

**Purpose**: Borsh deserialization of Pump.fun instruction data.

**Structures**:

```rust
pub struct BuyInstruction {
    pub amount: u64,             // Tokens to buy
    pub max_sol_cost: u64,       // Max SOL willing to spend
}

pub struct SellInstruction {
    pub amount: u64,             // Tokens to sell
    pub min_sol_output: u64,     // Min SOL expected
}
```

---

### `/src/db/` - Database Operations

#### `mod.rs`

**Purpose**: SQLite database abstraction.

**Key Methods**:

```rust
impl Database {
    pub fn insert_token(&mut self, token: &Token) -> Result<()>
    pub fn insert_trade(&mut self, trade: &Trade) -> Result<()>
    pub fn update_window(&mut self, mint: &str, window_sec: u32, ...) -> Result<()>
    pub fn get_wallet_stats(&self, wallet: &str) -> Result<WalletFeatures>
    pub fn get_token(&self, mint: &str) -> Result<Token>
}
```

---

#### `aggregator.rs`

**Purpose**: Window aggregation logic (time-based candlesticks).

**Key Functions**:

```rust
pub struct WindowAggregator {
    window_sizes: Vec<u32>,  // [2, 5, 60] seconds
}

impl WindowAggregator {
    pub fn update_windows(
        &self,
        db: &mut Database,
        mint: &str,
        trade: &Trade
    ) -> Result<()>
}
```

**Aggregation Logic**:

1. Determine which windows this trade belongs to (by timestamp)
2. For each window (2s, 5s, 60s):
   - Increment buy/sell count
   - Add to volume
   - Update OHLC (high, low, close)
   - Calculate VWAP
   - Track unique buyers
3. Upsert window record in database

---

#### `checkpoint.rs`

**Purpose**: Track last processed slot for crash recovery.

**Schema**:

```sql
CREATE TABLE checkpoint (
    id INTEGER PRIMARY KEY,
    last_slot INTEGER NOT NULL,
    last_timestamp INTEGER NOT NULL
);
```

**Usage**: On startup, resume from `last_slot + 1` to avoid re-processing.

---

### `/src/udp/` - UDP Advisory Sender

#### `mod.rs`

**Purpose**: UDP socket management and advisory sending.

**Key Methods**:

```rust
pub struct AdvisorySender {
    socket: Arc<UdpSocket>,
    target_addr: String,
}

impl AdvisorySender {
    pub fn send_momentum_opportunity(...) -> Result<()>
    pub fn send_rank_opportunity(...) -> Result<()>
    pub fn send_sol_price_update(...) -> Result<()>
}
```

**Non-blocking**: Socket set to non-blocking mode so data-mining doesn't stall if Brain is offline.

---

### `/src/pyth_subscriber.rs`

**Purpose**: Subscribe to Pyth Network for SOL/USD price.

**Methods**:

- **WebSocket**: Real-time price updates (primary)
- **HTTP Fallback**: Poll every 30s if WebSocket fails

**Flow**:

1. Connect to Pyth WebSocket
2. Subscribe to SOL/USD feed
3. On price update:
   - Store in pyth_prices table
   - Send SolPriceUpdate advisory to Brain

---

### `/src/types/` - Data Structures

#### `trade.rs`

```rust
pub struct Trade {
    pub sig: String,
    pub slot: u64,
    pub block_time: i64,
    pub mint: String,
    pub side: TradeSide,      // Buy or Sell
    pub trader: String,
    pub amount_tokens: f64,
    pub amount_sol: f64,
    pub price: f64,
}

pub enum TradeSide {
    Buy,
    Sell,
}
```

---

#### `token.rs`

```rust
pub struct Token {
    pub mint: String,
    pub creator_wallet: String,
    pub bonding_curve_addr: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: u8,
    pub launch_slot: u64,
    pub launch_block_time: i64,
    pub initial_price: Option<f64>,
    // ... other metadata
}
```

---

#### `window.rs`

```rust
pub struct Window {
    pub mint: String,
    pub window_sec: u32,
    pub start_time: i64,
    pub end_time: i64,
    pub num_buys: u32,
    pub num_sells: u32,
    pub uniq_buyers: u32,
    pub vol_sol: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub vwap: f64,
}
```

---

### `/src/config.rs`

**Purpose**: Load and validate configuration from environment variables.

**Structure**:

```rust
pub struct Config {
    pub grpc_endpoint: String,
    pub grpc_token: Option<String>,
    pub database_path: String,
    pub wal_mode: bool,
    pub advice_bus: AdviceBusConfig,
    pub pyth: PythConfig,
}

pub struct AdviceBusConfig {
    pub host: String,
    pub port: u16,
}
```

---

### `/src/checkpoint.rs`

**Purpose**: Persistence of last processed slot.

**Methods**:

```rust
pub fn save_checkpoint(db: &Database, slot: u64, timestamp: i64) -> Result<()>
pub fn load_checkpoint(db: &Database) -> Result<Option<(u64, i64)>>
```

---

## Performance Characteristics

### Throughput

- **Typical**: 10-20 transactions/second (Pump.fun activity)
- **Peak**: 50-100 transactions/second (high activity periods)
- **Database Writes**: ~30-50 inserts/updates per second

### Latency

- **gRPC → Parse**: <5ms
- **Parse → DB Write**: <10ms
- **DB Write → Advisory Send**: <1ms
- **Total Latency**: ~15-20ms from transaction confirmation to advisory

### Resource Usage

- **Memory**: ~50-100MB (mostly gRPC buffers)
- **CPU**: 5-10% (single core, mostly parsing)
- **Disk I/O**: ~500KB/s writes (with WAL mode)

---

## Monitoring & Logging

### Log Levels

- **INFO**: Transaction processed, advisory sent, checkpoint saved
- **DEBUG**: Window updates, wallet stats calculations
- **WARN**: Failed wallet updates, stale price data
- **ERROR**: gRPC disconnection, database errors

### Key Metrics Logged

```rust
info!("📊 Processed 800 txs | 6 launches | 0 wallet txs");
info!("💰 SOL price: $153.45 (confidence: ±0.12)");
info!("📡 Sent MomentumOpportunity: {} (score: 87)", mint);
```

---

## Error Handling

### gRPC Disconnection

- **Strategy**: Exponential backoff reconnection (1s, 2s, 4s, 8s, max 60s)
- **Recovery**: Resume from last checkpoint slot
- **Fallback**: Continue with cached data, log warnings

### Database Errors

- **UNIQUE constraint violation**: Ignore (idempotent inserts)
- **Lock timeout**: Retry with backoff
- **Corruption**: Panic and require manual intervention

### UDP Send Failures

- **Strategy**: Silent drop (non-blocking socket)
- **Reason**: Advisory system is best-effort, Brain should handle missing data

---

## Testing

### Unit Tests

- Parser: Test buy/sell instruction extraction
- Aggregator: Validate window calculations
- Database: Test schema creation and queries

### Integration Tests

- Mock gRPC: Inject test transactions
- Verify: Database state after processing
- Check: Advisory messages sent to UDP port

---

## Future Enhancements

### Planned Features

1. **RankOpportunity**: Detect top-ranked new launches
2. **CopyTrade**: Monitor alpha wallet trades and send advisories
3. **ExtendHold**: Position management advisories
4. **Multi-DEX Support**: Expand beyond Pump.fun to Raydium, Orca

### Performance Optimizations

1. **Batch Database Writes**: Buffer 100-500ms of trades, write in single transaction
2. **Async Window Aggregation**: Offload to background task
3. **Redis Cache**: Cache hot token data to reduce SQLite reads

---

## Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
rusqlite = { version = "0.30", features = ["bundled"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bs58 = "0.5"
solana-sdk = "1.17"
solana-transaction-status = "1.17"
yellowstone-grpc-client = "1.12"
yellowstone-grpc-proto = "1.12"
tonic = "0.10"
prost = "0.12"
borsh = "0.10"
dotenv = "0.15"
```

---

## Summary

The Data-Mining bot is a high-performance, real-time data collection system that:

- ✅ Streams all Pump.fun transactions via gRPC
- ✅ Parses and stores trades, tokens, and wallet activity in SQLite
- ✅ Aggregates metrics into 2s/5s/60s windows for momentum detection
- ✅ Sends UDP advisories to Brain when opportunities arise
- ✅ Broadcasts SOL/USD price updates every 30 seconds
- ✅ Tracks wallet performance for copy-trading strategies
- ✅ Handles ~20 TPS with <20ms latency

**Key Design Principles**:

1. **Speed**: Low-latency parsing and database writes
2. **Reliability**: Checkpoint-based recovery, automatic reconnection
3. **Simplicity**: Fire-and-forget UDP, no complex state management
4. **Separation**: Pure data collection, minimal decision logic (that's Brain's job)
