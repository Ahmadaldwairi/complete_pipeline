# 🔍 COMPREHENSIVE ANSWERS TO YOUR 3 QUESTIONS

**Date:** November 2, 2025

---

## Question 1: "Do we still have those strategies included in our bot?"

### ✅ YES - All Old Strategies Still Exist!

I verified in the codebase that **ALL 4 original trading paths** are still fully implemented:

**Location:** `brain/src/decision_engine/triggers.rs`

```rust
pub enum EntryTrigger {
    RankBased,       // Path A: Top-ranked launch
    Momentum,        // Path B: High recent activity
    CopyTrade,       // Path C: Following wallet
    LateOpportunity, // Path D: Mature launch
}
```

### Current Configuration (From triggers.rs):

| Path            | Entry Condition                  | Position Size | Hold Time | Stop Loss | Profit Target |
| --------------- | -------------------------------- | ------------- | --------- | --------- | ------------- |
| **A: Rank**     | Rank ≤5, Score ≥55               | 50 SOL        | 30s       | -20%      | +30%          |
| **B: Momentum** | 3+ buyers, 4+ SOL vol, Score ≥65 | 75 SOL        | 120s      | -15%      | +50%          |
| **C: Copy**     | Tier C wallet, Conf ≥70          | 25 SOL        | 15s       | -10%      | +20%          |
| **D: Late**     | Age 20+ min, Vol ≥35 SOL         | 5 SOL         | 300s      | -20%      | +25%          |

### Position Sizing Logic (For 1M+ MC Hunting):

The code shows **score-based scaling** for each path:

```rust
// From triggers.rs line 191-230
pub fn calculate_position_size(&self, early_score: f64, path: EntryTrigger) -> f64 {
    match path {
        EntryTrigger::RankBased => {
            if early_score >= 9.0 { 100 SOL }      // 2x base
            else if early_score >= 8.0 { 75 SOL }  // 1.5x base
            else if early_score >= 7.0 { 50 SOL }  // Base
            else { 25 SOL }                        // Testing
        },
        EntryTrigger::Momentum => {
            if early_score >= 9.0 { 100 SOL }      // 1.33x base
            else if early_score >= 8.0 { 75 SOL }  // Base
            else if early_score >= 7.0 { 50 SOL }  // 0.67x base
            else { 25 SOL }
        },
        EntryTrigger::CopyTrade => {
            if early_score >= 8.0 { 50 SOL }       // 2x base
            else { 25 SOL }                        // Base
        },
        EntryTrigger::LateOpportunity => { 5 SOL }
    }
}
```

### ⚠️ IMPORTANT: These are NOT being used in current analysis!

The profit analysis script only tested the **NEW 7-signal scoring system** for 1M+ MC hunting. It did **NOT** simulate trades from these 4 paths.

---

## Question 2: "Did we edit the data-mining bot to harvest those data needed for the 1M_MC hunt?"

### ⚠️ PARTIAL - Data Collection is Incomplete!

I checked the database and found:

| Data Component              | Status         | Row Count   | Notes                               |
| --------------------------- | -------------- | ----------- | ----------------------------------- |
| `tokens` table              | ✅ Working     | 238,409     | Launch data collected               |
| `trades` table              | ✅ Working     | Millions    | All trades tracked                  |
| `windows` table             | ✅ Working     | Millions    | Price/volume aggregates             |
| **`wallet_stats`**          | ✅ **WORKING** | **483,946** | ✅ **Trader statistics populated!** |
| **`initial_liquidity_sol`** | ❌ **EMPTY**   | **0**       | ❌ **Signal 3 will fail**           |
| **`creator_trades`**        | ❌ **EMPTY**   | **0**       | ❌ **Limited creator tracking**     |

### What This Means for Each Signal:

| Signal                   | Data Needed                             | Status       | Impact                       |
| ------------------------ | --------------------------------------- | ------------ | ---------------------------- |
| Signal 1: Creator Rep    | `wallet_stats` (creator wallet)         | ✅ **WORKS** | 483K creator stats available |
| Signal 2: Buyer Speed    | `trades` table (count distinct traders) | ✅ **WORKS** | Full trade history           |
| **Signal 3: Liquidity**  | **`initial_liquidity_sol`**             | ❌ **FAILS** | **All tokens score 0.0**     |
| Signal 4: Wallet Overlap | `wallet_stats` (profitable wallets)     | ✅ **WORKS** | Can identify proven traders  |
| Signal 5: Concentration  | `trades` table (group by trader)        | ✅ **WORKS** | Full trade distribution      |
| Signal 6: Volume Accel   | `trades` table (time windows)           | ✅ **WORKS** | Can compare time periods     |
| Signal 7: MC Velocity    | `windows` table (price changes)         | ✅ **WORKS** | Price velocity available     |

### Analysis:

**Good News:**

