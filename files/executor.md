# Executor (Execution Bot) - Comprehensive Reference

**Version**: 1.0  
**Purpose**: Trade execution engine - receives decisions, executes blockchain transactions  
**Language**: Rust  
**Dependencies**: PostgreSQL (trade logging), Solana RPC, Jito bundles, Telegram notifications

---

## High-Level Overview

The Executor is the **trading execution layer** that receives TradeDecision messages from the Brain and executes them on-chain. It handles all blockchain interactions, position tracking, profit/loss calculation, and real-time notifications.

**Core Responsibilities**:

1. **Listen for UDP decisions** from Brain (port 45110)
2. **Execute BUY transactions** via Pump.fun bonding curve
3. **Execute SELL transactions** with stored entry data
4. **Track active positions** in-memory (mint → BuyResult mapping)
5. **Calculate profit/loss** on exits
6. **Send Jito bundles** for MEV protection
7. **Log trades** to PostgreSQL
8. **Send Telegram notifications** for executions and P&L
9. **Monitor mempool signals** (port 45130, optional)

**Data Flow**:

```
Brain (UDP:45110)
    → Executor (parse TradeDecision)
    → Validate (check side, size, slippage)
    → Execute:
        • BUY: bonding_curve.buy() → store BuyResult
        • SELL: lookup BuyResult → bonding_curve.sell()
    → Log to PostgreSQL
    → Send Telegram notification
    → Update position tracker
```

---

## UDP Communication

### Incoming: Decision Bus (Port 45110)

Executor **BINDS** to port 45110 to receive trade decisions from Brain.

**Socket**: `127.0.0.1:45110` (UDP listener)

#### Received Message: TradeDecision

```rust
// Packet: 52 bytes (fixed size)
pub struct TradeDecision {
    pub msg_type: u8,           // [0] Always = 1
    pub mint: [u8; 32],         // [1-32] Token mint address
    pub side: u8,               // [33] 0=BUY, 1=SELL
    pub size_lamports: u64,     // [34-41] Position size in lamports
    pub slippage_bps: u16,      // [42-43] Max slippage (basis points)
    pub confidence: u8,         // [44] Confidence score 0-100
    pub _padding: [u8; 5],      // [45-51] Reserved
}
```

**Parsing Code**:

```rust
impl TradeDecision {
    pub fn from_bytes(buf: &[u8]) -> Result<Self> {
        if buf.len() != 52 {
            return Err(anyhow!("Invalid packet size: {}", buf.len()));
        }

        Ok(TradeDecision {
            msg_type: buf[0],
            mint: buf[1..33].try_into()?,
            side: buf[33],
            size_lamports: u64::from_le_bytes(buf[34..42].try_into()?),
            slippage_bps: u16::from_le_bytes(buf[42..44].try_into()?),
            confidence: buf[44],
            _padding: [0; 5],
        })
    }
}
```

**Example Decision**:

```rust
TradeDecision {
    msg_type: 1,
    mint: [45, 123, 89, ...], // 32-byte mint pubkey
    side: 0,                   // BUY
    size_lamports: 20_200_000, // 0.0202 SOL
    slippage_bps: 500,         // 5%
    confidence: 84,
    _padding: [0; 5],
}
```

---

### Incoming: Mempool Signals (Port 45130, Optional)

Executor can optionally listen for frontrun opportunities from Mempool-Watcher.

**Socket**: `127.0.0.1:45130` (UDP listener)

**Message Type**: HotSignal

```rust
pub struct HotSignal {
    pub mint: [u8; 32],
    pub detected_buyer: [u8; 32],
    pub amount_sol: f64,
    pub urgency: u8, // 0-255
}
```

**Handler**: Currently disabled in favor of Brain-driven decisions only.

---

## Trade Execution

### BUY Execution Flow

**Entry Point**: `main.rs` line 142-159

