# Calculations Reference - Complete Mathematical Formulas

**Version**: 1.0  
**Purpose**: Document all mathematical calculations, formulas, and metrics across the trading system  
**Last Updated**: October 28, 2025

---

## Table of Contents

1. [Data-Mining Calculations](#data-mining-calculations)
2. [Brain Calculations](#brain-calculations)
3. [Executor Calculations](#executor-calculations)
4. [Mempool-Watcher Calculations](#mempool-watcher-calculations)

---

## Data-Mining Calculations

### 1. Volume Weighted Average Price (VWAP)

**Purpose**: Calculate average price weighted by volume for a time window.

**Formula**:

```
VWAP = Σ(price × volume) / Σ(volume)
```

**Source**: `data-mining/src/db/aggregator.rs` lines 101-106

**Code**:

```rust
let vwap = if vol_sol > 0.0 {
    total_sol_weighted / vol_sol
} else {
    0.0
};
```

**Calculation Steps**:

1. For each trade: `total_sol_weighted += trade.amount_sol * trade.price`
2. Sum all SOL volume: `vol_sol += trade.amount_sol`
3. Calculate: `vwap = total_sol_weighted / vol_sol`

**Example**:

- Trade 1: 2 SOL at $0.00001 → contributes 0.00002
- Trade 2: 3 SOL at $0.00002 → contributes 0.00006
- Total SOL: 5 SOL
- VWAP = (0.00002 + 0.00006) / 5 = $0.000016

---

### 2. Buyer Concentration Metrics (Top1/Top3/Top5 Share)

**Purpose**: Measure concentration risk - detect potential pump/rug by tracking if top buyers hold too much volume.

**Formula**:

```
TopN_Share = Σ(top N buyer volumes) / Total buy volume
```

**Source**: `data-mining/src/db/aggregator.rs` lines 116-127

**Code**:

```rust
let (top1_share, top3_share, top5_share) = if !buyer_volumes.is_empty() {
    let mut volumes: Vec<f64> = buyer_volumes.values().copied().collect();
    volumes.sort_by(|a, b| b.partial_cmp(a).unwrap()); // Descending sort

    let total_buy_vol: f64 = volumes.iter().sum();
    let top1 = volumes.get(0).copied().unwrap_or(0.0) / total_buy_vol.max(1e-9);
    let top3 = volumes.iter().take(3).sum::<f64>() / total_buy_vol.max(1e-9);
    let top5 = volumes.iter().take(5).sum::<f64>() / total_buy_vol.max(1e-9);

    (top1, top3, top5)
} else {
    (0.0, 0.0, 0.0)
};
```

**Calculation Steps**:

1. Track each buyer's total volume: `buyer_volumes[wallet] += amount_sol`
2. Sort volumes descending
3. Calculate shares:
   - Top1 = largest buyer volume / total buy volume
   - Top3 = sum of 3 largest / total
   - Top5 = sum of 5 largest / total

**Example**:

- Total buy volume: 10 SOL
- Buyer A: 6 SOL
- Buyer B: 2 SOL
- Buyer C: 1 SOL
- Buyer D: 0.5 SOL
- Buyer E: 0.5 SOL

Results:

- Top1 share = 6 / 10 = 0.60 (60%)
- Top3 share = (6 + 2 + 1) / 10 = 0.90 (90%)
- Top5 share = (6 + 2 + 1 + 0.5 + 0.5) / 10 = 1.00 (100%)

**Risk Threshold**: Top1 > 60% is high concentration risk (potential rug).

---

### 3. Window Candle Statistics (OHLC)

**Purpose**: Calculate Open, High, Low, Close prices for time windows.

**Source**: `data-mining/src/db/aggregator.rs` lines 88-96

**Code**:

```rust
for trade in trades {
    if trade.price > high {
        high = trade.price;
    }
    if trade.price < low {
        low = trade.price;
    }
    close = trade.price;  // Last trade price
}

if low == f64::MAX {
    low = 0.0;  // No trades = 0 low
}
```

**Calculation**:

- **Open**: Price of first trade in window (not shown in snippet, from first trade)
- **High**: Maximum price across all trades
- **Low**: Minimum price across all trades
- **Close**: Price of last trade (most recent)

---

### 4. Trade Price Calculation from Event Data

**Purpose**: Extract price per token from Pump.fun trade events.

**Formula**:

```
Price = SOL_amount / Token_amount
```

**Source**: `data-mining/src/parser/mod.rs` lines 450-470 (approximate)

**Code**:

```rust
// From TRADE event:
// - sol_amount (u64, lamports)
// - token_amount (u64, raw units with 6 decimals)

let sol_amount = u64::from_le_bytes(...);  // Lamports
let token_amount = u64::from_le_bytes(...);  // Raw units

// Convert to human-readable
let sol = sol_amount as f64 / 1_000_000_000.0;  // 1 SOL = 1e9 lamports
let tokens = token_amount as f64 / 1_000_000.0;  // 6 decimals

// Calculate price
let price = sol / tokens;  // SOL per token
```

**Example**:

- SOL amount: 20_000_000 lamports = 0.02 SOL
- Token amount: 15_000_000 raw units = 15 tokens
- Price = 0.02 / 15 = 0.001333 SOL per token

---

### 5. Bonding Curve Progress Calculation

**Purpose**: Track how close a token is to graduating to Raydium (bonding curve completion).

**Formula**:

```
Progress = 1.0 - (current_real_token_reserves / initial_real_token_reserves)
```

**Source**: `execution/src/pump_bonding_curve.rs` lines 150-160 (approximate)

**Code**:

```rust
pub fn calculate_progress(&self) -> f64 {
    const INITIAL_REAL_TOKEN_RESERVES: u64 = 793_100_000_000_000; // 793.1M tokens

    if self.real_token_reserves >= INITIAL_REAL_TOKEN_RESERVES {
        return 0.0;  // Not started
    }

    1.0 - (self.real_token_reserves as f64 / INITIAL_REAL_TOKEN_RESERVES as f64)
}
```

**Calculation**:

- Initial reserves: 793,100,000 tokens (with 6 decimals = 793.1M raw)
- As tokens are bought, real_token_reserves decreases
- Progress = 0% (just launched) → 100% (graduated)

**Example**:

- Current reserves: 396,550,000,000,000 (50% of initial)
- Progress = 1.0 - (396.55M / 793.1M) = 1.0 - 0.5 = 0.50 (50% complete)

---

## Brain Calculations

### 1. Follow-Through Score (Momentum Score)

**Purpose**: Calculate 0-100 confidence score based on buyer momentum and volume.

**Formula**:

```
Total Score = (Buyer Score × 0.4) + (Volume Score × 0.4) + (Wallet Quality × 0.2)
```

**Source**: `brain/src/decision_engine/scoring.rs` lines 130-150

**Code**:

```rust
let total_score = (
    (buyer_score as f64 * self.buyer_weight) +      // 0.4 weight
    (volume_score as f64 * self.volume_weight) +    // 0.4 weight
    (wallet_quality_score as f64 * self.quality_weight)  // 0.2 weight
).round() as u8;
```

**Component Calculations**:

#### Buyer Score (0-100)

**Source**: `brain/src/decision_engine/scoring.rs` lines 195-210

```rust
fn score_buyers(&self, buyers_2s: u32) -> u8 {
    if buyers_2s == 0 {
        return 0;
    }

    let normalized = (buyers_2s as f64 / self.max_buyers_2s as f64).min(1.0);

    // Sigmoid curve for smooth scaling
    let score = if buyers_2s <= 5 {
        // Linear: 0-5 buyers → 0-50 points
        (buyers_2s as f64 / 5.0 * 50.0).round() as u8
    } else {
        // Logarithmic: 5-20 buyers → 50-100 points
        let log_factor = ((buyers_2s as f64).ln() - 5.0_f64.ln()) / (20.0_f64.ln() - 5.0_f64.ln());
        (50.0 + log_factor * 50.0).min(100.0).round() as u8
    };

    score
}
```

**Example**:

- 2 buyers in 2s → score = 2/5 × 50 = 20
- 5 buyers in 2s → score = 50 (threshold)
- 10 buyers in 2s → score ≈ 75 (logarithmic scaling)
- 20 buyers in 2s → score = 100 (max)

#### Volume Score (0-100)

**Source**: `brain/src/decision_engine/scoring.rs` lines 220-235

```rust
fn score_volume(&self, vol_5s_sol: f64) -> u8 {
    if vol_5s_sol <= 0.0 {
        return 0;
    }

    let normalized = (vol_5s_sol / self.max_vol_5s).min(1.0);

    // Logarithmic scaling for volume
    let score = if vol_5s_sol <= 2.0 {
        // Linear: 0-2 SOL → 0-40 points
        (vol_5s_sol / 2.0 * 40.0).round() as u8
    } else {
        // Logarithmic: 2-50 SOL → 40-100 points
        let log_factor = (vol_5s_sol.ln() - 2.0_f64.ln()) / (50.0_f64.ln() - 2.0_f64.ln());
        (40.0 + log_factor * 60.0).min(100.0).round() as u8
    };

    score
}
```

**Example**:

- 1 SOL in 5s → score = 1/2 × 40 = 20
- 2 SOL in 5s → score = 40 (threshold)
- 10 SOL in 5s → score ≈ 70
- 50 SOL in 5s → score = 100 (max)

#### Combined Example

- Buyers: 8 (score = 65)
- Volume: 5 SOL (score = 60)
- Wallet quality: 70

Total = (65 × 0.4) + (60 × 0.4) + (70 × 0.2) = 26 + 24 + 14 = **64**

---

### 2. Position Size Calculation

**Purpose**: Calculate optimal position size based on confidence and risk management.

**Strategy**: Confidence-Scaled Sizing

**Formula**:

```
Base Size = min_size + (max_size - min_size) × (confidence / 100)
Final Size = min(Base Size, Portfolio Limit, Heat Limit)
```

**Source**: `brain/src/decision_engine/position_sizer.rs` lines 135-165

**Code**:

```rust
fn calculate_base_size(&self, confidence: u8) -> f64 {
    let confidence_f64 = (confidence as f64 / 100.0).clamp(0.0, 1.0);

    match &self.config.strategy {
        SizingStrategy::ConfidenceScaled { min_size_sol, max_size_sol } => {
            // Linear interpolation
            min_size_sol + (max_size_sol - min_size_sol) * confidence_f64
        }
        // ... other strategies
    }
}

pub fn calculate_size(
    &self,
    confidence: u8,
    active_positions: usize,
    max_positions: usize,
    total_exposure_sol: f64,
) -> f64 {
    // 1. Base size from confidence
    let base_size = self.calculate_base_size(confidence);

    // 2. Portfolio heat limit (don't use more than 80% of remaining capital)
    let remaining_capacity = self.config.portfolio_sol - total_exposure_sol;
    let heat_adjusted = base_size.min(remaining_capacity * 0.8);

    // 3. Position limit scaling (reduce size when near max positions)
    let limit_adjusted = if self.config.scale_down_near_limit && max_positions > 0 {
        let utilization = active_positions as f64 / max_positions as f64;
        if utilization >= 0.8 {
            heat_adjusted * 0.5  // 50% reduction when 80%+ full
        } else if utilization >= 0.6 {
            heat_adjusted * 0.75  // 25% reduction when 60%+ full
        } else {
            heat_adjusted
        }
    } else {
        heat_adjusted
    };

    // 4. Apply absolute limits
    let final_size = limit_adjusted
        .max(self.config.min_position_sol)
        .min(self.config.max_position_sol)
        .min(self.config.portfolio_sol * self.config.max_position_pct / 100.0);

    final_size
}
```

**Example**:
**Config**:

- Min size: 0.05 SOL
- Max size: 0.2 SOL
- Portfolio: 10 SOL
- Max position: 5% of portfolio

**Scenario 1**: Confidence 60, no active positions

- Base = 0.05 + (0.2 - 0.05) × 0.6 = 0.05 + 0.09 = 0.14 SOL
- Heat limit: 10 SOL available, no adjustment
- Position limit: 0 active, no reduction
- Final = 0.14 SOL ✅

**Scenario 2**: Confidence 80, 2/3 active positions, 8 SOL exposed

- Base = 0.05 + (0.2 - 0.05) × 0.8 = 0.17 SOL
- Heat limit: 2 SOL remaining × 0.8 = 1.6 SOL (no reduction)
- Position limit: 2/3 = 67% utilization → 75% reduction = 0.17 × 0.75 = 0.1275 SOL
- Final = 0.1275 SOL ✅

---

### 3. Dynamic Slippage by Position

**Purpose**: Adjust slippage tolerance based on estimated entry position (later positions = more slippage).

**Formula**:

```
Slippage Multiplier = 1 + (base_slippage × position_factor)
Position Factor = position / 50  (capped at 2.0)
```

**Source**: `brain/src/decision_engine/triggers.rs` (not in provided snippets but inferred from executor usage)

**Example Calculation**:

- Base slippage: 3%
- Position #5: factor = 5/50 = 0.1 → slippage = 3% × (1 + 0.1) = 3.3%
- Position #25: factor = 25/50 = 0.5 → slippage = 3% × (1 + 0.5) = 4.5%
- Position #50: factor = 50/50 = 1.0 → slippage = 3% × (1 + 1.0) = 6%
- Position #100: factor = 2.0 (capped) → slippage = 3% × (1 + 2.0) = 9%

---

## Executor Calculations

### 1. Bonding Curve Price Calculation

**Purpose**: Calculate current token price from bonding curve reserves.

**Formula**:

```
Price (SOL per token) = Virtual SOL Reserves / Virtual Token Reserves
```

**Source**: `execution/src/pump_bonding_curve.rs` lines 88-100

**Code**:

```rust
pub fn calculate_price(&self) -> f64 {
    if self.virtual_token_reserves == 0 || self.virtual_sol_reserves == 0 {
        return 0.0;
    }

    let sol_in_lamports = self.virtual_sol_reserves as f64;
    let tokens_in_base_units = self.virtual_token_reserves as f64;

    // Convert to human-readable units
    let sol_amount = sol_in_lamports / LAMPORTS_PER_SOL as f64;  // 1e9
    let token_amount = tokens_in_base_units / 10_f64.powi(6);    // 6 decimals

    // Price = SOL per token
    sol_amount / token_amount
}
```

**Example**:

- Virtual SOL reserves: 30_000_000_000 lamports = 30 SOL
- Virtual token reserves: 1_000_000_000_000 raw = 1,000,000 tokens
- Price = 30 / 1,000,000 = **0.00003 SOL per token**

---

### 2. Bonding Curve BUY Calculation (Constant Product)

**Purpose**: Calculate how many tokens you receive for a given SOL amount.

**Formula**:

```
k = Virtual SOL Reserves × Virtual Token Reserves
New SOL Reserves = Old SOL Reserves + SOL In
New Token Reserves = k / New SOL Reserves
Tokens Out = Old Token Reserves - New Token Reserves
```

**Source**: `execution/src/pump_bonding_curve.rs` lines 105-125

**Code**:

```rust
pub fn calculate_buy_tokens(&self, sol_amount: f64) -> f64 {
    if self.complete {
        return 0.0;
    }

    let sol_lamports = (sol_amount * LAMPORTS_PER_SOL as f64) as u64;

    // Constant product: k = x * y
    let k = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);

    // New SOL reserves after buy
    let new_sol_reserves = self.virtual_sol_reserves + sol_lamports;

    // Calculate new token reserves: new_token_reserves = k / new_sol_reserves
    let new_token_reserves = (k / new_sol_reserves as u128) as u64;

    // Tokens to receive
    let tokens_base_units = self.virtual_token_reserves.saturating_sub(new_token_reserves);

    // Convert to human-readable
    tokens_base_units as f64 / 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32)
}
```

**Example**:

- Virtual SOL reserves: 30 SOL = 30,000,000,000 lamports
- Virtual token reserves: 1,000,000,000,000 raw = 1,000,000 tokens
- k = 30,000,000,000 × 1,000,000,000,000 = 30,000,000,000,000,000,000,000
- SOL in: 0.02 SOL = 20,000,000 lamports

Calculation:

1. New SOL reserves = 30,000,000,000 + 20,000,000 = 30,020,000,000
2. New token reserves = 30e21 / 30,020,000,000 = 999,333,777,778
3. Tokens out = 1,000,000,000,000 - 999,333,777,778 = **666,222,222** raw
4. Human-readable = 666,222,222 / 1e6 = **666.22 tokens**

**Price Impact**: (30.02 - 30) / 30 = 0.067% (very low impact)

---

### 3. Bonding Curve SELL Calculation

**Purpose**: Calculate how much SOL you receive for selling tokens.

**Formula**:

```
k = Virtual SOL Reserves × Virtual Token Reserves
New Token Reserves = Old Token Reserves + Tokens In
New SOL Reserves = k / New Token Reserves
SOL Out = Old SOL Reserves - New SOL Reserves
Net SOL = SOL Out - Fees
```

**Source**: `execution/src/pump_bonding_curve.rs` lines 130-148

**Code**:

```rust
pub fn calculate_sell_sol(&self, token_amount: f64, fee_basis_points: u64) -> f64 {
    if self.complete {
        return 0.0;
    }

    let tokens_base_units = (token_amount * 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32)) as u64;

    // Calculate SOL received using constant product
    let k = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);
    let new_token_reserves = self.virtual_token_reserves + tokens_base_units;
    let new_sol_reserves = (k / new_token_reserves as u128) as u64;

    let sol_received_lamports = self.virtual_sol_reserves.saturating_sub(new_sol_reserves);

    // Apply fee (e.g., 100 bps = 1%)
    let fee_lamports = (sol_received_lamports * fee_basis_points) / 10000;
    let net_sol_lamports = sol_received_lamports - fee_lamports;

    net_sol_lamports as f64 / LAMPORTS_PER_SOL as f64
}
```

**Example**:

- Virtual SOL reserves: 30 SOL
- Virtual token reserves: 1,000,000 tokens
- Selling: 666.22 tokens
- Fee: 100 bps (1%)

Calculation:

1. k = 30e21 (same as before)
2. New token reserves = 1,000,000 + 666.22 = 1,000,666.22
3. New SOL reserves = 30e21 / 1,000,666.22e6 = 29,980,020,000 lamports
4. SOL out = 30,000,000,000 - 29,980,020,000 = 19,980,000 lamports
5. Fee = 19,980,000 × 0.01 = 199,800 lamports
6. Net SOL = 19,980,000 - 199,800 = 19,780,200 lamports = **0.01978 SOL**

**Price Impact**: Received 0.01978 SOL for 0.02 SOL purchase = **98.9% recovery** (1.1% loss from slippage)

---

### 4. Entry Fees Breakdown

**Purpose**: Calculate all fees paid on BUY transaction.

**Components**:

- Jito tip: 100,000,000 lamports (0.1 SOL) when using Jito
- Gas fee: ~5,000 lamports (0.000005 SOL)
- Slippage: Implicit in execution price (not a separate fee)

**Source**: `execution/src/trading.rs` lines 580-595

**Code**:

```rust
let jito_tip = if self.config.use_jito {
    self.config.jito_tip_amount as f64 / 1_000_000_000.0 * sol_price
} else {
    0.0 // No Jito tip when using direct RPC
};
let gas_fee = 0.000005 * sol_price; // ~5000 lamports

let entry_fees = FeeBreakdown {
    jito_tip,
    gas_fee,
    slippage: 0.0, // Not a separate fee - already in execution price
    total: jito_tip + gas_fee,
};
```

**Example** (SOL price = $150):

- Jito tip: 0.1 SOL × $150 = **$15.00**
- Gas fee: 0.000005 SOL × $150 = **$0.00075**
- **Total entry fees: $15.00075** ≈ **$15.00**

**Note**: Slippage is not counted as a "fee" because it's reflected in the actual tokens received. If you set 5% slippage, you might get 5% fewer tokens, but that's market impact, not a fee.

---

### 5. Exit Fees Breakdown

**Purpose**: Calculate all fees paid on SELL transaction.

**Components**: Same as entry fees.

**Source**: `execution/src/trading.rs` lines 940-955

**Code**:

```rust
let jito_tip = if self.config.use_jito {
    self.config.jito_tip_amount as f64 / 1_000_000_000.0 * sol_price
} else {
    0.0
};
let gas_fee = 0.000005 * sol_price;

let exit_fees = FeeBreakdown {
    jito_tip,
    gas_fee,
    slippage: 0.0,
    total: jito_tip + gas_fee,
};
```

**Example** (SOL price = $150):

- Jito tip: **$15.00**
- Gas fee: **$0.00075**
- **Total exit fees: $15.00075** ≈ **$15.00**

---

### 6. Gross Profit Calculation

**Purpose**: Calculate profit before fees.

**Formula**:

```
Current Value (USD) = Token Amount × Current Price × SOL Price
Gross Profit = Current Value - Position Size
```

**Source**: `execution/src/trading.rs` lines 960-965

**Code**:

```rust
// Use FRESH curve price, not cached monitoring price
let current_value_sol = buy_result.token_amount * live_current_price;
let current_value_usd = current_value_sol * sol_price;
let gross_profit = current_value_usd - buy_result.position_size;
```

**Example**:

- Entry: $5.00 position
- Tokens: 666.22 tokens
- Entry price: $0.00003 SOL/token
- Exit price: $0.000036 SOL/token (+20%)
- SOL price: $150

Calculation:

1. Current value = 666.22 × 0.000036 × 150 = **$3.60**
2. Gross profit = $3.60 - $5.00 = **-$1.40** (20% price increase not enough!)

---

### 7. Net Profit Calculation (USD)

**Purpose**: Calculate actual profit after ALL fees.

**Formula**:

```
Total Fees = Entry Fees + Exit Fees
Net Profit (USD) = Gross Profit - Total Fees
```

**Source**: `execution/src/trading.rs` lines 967-970

**Code**:

```rust
let total_fees = buy_result.entry_fees.total + exit_fees.total;
let net_profit = gross_profit - total_fees;
```

**Example** (continuing from above):

- Gross profit: -$1.40
- Entry fees: $15.00
- Exit fees: $15.00
- Total fees: $30.00
- **Net profit: -$1.40 - $30.00 = -$31.40** ❌

**Breakeven Calculation**:
To break even with $5 position and $30 fees:

- Need: $5 + $30 = $35 total value
- Breakeven multiplier: $35 / $5 = **7x** (600% gain!)
- Entry price: $0.00003
- Breakeven exit: $0.00003 × 7 = **$0.00021 SOL/token**

---

### 8. Net Profit Calculation (SOL)

**Purpose**: Calculate profit in SOL for accurate wallet tracking (avoids USD conversion errors).

**Formula**:

```
Entry SOL Spent = (Position Size / SOL Price) + (Entry Fees / SOL Price)
Current Value SOL = Token Amount × Current Price
Exit Fees SOL = Exit Fees / SOL Price
Net Profit SOL = Current Value SOL - Entry SOL Spent - Exit Fees SOL
```

**Source**: `execution/src/trading.rs` lines 972-978

**Code**:

```rust
let entry_fees_sol = buy_result.entry_fees.total / sol_price;
let exit_fees_sol = exit_fees.total / sol_price;
let entry_sol_spent = (buy_result.position_size / sol_price) + entry_fees_sol;
let net_profit_sol = current_value_sol - entry_sol_spent - exit_fees_sol;
```

**Example** (SOL price = $150):

- Position: $5.00 → 0.0333 SOL
- Entry fees: $15.00 → 0.1 SOL
- Entry SOL spent: 0.0333 + 0.1 = **0.1333 SOL**
- Exit fees: $15.00 → 0.1 SOL
- Current value: 666.22 × 0.000036 = **0.024 SOL**
- Net profit: 0.024 - 0.1333 - 0.1 = **-0.2093 SOL**
- In USD: -0.2093 × 150 = **-$31.40** ✅ (matches USD calculation)

---

### 9. Dynamic Slippage (TIER 2)

**Purpose**: Adjust slippage based on estimated position AND pending buy queue depth.

**Formula**:

```
Base Slippage = get_dynamic_slippage(position)  // Position-based
Queue Adjustment = (pending_buys / 10) × 0.02   // +2% per 10 pending
Total Slippage = Base Slippage + Queue Adjustment
```

**Source**: `execution/src/trading.rs` lines 490-510 (approximate)

**Code**:

```rust
pub fn get_dynamic_slippage_with_queue(&self, estimated_position: u32, pending_buys: u32) -> f64 {
    // Base slippage from position
    let base = self.get_dynamic_slippage(estimated_position);

    // Queue depth adjustment: +2% per 10 pending buys
    let queue_factor = (pending_buys as f64 / 10.0) * 0.02;
    let queue_adjusted = queue_factor.min(0.10); // Cap at +10%

    base + queue_adjusted
}

pub fn get_dynamic_slippage(&self, estimated_position: u32) -> f64 {
    // Earlier positions = tighter slippage
    match estimated_position {
        1..=5 => 1.03,    // 3% for top 5
        6..=15 => 1.05,   // 5% for 6-15
        16..=30 => 1.07,  // 7% for 16-30
        _ => 1.09,        // 9% for 31+
    }
}
```

**Example**:

- Position #8, 25 pending buys
- Base slippage: 5% (position 6-15)
- Queue adjustment: (25 / 10) × 2% = 5%
- **Total slippage: 5% + 5% = 10%**

---

### 10. Dynamic Priority Fee (TIER 2)

**Purpose**: Set Solana priority fee based on network congestion.

**Formula**:

```
Priority Fee = max(base_fee, recent_median_fee) × urgency_multiplier
```

**Source**: `execution/src/grpc_client.rs` (not in snippets, but referenced)

**Example Values**:

- Base fee: 10,000 micro-lamports/CU (0.01 lamports)
- Low congestion: 10,000 µL
- Medium congestion: 25,000 µL
- High congestion: 50,000 µL
- Urgent: 100,000 µL

**Cost Example** (200k CU transaction):

- Low: 10,000 × 200,000 = 2,000,000 micro-lamports = **0.000002 SOL** ($0.0003)
- High: 50,000 × 200,000 = 10,000,000 micro-lamports = **0.00001 SOL** ($0.0015)

---

### 11. Breakeven Price Calculation

**Purpose**: Calculate the exit price needed to break even after all fees.

**Formula**:

```
Total Cost = Position Size + Entry Fees + Exit Fees
Tokens Received = Position Size / (Entry Price × SOL Price)
Breakeven Price = (Total Cost / Tokens Received) / SOL Price
```

**Source**: `execution/src/trading.rs` lines 615-620 (logging)

**Code**:

```rust
let total_cost_usd = position_size_usd + entry_fees.total;
let breakeven_price = entry_price * (total_cost_usd / position_size_usd);

info!("   💰 Position Size: ${:.2}", position_size_usd);
info!("   💸 Total Cost (including fees): ${:.2}", total_cost_usd);
info!("   📊 Break-even price: ${:.8}", breakeven_price);
```

**Example**:

- Position: $5.00
- Entry fees: $15.00
- Exit fees: $15.00 (anticipated)
- Total cost: $5 + $15 + $15 = **$35.00**
- Entry price: $0.00003 SOL/token
- Cost multiplier: $35 / $5 = 7x
- **Breakeven exit: $0.00003 × 7 = $0.00021 SOL/token** (700% gain needed!)

---

## Mempool-Watcher Calculations

### 1. Hot Signal Urgency Score

**Purpose**: Calculate urgency level (0-255) for frontrun opportunities.

**Formula**:

```
Amount Score = (SOL Amount / 10) × 100  (capped at 100)
Wallet Score = Win Rate × 100
Urgency = (Amount Score × 0.6) + (Wallet Score × 0.4)
Urgency = clamp(Urgency, 50, 255)  // Minimum 50 to filter noise
```

**Source**: `mempool-watcher/src/heat_calculator.rs` (approximate, not in snippets)

**Code** (reconstructed from documentation):

```rust
pub fn calculate_urgency(amount_sol: f64, wallet_win_rate: f64) -> u8 {
    let amount_score = (amount_sol / 10.0 * 100.0).clamp(0.0, 100.0);
    let wallet_score = wallet_win_rate * 100.0;
    let urgency = (amount_score * 0.6 + wallet_score * 0.4) as u8;
    urgency.clamp(50, 255)
}
```

**Examples**:

**Example 1**: Medium buy from good wallet

- Amount: 5 SOL → score = (5/10) × 100 = 50
- Win rate: 0.8 (80%) → score = 80
- Urgency = (50 × 0.6) + (80 × 0.4) = 30 + 32 = **62**

**Example 2**: Large buy from alpha wallet

- Amount: 10 SOL → score = (10/10) × 100 = 100
- Win rate: 0.9 (90%) → score = 90
- Urgency = (100 × 0.6) + (90 × 0.4) = 60 + 36 = **96**

**Example 3**: Huge buy from proven trader

- Amount: 50 SOL → score = 100 (capped)
- Win rate: 0.95 (95%) → score = 95
- Urgency = (100 × 0.6) + (95 × 0.4) = 60 + 38 = **98**

---

### 2. Alpha Wallet Win Rate Calculation

**Purpose**: Calculate win percentage from wallet trade history.

**Formula**:

```
Win Rate = Realized Wins / (Realized Wins + Realized Losses)
```

**Source**: `data-mining/src/db/mod.rs` (wallet_stats table)

**SQL** (from database schema):

```sql
CREATE TABLE wallet_stats (
    wallet TEXT PRIMARY KEY,
    realized_wins INTEGER DEFAULT 0,
    realized_losses INTEGER DEFAULT 0,
    net_pnl_sol REAL DEFAULT 0.0,
    win_rate REAL DEFAULT 0.0,  -- Calculated: wins / (wins + losses)
    ...
);
```

**Example**:

- Wallet has 82 winning trades
- Wallet has 18 losing trades
- Total: 100 trades
- **Win rate = 82 / 100 = 0.82 (82%)**

**Alpha Criteria**:

- Win rate > 70% ✅
- Net P&L > 10 SOL ✅
- Total trades > 10 ✅

---

## Summary Tables

### Fee Costs by Transaction Type

| Fee Type                  | BUY        | SELL       | Total Round Trip |
| ------------------------- | ---------- | ---------- | ---------------- |
| Jito Tip (0.1 SOL @ $150) | $15.00     | $15.00     | $30.00           |
| Gas Fee (5000 lamports)   | $0.0008    | $0.0008    | $0.0016          |
| **Total per Transaction** | **$15.00** | **$15.00** | **$30.00**       |

### Breakeven Requirements by Position Size

| Position Size | Total Fees | Breakeven Gain | Exit Price Multiplier |
| ------------- | ---------- | -------------- | --------------------- |
| $5.00         | $30.00     | $35.00         | **7.0x** (600%)       |
| $10.00        | $30.00     | $40.00         | **4.0x** (300%)       |
| $20.00        | $30.00     | $50.00         | **2.5x** (150%)       |
| $50.00        | $30.00     | $80.00         | **1.6x** (60%)        |
| $100.00       | $30.00     | $130.00        | **1.3x** (30%)        |

**Key Insight**: With $30 fixed fees, small positions ($5) need **600% gains** just to break even! Larger positions ($50+) have more reasonable breakeven thresholds.

---

### Slippage by Entry Position (TIER 2)

| Position | Base Slippage | +10 Pending | +30 Pending | +50 Pending |
| -------- | ------------- | ----------- | ----------- | ----------- |
| #1-5     | 3%            | 5%          | 9%          | 13%         |
| #6-15    | 5%            | 7%          | 11%         | 15%         |
| #16-30   | 7%            | 9%          | 13%         | 17%         |
| #31+     | 9%            | 11%         | 15%         | 19%         |

---

### Score Calculation Quick Reference

| Metric         | Low      | Medium   | High    | Score Contribution |
| -------------- | -------- | -------- | ------- | ------------------ |
| Buyers (2s)    | 0-2      | 3-8      | 9+      | 0-100 × 40%        |
| Volume (5s)    | 0-1 SOL  | 2-10 SOL | 10+ SOL | 0-100 × 40%        |
| Wallet Quality | <50% win | 50-75%   | 75%+    | 0-100 × 20%        |
| **Combined**   | 0-40     | 41-70    | 71-100  | **Total Score**    |

---

## Validation Checklist

When implementing or debugging calculations, verify:

### Data-Mining

- [ ] VWAP uses trade.amount_sol as weight
- [ ] Top1 share checks for >60% concentration risk
- [ ] Price = SOL / tokens (not tokens / SOL!)
- [ ] All SOL amounts converted from lamports (÷ 1e9)
- [ ] All token amounts converted from raw units (÷ 1e6)

### Brain

- [ ] Follow-through score weights sum to 1.0
- [ ] Position size respects min/max limits
- [ ] Portfolio heat doesn't exceed 90%
- [ ] Slippage increases with position number
- [ ] Confidence score clamped to 0-100

### Executor

- [ ] Bonding curve uses u128 for k (prevents overflow)
- [ ] Price calculated BEFORE trade (not after)
- [ ] Net profit includes BOTH entry and exit fees
- [ ] SOL price fetched from cache (no HTTP calls)
- [ ] Breakeven includes anticipated exit fees

### Mempool-Watcher

- [ ] Urgency score clamped to 50-255 range
- [ ] Alpha wallet requires >70% win rate
- [ ] Signal deduplication prevents spam
- [ ] Wallet performance updated every 60s

---

## Common Pitfalls

1. **Double-counting slippage**: Slippage is NOT a fee - it's implicit in execution price
2. **Using cached prices for exits**: Always fetch FRESH curve state before SELL
3. **Ignoring SOL price volatility**: Track SOL/USD separately from token price
4. **Fee underestimation**: $30 total fees means even 50% token gains lose money on small positions
5. **Integer overflow**: Use u128 for bonding curve k constant product
6. **Decimal confusion**: Lamports (1e9), token decimals (1e6), USD (1e2)

---

## Version History

- **v1.0** (Oct 28, 2025): Initial comprehensive documentation
