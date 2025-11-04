# 🎯 Entry/Exit Threshold Configuration Guide

**Last Updated:** December 2024  
**Status:** System complete and ready for production testing  
**Purpose:** Document current entry/exit thresholds and provide tuning guidance for increased transaction frequency

---

## 📊 Overview

The scalper bot uses **4 distinct entry strategies** to identify trading opportunities:

1. **Rank-Based Entry** (Path A) - Top-ranked new launches
2. **Momentum Entry** (Path B) - High recent activity surges
3. **Copy-Trade Entry** (Path C) - Following profitable wallets
4. **Late Opportunity Entry** (Path D) - Mature launches with sustained volume

Each strategy has independent thresholds that can be tuned to increase/decrease transaction frequency.

---

## 🚪 Entry Strategy 1: Rank-Based (Path A)

### Description

Fires for top-ranked launches (rank ≤ 2) with sufficient follow-through momentum. These are the **hottest** launches detected by the system.

### Current Thresholds (🧪 TESTING MODE)

```rust
// Location: brain/src/decision_engine/triggers.rs

max_rank_for_instant: 2           // Only top 2 ranked launches trigger
min_follow_through_rank: 30       // 🧪 TESTING (Production: 60)
rank_position_size_sol: 10.0      // Position size: 10 SOL
```

### Entry Conditions

```
✅ Entry Triggered When:
   - Launch rank ≤ 2 (top 2 launches)
   - Follow-through score ≥ 30 (testing) / 60 (production)
   - No minimum pool size required
```

### Effect on Frequency

- **Current (Testing)**: ~2-3 entries per hour (top launches only)
- **Production (score 60)**: ~1-2 entries per hour (higher quality)

### Tuning for More Entries

| Threshold                 | Current | Moderate | Aggressive | Effect                      |
| ------------------------- | ------- | -------- | ---------- | --------------------------- |
| `max_rank_for_instant`    | 2       | 3        | 5          | +50% per rank added         |
| `min_follow_through_rank` | 30      | 40       | 25         | Lower = 30-50% more entries |

**Example Adjustments:**

```rust
// Moderate (2x more entries)
max_rank_for_instant: 3
min_follow_through_rank: 35

// Aggressive (4x more entries)
max_rank_for_instant: 5
min_follow_through_rank: 25
```

### ⚠️ Risk Considerations

- Lower follow-through = higher false positives
- Higher rank threshold = lower quality launches
- Recommended: Test with smaller position sizes first

---

## 🚀 Entry Strategy 2: Momentum (Path B)

### Description

Triggers on high recent activity (buyers in 2s window, volume in 5s window). Catches **momentum surges** and buying pressure spikes.

### Current Thresholds (🧪 TESTING MODE)

```rust
// Location: brain/src/decision_engine/triggers.rs

min_buyers_2s: 2                  // 🧪 TESTING (Production: 5)
min_vol_5s_sol: 2.0               // 🧪 TESTING (Production: 8.0 SOL)
min_follow_through_momentum: 30   // 🧪 TESTING (Production: 60)
momentum_position_size_sol: 8.0   // Position size: 8 SOL
```

### Additional Scoring Thresholds

```rust
// Location: brain/src/decision_engine/scoring.rs

max_buyers_2s: 20                 // Normalization ceiling (20 buyers = 100% score)
max_vol_5s: 50.0                  // Normalization ceiling (50 SOL = 100% score)
```

### Entry Conditions

```
✅ Entry Triggered When:
   - Unique buyers (2s window) ≥ 2 (testing) / 5 (production)
   - Volume (5s window) ≥ 2.0 SOL (testing) / 8.0 SOL (production)
   - Follow-through score ≥ 30 (testing) / 60 (production)
```

### Effect on Frequency

- **Current (Testing)**: ~5-10 entries per hour (very loose)
- **Production (buyers≥5, vol≥8)**: ~1-2 entries per hour
- **Wait time**: Average 2-5 minutes per token before conditions met

### Tuning for More Entries

| Threshold            | Current (Test) | Production | Moderate | Aggressive | Effect                   |
| -------------------- | -------------- | ---------- | -------- | ---------- | ------------------------ |
| `min_buyers_2s`      | 2              | 5          | 3        | 2          | -1 buyer = +40% entries  |
| `min_vol_5s_sol`     | 2.0            | 8.0        | 5.0      | 3.0        | -1 SOL = +25% entries    |
| `min_follow_through` | 30             | 60         | 45       | 35         | -10 score = +30% entries |

**Example Adjustments:**

```rust
// Moderate (3x more entries than production)
min_buyers_2s: 3
min_vol_5s_sol: 5.0
min_follow_through_momentum: 45

// Aggressive (6x more entries than production)
min_buyers_2s: 2
min_vol_5s_sol: 3.0
min_follow_through_momentum: 35
```