```rust
if decision.side == 0 {
    // BUY EXECUTION
    let mint_str = bs58::encode(&decision.mint).into_string();
    let size_sol = decision.size_lamports as f64 / 1e9;
    let slippage = decision.slippage_bps as f64 / 10000.0;

    info!("🟢 BUY {} | size: {:.4} SOL | slippage: {:.1}% | conf: {}",
        mint_str, size_sol, slippage * 100.0, decision.confidence);

    match trading_clone.buy(&mint_str, size_sol, None, Some(slippage)).await {
        Ok(buy_result) => {
            info!("✅ BUY EXECUTED: {} | entry: ${:.6} | tokens: {}",
                mint_str, buy_result.entry_price, buy_result.tokens_received);

            // STORE BUYRESULT FOR FUTURE SELL
            let mut positions = positions_clone.write().await;
            positions.insert(mint_str.clone(), ActivePosition {
                mint: mint_str.clone(),
                buy_result: buy_result,
                entry_time: Instant::now(),
            });
        }
        Err(e) => {
            error!("❌ BUY FAILED: {} | error: {}", mint_str, e);
        }
    }
}
```

**Trading Module** (`trading.rs`):

```rust
pub async fn buy(
    &self,
    mint: &str,
    amount_sol: f64,
    min_tokens: Option<u64>,
    max_slippage: Option<f64>,
) -> Result<BuyResult> {
    // 1. Get bonding curve state
    let curve_state = self.pump_curve.get_curve_state(mint).await?;

    // 2. Calculate expected tokens
    let amount_lamports = (amount_sol * 1e9) as u64;
    let expected_tokens = curve_state.calculate_tokens_out(amount_lamports);

    // 3. Apply slippage
    let slippage = max_slippage.unwrap_or(0.05);
    let min_tokens_out = (expected_tokens as f64 * (1.0 - slippage)) as u64;

    // 4. Build transaction
    let tx = self.pump_curve.build_buy_transaction(
        mint,
        amount_lamports,
        min_tokens_out,
        &self.wallet_keypair,
    ).await?;

    // 5. Send via Jito bundle
    let signature = self.jito.send_bundle(vec![tx]).await?;

    // 6. Confirm transaction
    self.rpc.confirm_transaction(&signature, CommitmentLevel::Confirmed).await?;

    // 7. Return BuyResult
    Ok(BuyResult {
        signature: signature.to_string(),
        mint: mint.to_string(),
        tokens_received: expected_tokens, // TODO: parse from logs
        entry_price: curve_state.current_price(),
        sol_spent: amount_sol,
        timestamp: chrono::Utc::now().timestamp(),
    })
}
```

**BuyResult Structure**:

```rust
pub struct BuyResult {
    pub signature: String,
    pub mint: String,
    pub tokens_received: u64,
    pub entry_price: f64,
    pub sol_spent: f64,
    pub timestamp: i64,
}
```

---

### SELL Execution Flow

**Entry Point**: `main.rs` line 164-220

```rust
if decision.side == 1 {
    // SELL EXECUTION
    let mint_str = bs58::encode(&decision.mint).into_string();

    // LOOKUP STORED BUYRESULT
    let positions_read = positions_clone.read().await;
    if let Some(position) = positions_read.get(&mint_str) {
        let buy_result_clone = position.buy_result.clone();
        drop(positions_read); // Release lock before async call

        let slippage = decision.slippage_bps as f64 / 10000.0;

        info!("🔴 SELL {} | entry: ${:.6} | slippage: {:.1}%",
            mint_str, buy_result_clone.entry_price, slippage * 100.0);

        match trading_clone.sell(
            &mint_str,
            &buy_result_clone,
            0.0, // min_sol_out (calculated internally)
            "Discovery",
            None,
            Some(slippage)
        ).await {
            Ok(exit_result) => {
                let profit_sol = exit_result.sol_received - buy_result_clone.sol_spent;
                let profit_pct = (profit_sol / buy_result_clone.sol_spent) * 100.0;

                info!("✅ SELL EXECUTED: {} | exit: ${:.6} | P&L: {:.4} SOL ({:.1}%)",
                    mint_str, exit_result.exit_price, profit_sol, profit_pct);

                // REMOVE FROM ACTIVE POSITIONS
                let mut positions_write = positions_clone.write().await;
                positions_write.remove(&mint_str);

                // Send Telegram notification
                telegram_clone.send_trade_exit(&mint_str, profit_sol, profit_pct).await;
            }
            Err(e) => {
                error!("❌ SELL FAILED: {} | error: {}", mint_str, e);
            }
        }
    } else {
        error!("❌ SELL REJECTED: No active position for {}", mint_str);
    }
}
```

