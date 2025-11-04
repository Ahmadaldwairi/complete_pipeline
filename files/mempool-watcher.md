# Mempool-Watcher - Comprehensive Reference

**Version**: 1.0  
**Purpose**: Monitor mempool for alpha trading signals and frontrunning opportunities  
**Language**: Rust  
**Dependencies**: Solana WebSocket RPC, UDP networking

---

## High-Level Overview

The Mempool-Watcher is a **real-time transaction monitoring service** that subscribes to pending Solana transactions, identifies high-value trades from alpha wallets, and sends "hot signals" to the Executor for potential frontrunning opportunities.

**Core Responsibilities**:

1. **Subscribe to mempool** via Solana WebSocket RPC
2. **Filter relevant transactions** (Pump.fun buys, large SOL amounts)
3. **Identify alpha wallets** (known profitable traders)
4. **Detect frontrun opportunities** (large buys before they land)
5. **Send hot signals** to Executor via UDP (port 45130)
6. **Track signal effectiveness** (hit rate, profitability)

**Data Flow**:

```
Solana Mempool (WebSocket)
    → Mempool-Watcher (filter & analyze)
    → Detect alpha trade (large buy from tracked wallet)
    → HotSignal (UDP:45130)
    → Executor (optional frontrun execution)
```

**Status**: Currently **disabled** in production in favor of Brain-driven decisions.

**Reason**: Mempool frontrunning has high risk and limited effectiveness on Solana due to:

- Jito bundles hiding transactions
- Low mempool visibility
- Regulatory concerns
- High false positive rate

---

## UDP Communication

### Outgoing: Hot Signals (Port 45130)

Mempool-Watcher **SENDS** to port 45130 where Executor can optionally listen.

**Target**: `127.0.0.1:45130` (UDP send)

#### Sent Message: HotSignal

```rust
// Packet: 80 bytes (fixed size)
pub struct HotSignal {
    pub msg_type: u8,           // [0] Always = 20 for HotSignal
    pub mint: [u8; 32],         // [1-32] Token mint address
    pub detected_buyer: [u8; 32], // [33-64] Wallet executing the trade
    pub amount_sol: f64,        // [65-72] SOL amount (8 bytes)
    pub urgency: u8,            // [73] Urgency level 0-255
    pub _padding: [u8; 6],      // [74-79] Reserved
}
```

**Example Signal**:

```rust
HotSignal {
    msg_type: 20,
    mint: [45, 123, 89, ...], // Token being bought
    detected_buyer: [78, 234, ...], // Alpha wallet address
    amount_sol: 5.75,         // 5.75 SOL buy detected
    urgency: 180,             // High urgency (0-255 scale)
    _padding: [0; 6],
}
```

**Urgency Calculation**:

```rust
pub fn calculate_urgency(amount_sol: f64, wallet_win_rate: f64) -> u8 {
    let amount_score = (amount_sol / 10.0 * 100.0).clamp(0.0, 100.0);
    let wallet_score = wallet_win_rate * 100.0;
    let urgency = (amount_score * 0.6 + wallet_score * 0.4) as u8;
    urgency.clamp(50, 255) // Minimum 50 to avoid noise
}
```

**Example**:

- 5 SOL buy from 80% win-rate wallet → urgency = 180
- 10 SOL buy from 60% win-rate wallet → urgency = 204

---

## Mempool Monitoring

### WebSocket Subscription

**Endpoint**: `wss://api.mainnet-beta.solana.com`

**Subscription Type**: `programSubscribe` for Pump.fun program

**Code**:

```rust
use solana_client::nonblocking::pubsub_client::PubsubClient;

pub struct MempoolWatcher {
    ws_url: String,
    pubsub: PubsubClient,
    alpha_wallets: Arc<RwLock<HashSet<String>>>,
    signal_sender: UdpSocket,
}

impl MempoolWatcher {
    pub async fn subscribe_to_mempool(&self) -> Result<()> {
        let program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")?; // Pump.fun

        let (mut notifications, unsubscribe) = self.pubsub
            .program_subscribe(
                &program_id,
                Some(RpcProgramAccountsConfig {
                    filters: None,
                    account_config: RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64),
                        commitment: Some(CommitmentConfig::processed()),
                        ..Default::default()
                    },
                }),
            )
            .await?;

        info!("📡 Subscribed to Pump.fun mempool");

        while let Some(notification) = notifications.next().await {
            self.handle_transaction(notification).await?;
        }

        Ok(())
    }
}
```

---

### Transaction Filtering