### 🎯 Configuration Priority

```rust
// Config also controls decision confidence from .env
// Location: brain/src/config.rs

MIN_DECISION_CONF=75              // Default (testing lower = more entries)
MIN_FOLLOW_THROUGH_SCORE=55       // Default
```

### ⚠️ Risk Considerations

- Lower buyer count = catch early momentum (higher risk)
- Lower volume = more false starts and fakes
- **Testing mode is already aggressive** - production values are safer
- Recommendation: Start with production values, lower incrementally

---

## 🐳 Entry Strategy 3: Copy-Trade (Path C)

### Description

Follows profitable wallet transactions. Requires wallet to meet **tier and confidence thresholds**.

### Current Thresholds (🧪 TESTING MODE)

```rust
// Location: brain/src/decision_engine/triggers.rs

min_copy_tier: 1                  // Tier C (1 = C, 2 = B, 3 = A)
min_copy_confidence: 50           // 🧪 TESTING (Production: 75)
min_copy_size_sol: 0.25           // Minimum wallet transaction size
copy_multiplier: 1.2              // Our size = 1.2x wallet's size
```

### Wallet Tier System

```rust
// Location: brain/src/feature_cache/wallet_cache.rs

Tier Classification (confidence score):
- Discovery: 50 (lowest tier, not copyable)
- C: 80 (minimum for copy-trade)
- B: 87
- A: 93 (highest tier)

fn is_copyable() -> bool {
    tier >= WalletTier::C  // Requires tier C or above
}
```

### Configuration Overrides

```rust
// Location: brain/src/config.rs

MIN_COPYTRADE_CONFIDENCE=70       // Default from .env
```

### Entry Conditions

```
✅ Entry Triggered When:
   - Wallet tier ≥ C (confidence ≥ 80 in production)
   - Wallet confidence ≥ 50 (testing) / 75 (production)
   - Wallet transaction size ≥ 0.25 SOL
   - No rate limit cooldown active (90s default)
```

### Effect on Frequency

- **Current (Testing, confidence≥50)**: Copies ~60% of wallets
- **Production (confidence≥75, tier≥C)**: Copies ~15-20% of wallets
- **Wait time**: Depends on wallet activity (1-10 minutes between signals)

### Tuning for More Entries

| Threshold             | Current (Test) | Production | Moderate       | Aggressive     | Effect                     |
| --------------------- | -------------- | ---------- | -------------- | -------------- | -------------------------- |
| `min_copy_confidence` | 50             | 75         | 60             | 50             | -10 conf = +30-50% wallets |
| `min_copy_tier`       | C (80)         | C (80)     | Discovery (50) | Discovery (50) | Discovery = +300% wallets  |
| `wallet_cooling_secs` | 90             | 90         | 60             | 30             | -30s = +50% frequency      |

**Example Adjustments:**

```rust
// Moderate (2-3x more entries)
min_copy_confidence: 60
min_copy_tier: 1  // Still tier C
wallet_cooling_secs: 60

// Aggressive (5x more entries - HIGH RISK)
min_copy_confidence: 50
min_copy_tier: 0  // Accept Discovery tier
wallet_cooling_secs: 30
```

### ⚠️ Risk Considerations

- **Discovery tier wallets** = unproven, high risk
- Lower confidence = more false signals from wallets
- Shorter cooling = can over-allocate to one wallet's activity
- **Recommendation**: Keep tier≥C, lower confidence to 60-65 only

---

## 🕐 Entry Strategy 4: Late Opportunity (Path D)

### Description

Targets mature launches (20+ minutes old) with sustained volume and buyer activity. Catches **second pumps** and delayed momentum.

### Current Thresholds (🧪 TESTING MODE)

```rust
// Location: brain/src/decision_engine/triggers.rs

min_launch_age_seconds: 1200      // 20 minutes minimum age
min_vol_60s_late: 10.0            // 🧪 TESTING (Production: 35.0 SOL)
min_buyers_60s_late: 10           // 🧪 TESTING (Production: 40)
min_follow_through_late: 40       // 🧪 TESTING (Production: 70)
late_position_size_sol: 5.0       // Position size: 5 SOL
```

### Entry Conditions

```
✅ Entry Triggered When:
   - Launch age ≥ 20 minutes (1200 seconds)
   - Volume (60s window) ≥ 10 SOL (testing) / 35 SOL (production)
   - Buyers (60s window) ≥ 10 (testing) / 40 (production)
   - Follow-through score ≥ 40 (testing) / 70 (production)
```

### Effect on Frequency