**Trading Module** (`trading.rs`):

```rust
pub async fn sell(
    &self,
    mint: &str,
    buy_result: &BuyResult,
    min_sol_out: f64,
    reason: &str,
    force: Option<bool>,
    max_slippage: Option<f64>,
) -> Result<ExitResult> {
    // 1. Get current bonding curve state
    let curve_state = self.pump_curve.get_curve_state(mint).await?;

    // 2. Calculate expected SOL from tokens
    let tokens_to_sell = buy_result.tokens_received;
    let expected_sol_lamports = curve_state.calculate_sol_out(tokens_to_sell);

    // 3. Apply slippage
    let slippage = max_slippage.unwrap_or(0.05);
    let min_sol_lamports = (expected_sol_lamports as f64 * (1.0 - slippage)) as u64;

    // 4. Build transaction
    let tx = self.pump_curve.build_sell_transaction(
        mint,
        tokens_to_sell,
        min_sol_lamports,
        &self.wallet_keypair,
    ).await?;

    // 5. Send via Jito bundle
    let signature = self.jito.send_bundle(vec![tx]).await?;

    // 6. Confirm transaction
    self.rpc.confirm_transaction(&signature, CommitmentLevel::Confirmed).await?;

    // 7. Calculate profit
    let sol_received = expected_sol_lamports as f64 / 1e9;
    let profit_sol = sol_received - buy_result.sol_spent;

    // 8. Return ExitResult
    Ok(ExitResult {
        signature: signature.to_string(),
        mint: mint.to_string(),
        sol_received,
        exit_price: curve_state.current_price(),
        profit_sol,
        profit_pct: (profit_sol / buy_result.sol_spent) * 100.0,
        timestamp: chrono::Utc::now().timestamp(),
        reason: reason.to_string(),
    })
}
```

**ExitResult Structure**:

```rust
pub struct ExitResult {
    pub signature: String,
    pub mint: String,
    pub sol_received: f64,
    pub exit_price: f64,
    pub profit_sol: f64,
    pub profit_pct: f64,
    pub timestamp: i64,
    pub reason: String,
}
```

---

## Position Tracking

### ActivePosition Structure

**Defined**: `main.rs` line 30-36

```rust
struct ActivePosition {
    mint: String,
    buy_result: trading::BuyResult,
    entry_time: Instant,
}
```

**Storage**: In-memory HashMap

```rust
type PositionMap = Arc<RwLock<HashMap<String, ActivePosition>>>;

let active_positions: PositionMap = Arc::new(RwLock::new(HashMap::new()));
```

**Lifecycle**:

1. **BUY**: Insert into HashMap after successful execution
2. **SELL**: Lookup BuyResult, execute sell, remove from HashMap
3. **Exit**: Entry removed = position closed

**Why BuyResult is Stored**:

- SELL needs `tokens_received` to know how many tokens to sell
- SELL needs `entry_price` to calculate profit/loss
- SELL needs `sol_spent` to calculate profit percentage
- Without BuyResult, Executor cannot execute proper exits

---

## Database (PostgreSQL)

Executor uses PostgreSQL for **trade logging only** (not for decision-making).

**Connection String**: `postgresql://user:pass@localhost/executor_db`

### Schema

#### `executed_trades` table