**Goal**: Only process relevant transactions (large buys from alpha wallets).

**Filters**:

1. **Instruction Filter**: Only BUY instructions (discriminator = 0x66063d1201daebea)
2. **Amount Filter**: SOL amount ≥ 1.0 SOL
3. **Wallet Filter**: Buyer in alpha wallet list
4. **Timing Filter**: Skip if token already processed in last 5 seconds

**Code**:

```rust
async fn handle_transaction(&self, notification: RpcKeyedAccount) -> Result<()> {
    let account_data = notification.account.data.decode()?;

    // Parse instruction
    let instruction = parse_pump_instruction(&account_data)?;

    // Filter 1: Only BUY instructions
    if instruction.discriminator != BUY_DISCRIMINATOR {
        return Ok(());
    }

    // Filter 2: Amount threshold
    let amount_sol = instruction.amount_lamports as f64 / 1e9;
    if amount_sol < 1.0 {
        return Ok(());
    }

    // Filter 3: Alpha wallet check
    let buyer = instruction.user.to_string();
    if !self.is_alpha_wallet(&buyer).await {
        return Ok(());
    }

    // Filter 4: Deduplicate recent signals
    let mint = instruction.mint.to_string();
    if self.was_recently_signaled(&mint).await {
        return Ok(());
    }

    // SEND HOT SIGNAL
    self.send_hot_signal(&mint, &buyer, amount_sol).await?;

    Ok(())
}
```

---

### Instruction Parsing

**Pump.fun BUY Instruction Layout**:

```rust
pub struct PumpBuyInstruction {
    pub discriminator: u64,        // [0-7] 0x66063d1201daebea
    pub amount_lamports: u64,      // [8-15] SOL amount
    pub min_tokens_out: u64,       // [16-23] Slippage protection
    pub mint: Pubkey,              // [24-55] Token mint
    pub user: Pubkey,              // [56-87] Buyer wallet
    pub bonding_curve: Pubkey,     // [88-119] Curve PDA
}
```

**Parse Function**:

```rust
fn parse_pump_instruction(data: &[u8]) -> Result<PumpBuyInstruction> {
    if data.len() < 120 {
        return Err(anyhow!("Invalid instruction size"));
    }

    let discriminator = u64::from_le_bytes(data[0..8].try_into()?);

    if discriminator != 0x66063d1201daebea {
        return Err(anyhow!("Not a BUY instruction"));
    }

    Ok(PumpBuyInstruction {
        discriminator,
        amount_lamports: u64::from_le_bytes(data[8..16].try_into()?),
        min_tokens_out: u64::from_le_bytes(data[16..24].try_into()?),
        mint: Pubkey::try_from(&data[24..56])?,
        user: Pubkey::try_from(&data[56..88])?,
        bonding_curve: Pubkey::try_from(&data[88..120])?,
    })
}
```

---

## Alpha Wallet Management

### Alpha Wallet List

**Purpose**: Track wallets with proven profitability for copy-trading.

**Storage**: In-memory HashSet, periodically loaded from database.

**Structure**:

```rust
pub struct AlphaWalletManager {
    wallets: Arc<RwLock<HashSet<String>>>,
    db: Arc<Database>,
    last_update: Arc<RwLock<Instant>>,
}
```

**Loading From Database**:

```rust
impl AlphaWalletManager {
    pub async fn load_alpha_wallets(&self) -> Result<usize> {
        let wallets = self.db.query(
            "SELECT wallet FROM wallet_stats
             WHERE win_rate > 0.7
               AND net_pnl_sol > 10.0
               AND total_trades > 10
               AND is_tracked = true
             ORDER BY net_pnl_sol DESC
             LIMIT 500"
        ).await?;

        let mut wallet_set = self.wallets.write().await;
        wallet_set.clear();

        for row in wallets {
            wallet_set.insert(row.wallet);
        }

        let count = wallet_set.len();
        info!("📊 Loaded {} alpha wallets", count);

        Ok(count)
    }

    pub async fn is_alpha_wallet(&self, wallet: &str) -> bool {
        self.wallets.read().await.contains(wallet)
    }
}
```

**Update Interval**: Every 60 seconds (background task).

**Typical Count**: 100-300 alpha wallets.

---

### Wallet Performance Criteria

**Criteria for Alpha Status**:

1. **Win Rate**: > 70% (7 out of 10 trades profitable)
2. **Net P&L**: > 10 SOL cumulative profit
3. **Total Trades**: > 10 (sufficient sample size)
4. **Tracked**: Manually verified or auto-promoted

**Example Alpha Wallet**:

```sql
wallet: 7xKXtg2CW...
win_rate: 0.82 (82%)
net_pnl_sol: 45.7 SOL
total_trades: 34
is_tracked: true
```

---

## Signal Deduplication

### Recent Signal Cache

**Purpose**: Prevent sending duplicate signals for the same token within a short timeframe.

**Structure**:

```rust
pub struct SignalCache {
    recent_signals: Arc<RwLock<HashMap<String, Instant>>>,
    cooldown_seconds: u64,
}
```

**Check Before Sending**:

```rust
impl SignalCache {
    pub async fn was_recently_signaled(&self, mint: &str) -> bool {
        let cache = self.recent_signals.read().await;

        if let Some(last_signal_time) = cache.get(mint) {
            let elapsed = Instant::now().duration_since(*last_signal_time).as_secs();
            if elapsed < self.cooldown_seconds {
                return true; // Still cooling down
            }
        }

        false
    }

    pub async fn record_signal(&self, mint: &str) {
        let mut cache = self.recent_signals.write().await;
        cache.insert(mint.to_string(), Instant::now());
    }
}
```

**Cooldown Period**: 5 seconds (configurable).

**Cleanup**: Remove entries older than 60 seconds (background task every 10s).

---

## Signal Effectiveness Tracking

### Performance Metrics

**Purpose**: Measure how profitable frontrun signals are.

**Tracked Metrics**:

1. **Signals Sent**: Total hot signals dispatched
2. **Signals Acted On**: How many Executor executed
3. **Hit Rate**: Percentage of profitable frontrun trades
4. **Average Profit**: Mean P&L per executed signal
5. **False Positives**: Signals that would have lost money

**Structure**:

```rust
pub struct SignalMetrics {
    signals_sent: AtomicU64,
    signals_acted_on: AtomicU64,
    profitable_trades: AtomicU64,
    total_pnl_sol: Arc<RwLock<f64>>,
}
```

**Update After Trade**:

```rust
impl SignalMetrics {
    pub async fn record_outcome(&self, profit_sol: f64) {
        self.signals_acted_on.fetch_add(1, Ordering::Relaxed);

        if profit_sol > 0.0 {
            self.profitable_trades.fetch_add(1, Ordering::Relaxed);
        }

        let mut pnl = self.total_pnl_sol.write().await;
        *pnl += profit_sol;
    }

    pub fn calculate_hit_rate(&self) -> f64 {
        let acted = self.signals_acted_on.load(Ordering::Relaxed);
        if acted == 0 {
            return 0.0;
        }

        let profitable = self.profitable_trades.load(Ordering::Relaxed);
        (profitable as f64 / acted as f64) * 100.0
    }
}
```

**Prometheus Metrics**:

```rust
// Signals sent
hot_signals_sent_total: Counter

// Hit rate
signal_hit_rate: Gauge

// Total P&L from signals
signal_total_pnl_sol: Gauge
```

---

## Configuration

### Environment Variables (.env)

```bash
# Solana WebSocket
WS_RPC_URL=wss://api.mainnet-beta.solana.com

# Pump.fun Program
PUMP_PROGRAM_ID=6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P

# Signal Parameters
MIN_BUY_AMOUNT_SOL=1.0          # Minimum SOL to trigger signal
SIGNAL_COOLDOWN_SECONDS=5       # Cooldown between signals for same token
ALPHA_WALLET_UPDATE_INTERVAL=60 # Refresh alpha wallet list every 60s

# UDP
SIGNAL_PORT=45130               # Send hot signals to Executor

# Database
DATABASE_URL=postgresql://user:pass@localhost/executor_db

# Performance
MAX_CONCURRENT_SIGNALS=100      # Max signals in processing queue
```

---

## Module Breakdown

### `/src/main.rs`

**Purpose**: Entry point, WebSocket subscription, signal dispatching.

**Key Components**:

- **PubsubClient**: WebSocket connection to Solana RPC
- **AlphaWalletManager**: Load and check alpha wallets
- **SignalCache**: Deduplicate recent signals
- **SignalMetrics**: Track effectiveness
- **UDP Sender**: Send HotSignals to Executor