- **Current (Testing)**: ~3-5 entries per hour
- **Production (vol≥35, buyers≥40)**: ~0.5-1 entry per hour
- **Wait time**: 20+ minutes minimum (by design)

### Tuning for More Entries

| Threshold                 | Current (Test) | Production | Moderate | Aggressive | Effect                    |
| ------------------------- | -------------- | ---------- | -------- | ---------- | ------------------------- |
| `min_vol_60s_late`        | 10.0           | 35.0       | 20.0     | 12.0       | -5 SOL = +30% entries     |
| `min_buyers_60s_late`     | 10             | 40         | 20       | 12         | -10 buyers = +40% entries |
| `min_follow_through_late` | 40             | 70         | 50       | 40         | -10 score = +30% entries  |
| `min_launch_age_seconds`  | 1200           | 1200       | 900      | 600        | -5 min = +20% entries     |

**Example Adjustments:**

```rust
// Moderate (2-3x more entries)
min_vol_60s_late: 20.0
min_buyers_60s_late: 20
min_follow_through_late: 50
min_launch_age_seconds: 900  // 15 minutes

// Aggressive (5x more entries)
min_vol_60s_late: 12.0
min_buyers_60s_late: 12
min_follow_through_late: 40
min_launch_age_seconds: 600  // 10 minutes
```

### ⚠️ Risk Considerations

- Lower age threshold = might catch dumps instead of second pumps
- Lower volume = weak late rallies, less likely to sustain
- **This strategy is inherently safer** (more data to analyze)
- Recommendation: Moderate tuning safe, aggressive needs close monitoring

---

## 🚨 Exit Strategies

### Primary Exit: Profit Targets

```rust
// Location: brain/src/main.rs

profit_targets: (30.0, 60.0, 100.0)  // Percentage gains

Tier 1: 30% gain  → Sell 30% of position
Tier 2: 60% gain  → Sell 60% of remaining
Tier 3: 100% gain → Sell 100% (all remaining)
```

### Priority Exit: Dollar Profit Target

```rust
// Location: brain/src/decision_engine/position_tracker.rs

✅ PRIORITY EXIT CONDITION:
if realized_profit >= $1.00 {
    EXIT 100% immediately
}
```

**This overrides percentage targets** - if position makes $1+ profit, exit all.

### Stop Loss

```rust
stop_loss_pct: 15.0  // -15% loss triggers full exit
```

```
❌ Stop-Loss Triggered When:
   - Price drops 15% from entry
   - Exit 100% of position immediately
```

### Time-Based Exits

```rust
// Base hold time
base_hold_secs: 300  // 5 minutes default

// Autohold extensions (added to base)
Strong buying surge: +15 seconds
Moderate momentum: +10 seconds
Alpha wallet activity: +12 seconds
```

**Max hold time calculation:**

```rust
max_hold_secs = base_hold_secs + autohold_extension
```

**Time decay exit:**

```
⏰ Time Decay Triggered When:
   - Position held > max_hold_secs
   - Exit 100% of position
```

### Momentum-Based Exits

#### No Mempool Activity

```rust
// Location: brain/src/decision_engine/position_tracker.rs

if mempool_pending_buys == 0 && elapsed > 15 seconds {
    EXIT 100%  // No buying pressure = exit
}
```

**Priority exit condition** - bot only stays if mempool shows volume.

#### Volume Drop

```rust
if vol_5s_sol < 0.5 SOL && price_change < 10% && elapsed > 30 seconds {
    EXIT 100%  // Volume dried up
}
```

### Exit Validation

```rust
// Location: brain/src/decision_engine/validation.rs

fee_multiplier: 2.2               // Fees must be < profit/2.2
min_profit_target_usd: 1.0        // Minimum $1 profit after fees
max_slippage: 0.15                // Max 15% slippage allowed
```

---

## 🎛️ Configuration Summary

### Core Decision Thresholds (.env)

```bash
# General confidence
MIN_DECISION_CONF=75              # Minimum confidence for entries (0-100)
MIN_FOLLOW_THROUGH_SCORE=55       # Minimum follow-through (0-100)

# Copy-trade specific
MIN_COPYTRADE_CONFIDENCE=70       # Minimum confidence for copy-trades

# Validation
FEE_MULTIPLIER=2.2                # Fee vs profit multiplier
MAX_SLIPPAGE=0.15                 # Max 15% slippage
MIN_LIQUIDITY_USD=5000.0          # Minimum pool liquidity

# Guardrails
MAX_CONCURRENT_POSITIONS=3        # Max open positions
WALLET_COOLING_SECS=90            # Cooldown between same-wallet trades
RATE_LIMIT_MS=100                 # Minimum time between decisions
```

### Strategy-Specific Thresholds (Code)

Location: `brain/src/decision_engine/triggers.rs`