```sql
CREATE TABLE executed_trades (
    id SERIAL PRIMARY KEY,
    signature TEXT NOT NULL UNIQUE,
    mint TEXT NOT NULL,
    side TEXT NOT NULL, -- 'BUY' or 'SELL'
    amount_sol NUMERIC NOT NULL,
    tokens BIGINT,
    price NUMERIC,
    slippage_bps INTEGER,
    confidence INTEGER,
    profit_sol NUMERIC, -- NULL for BUY, calculated for SELL
    profit_pct NUMERIC, -- NULL for BUY
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reason TEXT,

    INDEX idx_mint (mint),
    INDEX idx_timestamp (timestamp),
    INDEX idx_side (side)
);
```

**Example BUY Row**:

```sql
INSERT INTO executed_trades
(signature, mint, side, amount_sol, tokens, price, slippage_bps, confidence, timestamp)
VALUES
('5x7Y...abc', '2u1767RX...', 'BUY', 0.0202, 15000000, 0.00000135, 500, 84, NOW());
```

**Example SELL Row**:

```sql
INSERT INTO executed_trades
(signature, mint, side, amount_sol, tokens, price, slippage_bps, profit_sol, profit_pct, reason, timestamp)
VALUES
('8kQP...xyz', '2u1767RX...', 'SELL', 0.0245, 15000000, 0.00000163, 500, 0.0043, 21.3, 'Time-based exit', NOW());
```

**Query for Performance**:

```sql
SELECT
    COUNT(*) FILTER (WHERE side = 'SELL' AND profit_sol > 0) AS wins,
    COUNT(*) FILTER (WHERE side = 'SELL' AND profit_sol <= 0) AS losses,
    SUM(profit_sol) FILTER (WHERE side = 'SELL') AS total_pnl,
    AVG(profit_pct) FILTER (WHERE side = 'SELL') AS avg_profit_pct
FROM executed_trades
WHERE timestamp > NOW() - INTERVAL '24 hours';
```

---

## Pump.fun Bonding Curve Integration

### Overview

Pump.fun uses a **virtual AMM bonding curve** for token launches. Executor interacts with this curve to execute buys/sells.

**Bonding Curve Address**: Derived from mint address

**Program ID**: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`

### Bonding Curve State

**Structure** (`pump_bonding_curve.rs`):

```rust
pub struct BondingCurveState {
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
}
```

**Fetch State**:

```rust
pub async fn get_curve_state(&self, mint: &str) -> Result<BondingCurveState> {
    let curve_pda = self.derive_curve_address(mint);
    let account = self.rpc.get_account(&curve_pda).await?;
    BondingCurveState::deserialize(&account.data)
}
```

---

### Buy Calculation

**Formula**: Constant Product (x \* y = k)

```rust
pub fn calculate_tokens_out(&self, sol_in: u64) -> u64 {
    let k = self.virtual_sol_reserves * self.virtual_token_reserves;
    let new_sol_reserves = self.virtual_sol_reserves + sol_in;
    let new_token_reserves = k / new_sol_reserves;
    let tokens_out = self.virtual_token_reserves - new_token_reserves;
    tokens_out
}
```

**Example**:

- SOL in: 0.02 (20_000_000 lamports)
- Virtual SOL reserves: 30 SOL
- Virtual token reserves: 1,000,000,000 tokens
- k = 30 \* 1B = 30B
- New SOL reserves = 30.02
- New token reserves = 30B / 30.02 = 999,333,778
- Tokens out = 1B - 999,333,778 = **666,222 tokens**

---

### Sell Calculation

**Formula**: Reverse constant product

```rust
pub fn calculate_sol_out(&self, tokens_in: u64) -> u64 {
    let k = self.virtual_sol_reserves * self.virtual_token_reserves;
    let new_token_reserves = self.virtual_token_reserves + tokens_in;
    let new_sol_reserves = k / new_token_reserves;
    let sol_out = self.virtual_sol_reserves - new_sol_reserves;
    sol_out
}
```

---

### Transaction Building

**Buy Transaction** (`pump_bonding_curve.rs`):

```rust
pub async fn build_buy_transaction(
    &self,
    mint: &str,
    amount_lamports: u64,
    min_tokens_out: u64,
    payer: &Keypair,
) -> Result<Transaction> {
    let mint_pubkey = Pubkey::from_str(mint)?;
    let curve_pda = self.derive_curve_address(mint);
    let user_token_account = get_associated_token_address(&payer.pubkey(), &mint_pubkey);

    // Instruction: Buy tokens from bonding curve
    let ix = pump_instruction::buy(
        &self.program_id,
        &curve_pda,
        &mint_pubkey,
        &user_token_account,
        &payer.pubkey(),
        amount_lamports,
        min_tokens_out,
    )?;

    let recent_blockhash = self.rpc.get_latest_blockhash().await?;

    let tx = Transaction::new_signed_with_payer(
        &[ix],
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );

    Ok(tx)
}
```

**Sell Transaction**: Similar structure, uses `pump_instruction::sell()`

---

## Jito Bundle Integration

### Overview

Executor sends transactions via **Jito bundles** for MEV protection and faster inclusion.

**Jito RPC**: `https://mainnet.block-engine.jito.wtf`