**Main Loop**:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize components
    let watcher = MempoolWatcher::new().await?;
    let alpha_wallets = Arc::new(AlphaWalletManager::new(db).await?);
    let signal_cache = Arc::new(SignalCache::new(5));
    let metrics = Arc::new(SignalMetrics::new());

    // Background: Update alpha wallet list every 60s
    tokio::spawn({
        let alpha_wallets = alpha_wallets.clone();
        async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if let Err(e) = alpha_wallets.load_alpha_wallets().await {
                    error!("Failed to update alpha wallets: {}", e);
                }
            }
        }
    });

    // Subscribe to mempool
    watcher.subscribe_to_mempool(alpha_wallets, signal_cache, metrics).await?;

    Ok(())
}
```

---

### `/src/mempool.rs`

**Purpose**: Core mempool monitoring logic.

**Key Structs**:

```rust
pub struct MempoolWatcher {
    ws_url: String,
    pubsub: PubsubClient,
    signal_sender: UdpSocket,
    program_id: Pubkey,
}
```

**Key Methods**:

- `subscribe_to_mempool()`: Start WebSocket subscription
- `handle_transaction()`: Process incoming transaction
- `send_hot_signal()`: Dispatch signal to Executor

---

### `/src/alpha_wallets.rs`

**Purpose**: Manage alpha wallet list.

**Key Structs**:

```rust
pub struct AlphaWalletManager {
    wallets: Arc<RwLock<HashSet<String>>>,
    db: Arc<Database>,
    last_update: Arc<RwLock<Instant>>,
}
```

**Key Methods**:

- `load_alpha_wallets()`: Fetch from database
- `is_alpha_wallet()`: Check if wallet is tracked
- `add_alpha_wallet()`: Manually add wallet
- `remove_alpha_wallet()`: Remove wallet

---

### `/src/signal_cache.rs`

**Purpose**: Deduplicate signals.

**Key Structs**:

```rust
pub struct SignalCache {
    recent_signals: Arc<RwLock<HashMap<String, Instant>>>,
    cooldown_seconds: u64,
}
```

**Key Methods**:

- `was_recently_signaled()`: Check if token was recently signaled
- `record_signal()`: Mark token as signaled
- `cleanup_old_entries()`: Remove stale entries (>60s old)

---

### `/src/metrics.rs`

**Purpose**: Track signal effectiveness.

**Key Structs**:

```rust
pub struct SignalMetrics {
    signals_sent: AtomicU64,
    signals_acted_on: AtomicU64,
    profitable_trades: AtomicU64,
    total_pnl_sol: Arc<RwLock<f64>>,
}
```

**Key Methods**:

- `record_signal_sent()`: Increment signal counter
- `record_outcome()`: Track trade result
- `calculate_hit_rate()`: Compute profitability percentage

**Prometheus Metrics**:

```rust
hot_signals_sent_total: Counter
signal_hit_rate: Gauge
signal_total_pnl_sol: Gauge
signal_latency_seconds: Histogram
```

**Endpoint**: `http://localhost:9092/metrics`

---

### `/src/instruction_parser.rs`

**Purpose**: Parse Pump.fun instructions.

**Key Structs**:

```rust
pub struct PumpBuyInstruction {
    pub discriminator: u64,
    pub amount_lamports: u64,
    pub min_tokens_out: u64,
    pub mint: Pubkey,
    pub user: Pubkey,
    pub bonding_curve: Pubkey,
}
```

**Key Functions**:

- `parse_pump_instruction()`: Decode instruction data
- `is_buy_instruction()`: Check discriminator
- `extract_mint()`: Get token mint
- `extract_buyer()`: Get buyer wallet

---

### `/src/database.rs`

**Purpose**: Query alpha wallet data.

**Key Structs**:

```rust
pub struct Database {
    pool: Pool<Postgres>,
}
```

**Key Methods**:

```rust
pub async fn load_alpha_wallets(&self) -> Result<Vec<String>> {
    let rows = sqlx::query!(
        "SELECT wallet FROM wallet_stats
         WHERE win_rate > 0.7 AND net_pnl_sol > 10.0 AND total_trades > 10
         ORDER BY net_pnl_sol DESC LIMIT 500"
    ).fetch_all(&self.pool).await?;

    Ok(rows.into_iter().map(|r| r.wallet).collect())
}
```

---

## Performance Characteristics

### Latency

- **Transaction Received → Signal Sent**: ~5-20ms
- **Alpha Wallet Lookup**: <1ms (in-memory HashSet)
- **Signal Deduplication Check**: <1ms
- **Total Signal Latency**: ~10-30ms

### Throughput

- **Max Transactions/sec**: 500+ (limited by WebSocket bandwidth)
- **Typical Signals/min**: 5-20 (depends on market activity)

### Resource Usage

- **Memory**: ~50MB (alpha wallet cache)
- **CPU**: 5-10% (single core)
- **Network**: ~20KB/s (WebSocket stream)