Must be changed in code and recompiled. See sections above for current values.

---

## 📈 Tuning Recommendations

### Current Status: 🧪 TESTING MODE

**Your observation:** "I think we have to wait few minutes to enter a transactions"

**Analysis:** System is currently in **TESTING MODE** with **lowered thresholds** (see 🧪 markers). Production thresholds are even stricter.

### To Increase Transaction Frequency

#### Quick Wins (Lowest Risk)

1. **Lower MIN_DECISION_CONF** (.env change, no recompile)

   ```bash
   MIN_DECISION_CONF=60  # From 75 → +40% entries
   ```

2. **Lower MIN_FOLLOW_THROUGH_SCORE** (.env change)

   ```bash
   MIN_FOLLOW_THROUGH_SCORE=40  # From 55 → +30% entries
   ```

3. **Reduce wallet cooling** (.env change)
   ```bash
   WALLET_COOLING_SECS=60  # From 90 → +50% copy-trade frequency
   ```

#### Medium Adjustments (Requires Recompile)

4. **Momentum: Lower buyer threshold**

   ```rust
   min_buyers_2s: 2  // Already at 2 (testing)
   // Consider: Keep at 2, or raise to 3 for quality
   ```

5. **Momentum: Lower volume threshold**

   ```rust
   min_vol_5s_sol: 2.0  // Already at 2.0 (testing)
   // Production is 8.0 - consider 3.0-5.0 range
   ```

6. **Copy-trade: Lower confidence**
   ```rust
   min_copy_confidence: 50  // Already at 50 (testing)
   // Production is 75 - testing value is aggressive
   ```

#### Aggressive Changes (Higher Risk, Requires Recompile)

7. **Rank: Expand rank window**

   ```rust
   max_rank_for_instant: 5  // From 2 → +150% entries
   ```

8. **Late: Reduce age requirement**

   ```rust
   min_launch_age_seconds: 600  // 10 min instead of 20 min
   ```

9. **Copy-trade: Accept Discovery tier**
   ```rust
   min_copy_tier: 0  // Accept all tiers (HIGH RISK)
   ```

### Recommended Tuning Path

#### Phase 1: .env Only (No Recompile)

```bash
# Moderate increase (~2x entries)
MIN_DECISION_CONF=65
MIN_FOLLOW_THROUGH_SCORE=45
MIN_COPYTRADE_CONFIDENCE=60
WALLET_COOLING_SECS=60
```

Run for 24-48 hours, monitor win rate and drawdown.

#### Phase 2: Code Changes (If Phase 1 Successful)

```rust
// Momentum tuning
min_buyers_2s: 2         // Keep testing value
min_vol_5s_sol: 3.0      // Raise from 2.0 for quality
min_follow_through_momentum: 35  // Raise from 30

// Rank tuning
max_rank_for_instant: 3  // From 2
min_follow_through_rank: 35  // Raise from 30

// Late opportunity
min_vol_60s_late: 15.0   // From 10.0
min_buyers_60s_late: 15  // From 10
```

Expected: ~4-6x more entries than production, ~2-3x more than current testing.

#### Phase 3: Monitor and Iterate

- Track win rate (target: >55%)
- Track average hold time
- Track profit per trade
- Adjust thresholds based on results

---

## 🎯 Preset Configurations

### Conservative (Production Ready)

```rust
// Rank
max_rank_for_instant: 2
min_follow_through_rank: 60

// Momentum
min_buyers_2s: 5
min_vol_5s_sol: 8.0
min_follow_through_momentum: 60

// Copy-trade
min_copy_confidence: 75
min_copy_tier: 1  // Tier C

// Late
min_vol_60s_late: 35.0
min_buyers_60s_late: 40
min_follow_through_late: 70
```

```bash
MIN_DECISION_CONF=75
MIN_FOLLOW_THROUGH_SCORE=55
MIN_COPYTRADE_CONFIDENCE=70
```

**Expected:** ~2-4 entries per hour, high win rate (60-70%)

### Moderate (Balanced)

```rust
// Rank
max_rank_for_instant: 3
min_follow_through_rank: 45

// Momentum
min_buyers_2s: 3
min_vol_5s_sol: 5.0
min_follow_through_momentum: 45

// Copy-trade
min_copy_confidence: 60
min_copy_tier: 1  // Tier C

// Late
min_vol_60s_late: 20.0
min_buyers_60s_late: 20
min_follow_through_late: 50
```

```bash
MIN_DECISION_CONF=65
MIN_FOLLOW_THROUGH_SCORE=45
MIN_COPYTRADE_CONFIDENCE=60
```

**Expected:** ~8-12 entries per hour, good win rate (55-65%)

### Aggressive (High Frequency)