**Bundle Structure**: Group of 1-5 transactions with optional tip

### Send Bundle

**Code** (`jito.rs`):

```rust
pub async fn send_bundle(&self, transactions: Vec<Transaction>) -> Result<Signature> {
    // 1. Add tip transaction (0.0001 SOL to Jito)
    let tip_tx = self.build_tip_transaction(100_000)?; // 0.0001 SOL

    // 2. Combine into bundle
    let mut bundle = transactions;
    bundle.push(tip_tx);

    // 3. Send to Jito
    let bundle_id = self.jito_client.send_bundle(bundle).await?;

    // 4. Wait for confirmation
    let signature = self.poll_bundle_status(&bundle_id, Duration::from_secs(30)).await?;

    Ok(signature)
}
```

**Tip Calculation**:

- Base tip: 0.0001 SOL (100k lamports)
- High priority: 0.0005 SOL (500k lamports)
- Urgent: 0.001 SOL (1M lamports)

**Bundle Advantages**:

- ✅ Atomic execution (all or nothing)
- ✅ MEV protection (front-run resistant)
- ✅ Faster inclusion (~1-2 slots vs 3-5 slots)
- ✅ Guaranteed ordering

---

## Telegram Notifications

### Setup

**Bot Token**: Loaded from `.env` file

**Chat ID**: Target user/group for notifications

**Code** (`telegram.rs`):

```rust
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            bot_token,
            chat_id,
            client: reqwest::Client::new(),
        }
    }
}
```

---

### Notification Types

#### 1. Trade Entry (BUY)

```rust
pub async fn send_trade_entry(&self, mint: &str, size_sol: f64, confidence: u8) -> Result<()> {
    let msg = format!(
        "🟢 <b>BUY EXECUTED</b>\n\
         Token: <code>{}</code>\n\
         Size: {} SOL\n\
         Confidence: {}",
        mint, size_sol, confidence
    );

    self.send_message(&msg).await
}
```

**Example**:

```
🟢 BUY EXECUTED
Token: 2u1767RXqW3aBp...
Size: 0.0202 SOL
Confidence: 84
```

---

#### 2. Trade Exit (SELL)

```rust
pub async fn send_trade_exit(&self, mint: &str, profit_sol: f64, profit_pct: f64) -> Result<()> {
    let emoji = if profit_sol > 0.0 { "✅" } else { "❌" };

    let msg = format!(
        "{} <b>SELL EXECUTED</b>\n\
         Token: <code>{}</code>\n\
         P&L: {:.4} SOL ({:.1}%)",
        emoji, mint, profit_sol, profit_pct
    );

    self.send_message(&msg).await
}
```

**Example**:

```
✅ SELL EXECUTED
Token: 2u1767RXqW3aBp...
P&L: +0.0043 SOL (+21.3%)
```

---

#### 3. Error Alert

```rust
pub async fn send_error(&self, error: &str) -> Result<()> {
    let msg = format!("⚠️ <b>EXECUTION ERROR</b>\n{}", error);
    self.send_message(&msg).await
}
```

---

### Send Message Implementation