---

## Limitations & Challenges

### 1. Limited Mempool Visibility

**Issue**: Solana has no public mempool like Ethereum.

**Reality**: Only transactions sent to specific RPC nodes are visible.

**Impact**: Most transactions (especially via Jito bundles) are invisible.

---

### 2. Jito Bundles

**Issue**: Alpha traders use Jito bundles to hide transactions.

**Reality**: Bundle transactions don't appear in public mempool.

**Impact**: Cannot frontrun bundled transactions.

---

### 3. Execution Speed

**Issue**: Frontrunning requires landing transaction before target.

**Reality**: Even with detection, execution takes ~1-3 seconds.

**Impact**: Often too slow to frontrun on Solana (400ms block times).

---

### 4. False Positives

**Issue**: Alpha wallet buy doesn't guarantee price increase.

**Reality**: Wallet may be exiting other positions or testing.

**Impact**: High false positive rate (~40-60%).

---

### 5. Regulatory Risk

**Issue**: Frontrunning may violate securities laws.

**Reality**: Mempool frontrunning is legally gray area.

**Impact**: Disabled in production to avoid legal issues.

---

## Why Mempool-Watcher is Disabled

**Reasons**:

1. **Low Effectiveness**: <30% hit rate in testing
2. **High Risk**: Regulatory concerns about frontrunning
3. **Limited Visibility**: Jito bundles hide most alpha trades
4. **Execution Lag**: Too slow to consistently frontrun
5. **Better Alternative**: Brain-driven decisions based on post-execution data are more reliable

**Current Strategy**: Use Data-Mining advisories (momentum, rank, late opportunities) instead of mempool signals.

---

## Testing

### Unit Tests

- Instruction parsing
- Alpha wallet lookup
- Signal deduplication

### Integration Tests

- Mock WebSocket: Send test transactions
- Verify: Correct HotSignals sent
- Check: False positives filtered

---

## Monitoring & Logging

### Key Logs

```rust
info!("📡 Subscribed to Pump.fun mempool");
info!("🔥 HOT SIGNAL: {} | buyer: {} | amount: {:.2} SOL | urgency: {}",
    mint, buyer, amount_sol, urgency);
warn!("⏭️  Skipping: {} recently signaled", mint);
debug!("🔍 Non-alpha wallet: {}", buyer);
```

### Metrics Dashboard

- **Signals Sent**: Rate of hot signals dispatched
- **Hit Rate**: Percentage of profitable signals
- **Total P&L**: Cumulative profit from signals
- **Latency**: Time from detection to signal sent

---

## Future Improvements

### 1. Multi-RPC Monitoring

**Idea**: Subscribe to multiple RPC endpoints for better mempool coverage.

**Benefit**: Increase transaction visibility by 2-3x.

---

### 2. Machine Learning Classification

**Idea**: Train ML model to predict profitable frontrun opportunities.

**Features**: Wallet history, token age, amount, time of day, market conditions.

**Benefit**: Reduce false positives to <20%.

---

### 3. Direct TPU Monitoring

**Idea**: Connect directly to validator TPU ports to see transactions earlier.

**Benefit**: Reduce detection latency to <5ms.

**Complexity**: Requires QUIC protocol implementation.

---

### 4. Whale Alert Integration

**Idea**: Cross-reference with on-chain whale tracking services.

**Benefit**: Higher confidence signals from verified large holders.

---

## Summary

The Mempool-Watcher is a **real-time transaction monitoring service** that:

- ✅ Subscribes to Solana mempool via WebSocket
- ✅ Filters Pump.fun BUY transactions
- ✅ Identifies trades from alpha wallets (>70% win rate, >10 SOL profit)
- ✅ Sends HotSignals to Executor via UDP (port 45130)
- ✅ Deduplicates signals with 5s cooldown
- ✅ Tracks signal effectiveness (hit rate, P&L)
- ❌ Currently **disabled** in production due to:
  - Low effectiveness (<30% hit rate)
  - Regulatory concerns (frontrunning)
  - Limited mempool visibility (Jito bundles)
  - Execution lag (too slow to consistently frontrun)

**Current Recommendation**: Use Brain-driven decisions based on post-execution data (momentum, rank, late opportunities) instead of mempool frontrunning.

**Key Design Principles**:

1. **Speed**: Low-latency signal dispatching (~10-30ms)
2. **Selectivity**: Only signal high-confidence opportunities
3. **Transparency**: Track effectiveness for continuous improvement
4. **Safety**: Disabled by default to avoid regulatory risk