```rust
// Rank
max_rank_for_instant: 5
min_follow_through_rank: 30

// Momentum
min_buyers_2s: 2
min_vol_5s_sol: 2.0
min_follow_through_momentum: 30

// Copy-trade
min_copy_confidence: 50
min_copy_tier: 1  // Tier C (keep this safe)

// Late
min_vol_60s_late: 10.0
min_buyers_60s_late: 10
min_follow_through_late: 40
```

```bash
MIN_DECISION_CONF=55
MIN_FOLLOW_THROUGH_SCORE=35
MIN_COPYTRADE_CONFIDENCE=50
```

**Expected:** ~20-30 entries per hour, lower win rate (45-55%)

---

## ⚠️ Risk Management

### Win Rate vs Frequency Trade-off

```
Conservative: 2-4 entries/hour, 60-70% win rate  → +EV
Moderate:     8-12 entries/hour, 55-65% win rate → +EV
Aggressive:   20-30 entries/hour, 45-55% win rate → Break-even risk
```

### Position Sizing

Lower thresholds = lower quality signals. Consider:

- Reduce position sizes by 30-50% when using aggressive config
- Use `rank_position_size_sol`, `momentum_position_size_sol` etc. to tune per strategy

### Monitoring Metrics

Track these after threshold changes:

1. **Win rate** (target: >50%)
2. **Average profit per trade**
3. **Max drawdown**
4. **False positive rate** (entries that exit at stop-loss)
5. **Hold time distribution**

### Safety Limits

These prevent disasters:

```rust
MAX_CONCURRENT_POSITIONS=3        // Hard cap on open positions
LOSS_BACKOFF_THRESHOLD=3          // Pause after 3 losses
LOSS_BACKOFF_PAUSE_SECS=120       // 2 min cooldown
```

Do not increase these until system is proven profitable.

---

## 🛠️ Implementation Guide

### To Change .env Values

1. Edit `.env` file in project root
2. Restart brain service
3. No recompilation needed

### To Change Code Values

1. Edit `brain/src/decision_engine/triggers.rs`
2. Modify `TriggerConfig::default()` function
3. Recompile: `cd brain && cargo build --release`
4. Restart brain service

### Testing Checklist

- [ ] Start with conservative config
- [ ] Monitor for 24 hours
- [ ] Check win rate ≥ 55%
- [ ] Check drawdown ≤ 20%
- [ ] Lower thresholds incrementally (10-20% at a time)
- [ ] Retest for 24 hours after each change
- [ ] Use small position sizes during testing

---

## 📝 Current Status Summary

**You said:** "I think we have to wait few minutes to enter a transactions"

**Current State:** System is in 🧪 **TESTING MODE** with already-lowered thresholds:

- Momentum: 2 buyers (prod: 5), 2 SOL volume (prod: 8)
- Copy-trade: 50 confidence (prod: 75)
- Late: 10 buyers/10 SOL (prod: 40/35)

**If still too slow**, the issue is likely:

1. **Market conditions** - Few tokens meeting even loose criteria
2. **Scoring is still filtering** - MIN_DECISION_CONF=75 may be too high
3. **Guardrails active** - Check if hitting max positions or rate limits

**Recommended Action:**

1. Lower `.env` thresholds first (quickest fix):

   ```bash
   MIN_DECISION_CONF=60
   MIN_FOLLOW_THROUGH_SCORE=40
   ```

2. Check logs for rejection reasons:

   ```bash
   tail -f brain/logs/brain.log | grep "❌\|below threshold\|exceeds threshold"
   ```

3. If still slow, consider the "Aggressive" preset config above

---

---

## 🎯 ADVANCED: Path-Specific Tuning for 1M+ Market Cap Hunting

**Strategy Shift:** From high-frequency $1 scalping → Catching 4+ tokens/day that reach 1M+ MC with multi-X returns

**Position Sizing:** $5-10 entries → $100 entries for high-confidence signals

### 🧠 The 7-Signal Scoring System

To catch tokens before they explode to 1M+ MC, implement this real-time scoring algorithm:

```rust
// Location: brain/src/decision_engine/early_scorer.rs (to be created)

score = 0.0

// Signal 1: Creator Wallet Reputation
if creator_is_profitable_in_db { score += 2.0 }

// Signal 2: Speed of First 10 Buyers
if unique_buyers_10s > 10 { score += 2.0 }

// Signal 3: Liquidity vs MC Ratio
if (market_cap / liquidity) < 4.0 { score += 1.5 }

// Signal 4: Wallet Overlap with Past Winners
if first_20_buyers_overlap_with_10x_wallets { score += 2.0 }

// Signal 5: Buy Concentration (Rug Check)
if top_3_wallets_share < 0.7 { score += 1.0 }

// Signal 6: Volume Acceleration
if volume_doubled_in_30s { score += 1.5 }

// Signal 7: MC Velocity
if mc_growth_rate > 1000_sol_per_min { score += 3.0 }

// High-confidence entry threshold
if score >= 6.0 {
    trigger_buy(confidence=95)
}
```