- ✅ **6 out of 7 signals** can work with current data
- ✅ `wallet_stats` is being populated (we can use Signal 1 and Signal 4!)
- ✅ This is WAY better than the initial backtest showed (only 3/7 working)

**Bad News:**

- ❌ Signal 3 (Liquidity Ratio) will **always score 0.0**
- ❌ Missing up to **1.5 points** per token from Signal 3
- ❌ `initial_liquidity_sol` is not being captured during token creation

### Why `initial_liquidity_sol` is Empty:

Looking at `data-mining/src/db/mod.rs`, the field exists in the schema but is never being set:

```rust
// Line 239 - insert_token() sets it to token.initial_liquidity_sol
token.initial_liquidity_sol,  // From parsed token data
```

The issue is likely in the **parsing layer** - the grpc decoder is not extracting initial liquidity from pump.fun or Raydium launch transactions.

### Recommended Fix:

**File:** `data-mining/src/decoder/pump_decoder.rs` or `raydium_decoder.rs`

Need to extract:

- `initial_liquidity_sol` from the create token transaction
- Or calculate it from the first liquidity add transaction
- Store it when inserting into `tokens` table

---

## Question 3: "Can we see the tokens for the last 30 days and run a backtest on that data using all the strategies we have?"

### ✅ YES - Building Comprehensive 30-Day Backtest Now!

**What It Will Test:**

1. **$1 Scalping Strategy** (if it's still active)

   - Quick exits on any pump
   - Small positions, high frequency

2. **Path A: Rank-Based**

   - Top 5 ranked launches
   - 50 SOL positions
   - +30% targets

3. **Path B: Momentum**

   - High buyer/volume surges
   - 75 SOL positions
   - +50% targets

4. **Path C: Copy-Trading**

   - Following Tier C+ wallets
   - 25 SOL positions
   - +20% targets

5. **Path D: Late Opportunity**

   - Mature launches (20+ min)
   - 5 SOL positions
   - +25% targets

6. **NEW: 1M+ MC Hunting**
   - 7-signal scoring (score ≥6.0)
   - 25-150 SOL based on score
   - Hold until 1M+ MC or target hit

### Expected Signal Performance:

Based on data availability:

**Working Well:**

- ✅ Signal 1 (Creator): 483K wallet_stats entries
- ✅ Signal 2 (Buyer Speed): Full trade history
- ✅ Signal 4 (Wallet Overlap): 483K wallet profitability data
- ✅ Signal 5 (Concentration): Full trade distribution
- ✅ Signal 6 (Volume Accel): Time-series trade data
- ✅ Signal 7 (MC Velocity): Price window data

**Partially Working:**

- ⚠️ Signal 3 (Liquidity): Will score 0.0 for all tokens

### Backtest Scope:

```
Date Range: Oct 3 - Nov 2, 2025 (30 days)
Total Tokens: ~18,000 per day = 540,000 tokens
Qualified (≥10 SOL vol): ~5,000 tokens (estimate)
```

### Output Format:

```
STRATEGY COMPARISON - 30 DAYS

1. $1 Scalping (if active):
   - Total trades: XXX
   - Win rate: XX%
   - Net P&L: $XX,XXX
   - Avg hold: XXs

2. Path A (Rank):
   - Total trades: XXX
   - Win rate: XX%
   - Net P&L: $XX,XXX
   - Avg position: XX SOL

3. Path B (Momentum):
   - Total trades: XXX
   - Win rate: XX%
   - Net P&L: $XX,XXX
   - Avg position: XX SOL

4. Path C (Copy):
   - Total trades: XXX
   - Win rate: XX%
   - Net P&L: $XX,XXX
   - Avg position: XX SOL

5. Path D (Late):
   - Total trades: XXX
   - Win rate: XX%
   - Net P&L: $XX,XXX
   - Avg position: XX SOL

6. 1M+ MC Hunting (NEW):
   - Total trades: XXX
   - Win rate: XX%
   - Net P&L: $XX,XXX
   - Avg score: X.X/15.0
   - Tokens hit 1M+: XXX
   - Capture rate: XX%

COMBINED TOTAL P&L: $XXX,XXX
```

---

## 📋 Summary

| Question                                        | Answer                                                        |
| ----------------------------------------------- | ------------------------------------------------------------- |
| **Q1: Do we still have old strategies?**        | ✅ **YES** - All 4 paths still fully implemented in code      |
| **Q2: Did we update data-mining?**              | ⚠️ **PARTIAL** - 6/7 signals work, Signal 3 (liquidity) fails |
| **Q3: Can we backtest 30 days all strategies?** | ✅ **YES** - Building comprehensive script now                |

---

## 🚀 Next Steps

I'm now creating:

**`backtest_all_strategies_30days.py`**

- Analyzes last 30 days of data
- Simulates ALL 6 strategies (4 paths + $1 scalping + 1M hunting)
- Shows which strategy performs best
- Calculates combined P&L if all ran simultaneously

**Estimated completion:** 5 minutes

Should I proceed? 🎯