```rust
async fn send_message(&self, text: &str) -> Result<()> {
    let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

    let params = serde_json::json!({
        "chat_id": self.chat_id,
        "text": text,
        "parse_mode": "HTML"
    });

    let response = self.client
        .post(&url)
        .json(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("Telegram API error: {}", response.status()));
    }

    Ok(())
}
```

---

## Configuration

### Environment Variables (.env)

```bash
# Solana RPC
RPC_URL=https://api.mainnet-beta.solana.com
RPC_WS_URL=wss://api.mainnet-beta.solana.com

# Jito
JITO_RPC_URL=https://mainnet.block-engine.jito.wtf
JITO_TIP_LAMPORTS=100000  # 0.0001 SOL

# Wallet
WALLET_KEYPAIR_PATH=/path/to/keypair.json

# Database
DATABASE_URL=postgresql://user:pass@localhost/executor_db

# UDP
DECISION_BUS_PORT=45110
MEMPOOL_SIGNAL_PORT=45130  # Optional

# Telegram
TELEGRAM_BOT_TOKEN=123456:ABC-DEF...
TELEGRAM_CHAT_ID=-1001234567890

# Execution Params
MAX_SLIPPAGE_BPS=500       # 5%
ENABLE_JITO_BUNDLES=true
```

---

## Module Breakdown

### `/src/main.rs`

**Purpose**: Entry point, UDP listener, decision routing.

**Key Components**:

- **UDP Receiver**: Listens on port 45110 for TradeDecisions
- **Position Tracker**: HashMap of active positions
- **Message Router**: Routes BUY/SELL to trading module
- **Logging**: Trade execution logs

**Main Loop**:

```rust
let mut buf = [0u8; 52];
loop {
    let (amt, _src) = socket.recv_from(&mut buf).await?;

    if amt != 52 {
        warn!("Invalid packet size: {}", amt);
        continue;
    }

    match TradeDecision::from_bytes(&buf) {
        Ok(decision) => {
            tokio::spawn(handle_decision(decision, ...));
        }
        Err(e) => {
            error!("Failed to parse decision: {}", e);
        }
    }
}
```

---

### `/src/trading.rs`

**Purpose**: Core buy/sell execution logic.

**Key Structs**:

```rust
pub struct TradingEngine {
    rpc: Arc<RpcClient>,
    pump_curve: Arc<PumpBondingCurve>,
    jito: Arc<JitoClient>,
    wallet_keypair: Keypair,
    telegram: Arc<TelegramNotifier>,
    database: Arc<Database>,
}

pub struct BuyResult {
    pub signature: String,
    pub mint: String,
    pub tokens_received: u64,
    pub entry_price: f64,
    pub sol_spent: f64,
    pub timestamp: i64,
}

pub struct ExitResult {
    pub signature: String,
    pub mint: String,
    pub sol_received: f64,
    pub exit_price: f64,
    pub profit_sol: f64,
    pub profit_pct: f64,
    pub timestamp: i64,
    pub reason: String,
}
```

**Key Methods**:

- `buy()`: Execute BUY transaction
- `sell()`: Execute SELL transaction
- `calculate_position_value()`: Get current position value
- `validate_transaction()`: Pre-flight checks

---

### `/src/advice_bus.rs`

**Purpose**: Parse UDP TradeDecision messages.

**Key Functions**:

```rust
pub fn parse_trade_decision(buf: &[u8]) -> Result<TradeDecision> {
    TradeDecision::from_bytes(buf)
}

pub struct TradeDecision {
    pub msg_type: u8,
    pub mint: [u8; 32],
    pub side: u8,
    pub size_lamports: u64,
    pub slippage_bps: u16,
    pub confidence: u8,
    pub _padding: [u8; 5],
}
```

---

### `/src/pump_bonding_curve.rs`

**Purpose**: Pump.fun bonding curve interaction.

**Key Structs**:

```rust
pub struct PumpBondingCurve {
    program_id: Pubkey,
    rpc: Arc<RpcClient>,
}

pub struct BondingCurveState {
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub complete: bool,
}
```

**Key Methods**:

- `get_curve_state()`: Fetch curve reserves
- `calculate_tokens_out()`: Calculate expected tokens for SOL in
- `calculate_sol_out()`: Calculate expected SOL for tokens in
- `build_buy_transaction()`: Construct BUY instruction
- `build_sell_transaction()`: Construct SELL instruction
- `derive_curve_address()`: Get bonding curve PDA

---

### `/src/pump_instructions.rs`

**Purpose**: Pump.fun instruction builders.

**Key Functions**:

```rust
pub fn buy(
    program_id: &Pubkey,
    curve: &Pubkey,
    mint: &Pubkey,
    user_token_account: &Pubkey,
    user: &Pubkey,
    amount_lamports: u64,
    min_tokens_out: u64,
) -> Result<Instruction>

pub fn sell(
    program_id: &Pubkey,
    curve: &Pubkey,
    mint: &Pubkey,
    user_token_account: &Pubkey,
    user: &Pubkey,
    tokens_in: u64,
    min_sol_out: u64,
) -> Result<Instruction>
```

---

### `/src/jito.rs`

**Purpose**: Jito bundle submission.

**Key Structs**:

```rust
pub struct JitoClient {
    rpc_url: String,
    tip_account: Pubkey,
    client: reqwest::Client,
}
```

**Key Methods**:

- `send_bundle()`: Submit transaction bundle
- `build_tip_transaction()`: Create tip transaction
- `poll_bundle_status()`: Wait for bundle confirmation

---

### `/src/tpu_client.rs`

**Purpose**: Direct TPU transaction submission (fallback).

**Key Structs**:

```rust
pub struct TpuClient {
    leader_schedule: LeaderSchedule,
    quic_connections: HashMap<Pubkey, QuicConnection>,
}
```

**Key Methods**:

- `send_transaction()`: Send directly to TPU leader
- `get_current_leader()`: Fetch current slot leader

---

### `/src/database.rs`

**Purpose**: PostgreSQL trade logging.

**Key Structs**:

```rust
pub struct Database {
    pool: Pool<Postgres>,
}
```

**Key Methods**:

```rust
pub async fn log_buy(&self, result: &BuyResult, confidence: u8) -> Result<()> {
    sqlx::query!(
        "INSERT INTO executed_trades (signature, mint, side, amount_sol, tokens, price, confidence)
         VALUES ($1, $2, 'BUY', $3, $4, $5, $6)",
        result.signature, result.mint, result.sol_spent, result.tokens_received as i64,
        result.entry_price, confidence as i32
    ).execute(&self.pool).await?;
    Ok(())
}

pub async fn log_sell(&self, result: &ExitResult) -> Result<()> {
    sqlx::query!(
        "INSERT INTO executed_trades (signature, mint, side, amount_sol, sol_received, price, profit_sol, profit_pct, reason)
         VALUES ($1, $2, 'SELL', $3, $4, $5, $6, $7, $8)",
        result.signature, result.mint, 0.0, result.sol_received, result.exit_price,
        result.profit_sol, result.profit_pct, result.reason
    ).execute(&self.pool).await?;
    Ok(())
}
```

---

### `/src/telegram.rs`

**Purpose**: Telegram notification integration.

**Key Structs**:

```rust
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}
```

**Key Methods**:

- `send_trade_entry()`: Notify BUY execution
- `send_trade_exit()`: Notify SELL execution with P&L
- `send_error()`: Alert on execution failures

---

### `/src/mempool.rs`

**Purpose**: Mempool monitoring (optional).

**Key Structs**:

```rust
pub struct MempoolWatcher {
    rpc_ws: WebSocketClient,
    signal_sender: UdpSocket,
}
```

**Key Methods**:

- `subscribe_transactions()`: Listen for pending transactions
- `detect_alpha_trades()`: Identify high-value trades
- `send_hot_signal()`: Alert Executor for frontrun opportunity

**Status**: Currently disabled in favor of Brain-driven decisions only.

---

### `/src/config.rs`

**Purpose**: Load and validate configuration.

**Structure**:

```rust
pub struct Config {
    pub rpc: RpcConfig,
    pub jito: JitoConfig,
    pub wallet: WalletConfig,
    pub database: DatabaseConfig,
    pub network: NetworkConfig,
    pub execution: ExecutionConfig,
    pub telegram: TelegramConfig,
}

pub struct ExecutionConfig {
    pub max_slippage_bps: u16,
    pub enable_jito_bundles: bool,
    pub confirm_timeout_seconds: u64,
}
```

---

### `/src/metrics.rs`

**Purpose**: Prometheus metrics export.

**Metrics**:

```rust
// Trades executed
trades_executed_total: Counter

// Trade success/failure
trade_success_total: Counter
trade_failure_total: Counter

// Execution latency
execution_latency_seconds: Histogram

// Position tracking
active_positions: Gauge

// P&L tracking
total_pnl_sol: Gauge
win_rate: Gauge
```

**Endpoint**: `http://localhost:9091/metrics`

---

## Performance Characteristics

### Latency

- **Decision Received → Transaction Sent**: ~50-150ms
- **Transaction Confirmed**: ~1-3 seconds (via Jito)
- **Total Execution Time**: ~1-5 seconds

### Throughput

- **Max Trades/sec**: 10-20 (limited by blockchain finality)
- **Typical**: 1-5 trades/minute

### Resource Usage

- **Memory**: ~200MB
- **CPU**: 10-20% (single core)
- **Network**: ~10KB/s (RPC + UDP)

---

## Error Handling

### Transaction Failures

**Strategy**: Retry 3 times with exponential backoff

**Common Failures**:

- Insufficient balance
- Slippage exceeded
- Bonding curve complete (token graduated)
- Network congestion

### Position Lookup Failures

**Strategy**: Log error, skip SELL (position unknown)

**Prevention**: Always store BuyResult after successful BUY

### Database Failures

**Strategy**: Log error, continue execution (database is for logging only)

**Reason**: Trade execution takes priority over logging

---

## Testing

### Unit Tests

- Bonding curve calculations
- Message parsing
- Position tracking

### Integration Tests

- Mock Brain: Send test TradeDecisions
- Verify: Correct transactions sent to blockchain
- Check: Position tracker updated correctly

---

## Monitoring & Logging

### Key Logs

```rust
info!("🟢 BUY 2u17... | size: 0.0202 SOL | slippage: 5.0% | conf: 84");
info!("✅ BUY EXECUTED: 2u17... | entry: $0.000135 | tokens: 666222");
info!("🔴 SELL 2u17... | entry: $0.000135 | slippage: 5.0%");
info!("✅ SELL EXECUTED: 2u17... | exit: $0.000163 | P&L: +0.0043 SOL (+21.3%)");
error!("❌ BUY FAILED: 2u17... | error: insufficient balance");
```

### Metrics Dashboard

- **Trades Executed**: Count of successful BUYs/SELLs
- **Win Rate**: Percentage of profitable trades
- **Total P&L**: Cumulative profit/loss in SOL
- **Execution Latency**: Time from decision to confirmation
- **Active Positions**: Current open positions count

---

## Summary

The Executor is a **high-performance trade execution engine** that:

- ✅ Receives TradeDecisions from Brain via UDP (port 45110)
- ✅ Executes BUY transactions on Pump.fun bonding curve
- ✅ Stores BuyResult for future SELL execution
- ✅ Executes SELL transactions with profit/loss calculation
- ✅ Tracks active positions in-memory (mint → BuyResult mapping)
- ✅ Sends transactions via Jito bundles for MEV protection
- ✅ Logs all trades to PostgreSQL for analysis
- ✅ Sends real-time Telegram notifications
- ✅ Handles errors gracefully with retries

**Key Design Principles**:

1. **Speed**: Low-latency execution pipeline (~1-5s total)
2. **Safety**: BuyResult tracking prevents orphaned positions
3. **Reliability**: Jito bundles for atomic execution
4. **Transparency**: Real-time notifications and comprehensive logging
5. **Separation**: Pure execution logic, no decision-making (that's Brain's job)