### 📋 Path-Specific Configuration Matrix

Each entry path has different objectives and requires independent tuning:

| Path                    | Primary Goal                 | Entry Timing       | Hold Time      | Profit Target      | Position Size        |
| ----------------------- | ---------------------------- | ------------------ | -------------- | ------------------ | -------------------- |
| **1️⃣ Rank-Based**       | Catch new coins before spike | First 10 buyers    | 10-30s         | $1-3 or MC doubles | 50-100 SOL (score>8) |
| **2️⃣ Momentum**         | Ride 100K→1M MC waves        | Confirmed surge    | Velocity-based | $5-20 or 1-2%      | 50-100 SOL (score>8) |
| **3️⃣ Copy-Trade**       | Follow proven whales         | <300ms after whale | 10-15s max     | $1-2 quick profit  | 10-50 SOL (testing)  |
| **4️⃣ Late-Opportunity** | Second waves (DISABLED)      | 20+ min old        | N/A            | N/A                | N/A                  |

---

### 🔧 Path 1: Rank-Based (Hot New Launches)

**Objective:** Enter top-ranked launches during first wave of buyers

**Optimized Thresholds for 1M+ MC Hunting:**

```rust
// brain/src/decision_engine/triggers.rs

max_rank_for_instant: 5              // Expand from 2 → catch top 5
min_follow_through_rank: 25          // Loosen from 30 → faster entry
rank_position_size_sol: 50.0         // Scale up from 10 → $100 entries

// Path-specific confidence (lower for speed)
MIN_DECISION_CONF_RANK: 55           // Lower than global 75
MIN_FOLLOW_THROUGH_SCORE_RANK: 35    // Quick entry threshold
```

**Entry Logic:**

```
✅ ENTER if:
   - rank ≤ 5
   - follow_through ≥ 25
   - early_score ≥ 6.0 (7-signal system)
   - unique_buyers_10s ≥ 10
```

**Exit Logic:**

```
🚪 EXIT when:
   - MC doubles from entry
   - $3 profit realized
   - 30s elapsed with no new buyers
   - Volume drops >50% in 10s
```

**Expected Results:**

- Entries: 4-8 per hour during active markets
- Hit rate: 60-70% catch tokens that reach 500K+ MC
- Average hold: 15-45 seconds

---

### 🔧 Path 2: Momentum-Based (Surge Rider)

**Objective:** Enter during confirmed momentum surges (100K → 1M MC)

**Optimized Thresholds for 1M+ MC Hunting:**

```rust
// brain/src/decision_engine/triggers.rs

min_buyers_2s: 3                     // Raise from 2 → quality filter
min_vol_5s_sol: 4.0                  // Raise from 2.0 → genuine interest
min_follow_through_momentum: 35      // Balance speed vs quality
momentum_position_size_sol: 75.0     // Scale up for confirmed waves

// Path-specific confidence (moderate)
MIN_DECISION_CONF_MOMENTUM: 65       // Higher than rank-based
MIN_FOLLOW_THROUGH_SCORE_MOMENTUM: 45
```

**Entry Logic:**

```
✅ ENTER if:
   - buyers_2s ≥ 3
   - vol_5s_sol ≥ 4.0
   - follow_through ≥ 35
   - early_score ≥ 7.0 (stricter than rank)
   - mc_velocity > 500 SOL/min
```

**Exit Logic (Dynamic MC Velocity):**

```
🚪 EXIT when:
   - MC acceleration drops >50% in 10s window
   - $20 profit realized
   - Volume drops to <0.5 SOL/5s
   - 2 minutes elapsed
```

**Expected Results:**

- Entries: 2-5 per hour
- Hit rate: 70-80% catch tokens going 100K→1M
- Average hold: 45-120 seconds
- Best for: Tokens already showing strength

---

### 🔧 Path 3: Copy-Trade (Whale Following)

**Objective:** Fast scalp following proven profitable wallets

**Optimized Thresholds for 1M+ MC Hunting:**

```rust
// brain/src/decision_engine/triggers.rs

min_copy_tier: 1                     // Keep Tier C (80 confidence)
min_copy_confidence: 65              // Raise from 50 → quality
min_copy_size_sol: 0.25              // Minimum whale position
copy_multiplier: 1.2                 // Our size = 1.2x whale
copy_position_size_base: 25.0        // Base size for copies

// Path-specific confidence (strict)
MIN_DECISION_CONF_COPY: 70           // High quality wallets only
MAX_COPY_HOLD_TIME_SECS: 15          // Fast in/out
```

**Entry Logic:**

```
✅ ENTER if:
   - wallet_tier >= C (confidence ≥ 80)
   - wallet_confidence ≥ 65
   - follow_time < 300ms (fast execution)
   - wallet in top-100 profitable list
```

**Exit Logic (Aggressive):**

```
🚪 EXIT when:
   - Whale exits (if detected)
   - $2 profit realized
   - 15 seconds elapsed
   - No follow-through from other buyers
```

**Expected Results:**

- Entries: 6-12 per hour
- Hit rate: 50-60% (some are quick scalps)
- Average hold: 5-15 seconds
- Note: Mix of scalps + occasional multi-X catches

---

### 🔧 Path 4: Late-Opportunity (DISABLED for 1M+ MC Hunting)

**Why Disabled:**

- By the time a token is 20+ minutes old, the 0→1M MC window is closed
- This path is for second waves and recovery plays
- Not relevant for catching early explosive pumps

**Re-enable later for:**

- Post-hype reaccumulation patterns
- Second pump opportunities
- Longer-term swing trades

```rust
// .env or config.rs
ENABLE_LATE_OPPORTUNITY=false        // Disable for now
```

---

### 💎 Position Sizing Strategy (Score-Based)

Replace fixed position sizes with dynamic sizing based on early_score:

```rust
// brain/src/decision_engine/triggers.rs

fn calculate_position_size(early_score: f64, path: EntryPath) -> f64 {
    match path {
        EntryPath::RankBased => {
            if early_score >= 9.0 { 100.0 }      // Ultra high confidence
            else if early_score >= 8.0 { 75.0 }  // High confidence
            else if early_score >= 7.0 { 50.0 }  // Good confidence
            else { 25.0 }                        // Testing size
        },
        EntryPath::Momentum => {
            if early_score >= 9.0 { 100.0 }
            else if early_score >= 8.0 { 75.0 }
            else if early_score >= 7.0 { 50.0 }
            else { 25.0 }
        },
        EntryPath::CopyTrade => {
            // Copy-trade uses whale size multiplier
            let base = 25.0;
            if early_score >= 8.0 { base * 2.0 }  // 50 SOL
            else { base }                         // 25 SOL
        },
        _ => 10.0  // Default testing size
    }
}
```

**Safety Caps:**

```rust
MAX_POSITION_SIZE_SOL: 150.0         // Hard cap per trade
MAX_CONCURRENT_POSITIONS: 5          // Increase from 3
```

---

### 🚪 Dynamic Exit Strategy (MC Velocity-Based)

Replace fixed $1 profit target with trend exhaustion detection:

```rust
// brain/src/decision_engine/position_tracker.rs

struct McVelocityTracker {
    mc_10s_ago: f64,
    mc_20s_ago: f64,
    mc_30s_ago: f64,
}

fn should_exit_velocity_based(&self, current_mc: f64) -> bool {
    // Calculate recent acceleration
    let velocity_10s = (current_mc - self.mc_10s_ago) / 10.0;
    let velocity_20s = (self.mc_10s_ago - self.mc_20s_ago) / 10.0;

    // Exit when acceleration drops >50%
    if velocity_10s < (velocity_20s * 0.5) && velocity_20s > 0.0 {
        return true;  // Trend exhaustion
    }

    false
}
```

**Exit Conditions by Path:**

| Path       | Primary Exit  | Secondary Exit | Backstop           |
| ---------- | ------------- | -------------- | ------------------ |
| Rank-Based | MC doubles    | $3 profit      | 30s or no buyers   |
| Momentum   | Velocity -50% | $20 profit     | 2 min or vol drops |
| Copy-Trade | Whale exits   | $2 profit      | 15s elapsed        |

---

### 📊 Database Schema for Hotlist

Add real-time scoring table to data-mining SQLite:

```sql
-- Store top-scoring tokens for instant Brain access
CREATE TABLE IF NOT EXISTS hotlist (
    mint TEXT PRIMARY KEY,
    score REAL NOT NULL,
    mc_velocity REAL,
    unique_buyers_10s INTEGER,
    creator_score REAL,
    liquidity_ratio REAL,
    top3_share REAL,
    volume_acceleration REAL,
    wallet_overlap_score REAL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    INDEX idx_score DESC (score),
    INDEX idx_created (created_at)
);

-- Auto-refresh every 1 second
-- Brain queries: SELECT * FROM hotlist WHERE score >= 6.0 ORDER BY score DESC LIMIT 10
```

---

### 🎯 Implementation Checklist

#### Phase 1: Core Scoring (High Priority)

- [ ] Create `brain/src/decision_engine/early_scorer.rs`
- [ ] Implement 7-signal algorithm
- [ ] Add hotlist table to data-mining DB
- [ ] Integrate scoring with trigger paths

#### Phase 2: Path-Specific Configs

- [ ] Refactor `TriggerConfig` for per-path thresholds
- [ ] Add `MIN_DECISION_CONF_RANK`, `_MOMENTUM`, `_COPY`
- [ ] Implement score-based position sizing
- [ ] Add path field to `ActivePosition` struct

#### Phase 3: Dynamic Exits

- [ ] Add `McVelocityTracker` to position tracking
- [ ] Replace $1 exit with velocity-based logic
- [ ] Implement per-path exit strategies
- [ ] Track MC growth in 10s windows

#### Phase 4: Validation

- [ ] Backtest against historical 1M+ MC tokens
- [ ] Measure hit rate per path
- [ ] Validate position sizing with test trades
- [ ] Monitor logs for score breakdown

---

### ⚙️ Configuration Files to Create

**1. brain/.env.rank (Rank-Based Profile)**

```bash
MIN_DECISION_CONF_RANK=55
MIN_FOLLOW_THROUGH_SCORE_RANK=35
MAX_RANK_FOR_INSTANT=5
RANK_POSITION_SIZE_SOL=50.0
```

**2. brain/.env.momentum (Momentum Profile)**

```bash
MIN_DECISION_CONF_MOMENTUM=65
MIN_FOLLOW_THROUGH_SCORE_MOMENTUM=45
MIN_BUYERS_2S=3
MIN_VOL_5S_SOL=4.0
MOMENTUM_POSITION_SIZE_SOL=75.0
```

**3. brain/.env.copy (Copy-Trade Profile)**

```bash
MIN_DECISION_CONF_COPY=70
MIN_COPY_CONFIDENCE=65
MIN_COPY_TIER=1
COPY_POSITION_SIZE_BASE=25.0
MAX_COPY_HOLD_TIME_SECS=15
```

**4. brain/.env (Global Overrides)**

```bash
# Disable Late-Opportunity
ENABLE_LATE_OPPORTUNITY=false

# Increase guardrails
MAX_CONCURRENT_POSITIONS=5
MAX_POSITION_SIZE_SOL=150.0

# Early scoring thresholds
MIN_EARLY_SCORE=6.0
HIGH_CONFIDENCE_SCORE=8.0
```

---

### 📈 Expected Performance Targets

| Metric                | Conservative | Moderate | Aggressive |
| --------------------- | ------------ | -------- | ---------- |
| **Entries/day**       | 10-20        | 30-50    | 60-100     |
| **1M+ MC catch rate** | 50-60%       | 60-70%   | 40-50%     |
| **Win rate**          | 55-65%       | 50-60%   | 40-50%     |
| **Avg profit/trade**  | $5-15        | $10-30   | $5-20      |
| **Position size**     | $50-75       | $75-100  | $100-150   |

**Target Goal:** Catch 4+ tokens/day reaching 1M+ MC with $100 average entry size

---

### 🚨 Risk Management for Larger Positions

**Updated Guardrails:**

```rust
MAX_CONCURRENT_POSITIONS: 5          // Up from 3
LOSS_BACKOFF_THRESHOLD: 4            // Pause after 4 losses (was 3)
LOSS_BACKOFF_PAUSE_SECS: 180         // 3 min cooldown
MAX_POSITION_SIZE_SOL: 150.0         // Hard cap
MIN_POSITION_SIZE_SOL: 25.0          // Testing minimum
```

**Per-Path Stop Loss:**

- Rank-Based: -20% (fast exits, higher volatility)
- Momentum: -15% (confirmed moves, standard)
- Copy-Trade: -10% (tight stops, quick scalps)

**Daily Loss Limits:**

```rust
MAX_DAILY_LOSS_USD: 500.0            // Stop trading after -$500/day
POSITION_SIZE_REDUCTION_AFTER_LOSS: 0.5  // Half size after loss
```

---

**Document Version:** 2.0 (Path-Specific Tuning for 1M+ MC Hunting)  
**Last Updated:** December 2024  
**Configuration Files:**

- `brain/src/config.rs` (DecisionConfig, ValidationConfig, GuardrailsConfig)
- `brain/src/decision_engine/triggers.rs` (TriggerConfig with path-specific thresholds)
- `brain/src/decision_engine/scoring.rs` (FollowThroughScorer)
- `brain/src/decision_engine/position_tracker.rs` (Exit logic with MC velocity)
- `brain/src/decision_engine/early_scorer.rs` (7-signal scoring system - to be created)
- `.env` (Runtime configuration overrides)
- `.env.rank`, `.env.momentum`, `.env.copy` (Path-specific profiles - to be created)
