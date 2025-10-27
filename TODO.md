# 🎯 Trading Bot System - TODO List

**Created**: October 26, 2025  
**Status**: Active Development  
**Priority Legend**: 🔴 Critical | 🟡 High | 🟢 Medium | 🔵 Low

---

## 🔴 CRITICAL - Executor Refactoring

### 📝 Clean Up Executor

- [ ] 🔴 Remove all decision-making logic from executor
- [ ] 🔴 Strip out heavy logging (keep only: entry size, TP, mint, tx speed)
- [ ] 🔴 Remove database read operations
- [ ] 🔴 Remove price fetching logic
- [ ] 🔴 Keep Telegram notifications (make async with bounded channel)
- [ ] 🔴 Add telemetry UDP sender back to Brain (port 45110)

### ⚙️ Executor .env Cleanup

- [ ] 🔴 **KEEP**: gRPC settings, wallet/keypair, Telegram, Advice Bus listener, execution limits, Postgres logging
- [ ] 🔴 **REMOVE**: ENTRY_SIZE_SOL, MAX_CONCURRENT_TRADES, STOP_LOSS_PCT, TAKE_PROFIT_USD, MIN_MARKET_CAP, HOT_PATH_SOL_THRESHOLD
- [ ] 🔴 **ADD**: BRAIN_TELEMETRY_PORT=45110
- [ ] 🔴 Configure async Telegram queue (TELEGRAM_ASYNC_QUEUE=100)

---

## 🟡 HIGH PRIORITY - Brain Development

### 🧠 Brain Core Logic

- [ ] 🟡 Implement Universal Profitability Gate
  - [ ] Calculate total fees & impact (entry + exit + slippage + tip)
  - [ ] Set dynamic TP floor: `tp_usd = max(1.00, fees_total * 2.2)`
  - [ ] Add impact cap: `impact_usd ≤ tp_usd * 0.45`
  - [ ] Compute Follow-Through (FT) score from windows (0-100)
  - [ ] Add rug/creator sanity checks
- [ ] 🟡 Implement scanning logic (every 200-500ms)
  - [ ] Query top active mints by vol_60s
  - [ ] Query top mints by buyers_60s
  - [ ] Query recent vol_5s and buyers_5s
  - [ ] Join with wallet_tracker for quality overlap
- [ ] 🟡 Implement entry rules
  - [ ] Size by signal strength (FT ≥ 80 = full, 70-79 = 0.75×, 60-69 = 0.5×)
  - [ ] Set slippage_bps from volatility/curve buffer
  - [ ] Send TradeDecision via UDP to executor

### 🔧 Brain .env Configuration

- [ ] 🟡 **ADD DATABASE**: SQLITE_PATH=/data/collector.db
- [ ] 🟡 **ADD SIZING**: BASE_ENTRY_SOL=0.5, MAX_ENTRY_SOL=2.0, ENTRY_SIZE_MODE=dynamic
- [ ] 🟡 **ADD PROFIT/RISK**: TAKE_PROFIT_USD=1.0, STOP_LOSS_PCT=15, FEE_MULTIPLIER=2.2
- [ ] 🟡 **ADD CONCURRENCY**: MAX_CONCURRENT_POSITIONS=1, MIN_TIME_BETWEEN_ENTRIES_MS=500
- [ ] 🟡 **ADD FILTERS**: MIN_MARKET_CAP_USD=3000, MAX_MARKET_CAP_USD=20000
- [ ] 🟡 **ADD WALLET INTEL**: MIN_WINRATE_FOR_COPY=70, MIN_CONFIDENCE_FOR_COPY=75
- [ ] 🟡 **ADD ADVICE BUS**: ADVICE_HOST=127.0.0.1, ADVICE_PORT=45100
- [ ] 🟡 **ADD TELEMETRY**: EXEC_TELEMETRY_PORT=45110
- [ ] 🟡 **ADD FT SCORES**: FT_SCORE_MIN_FOR_ENTRY=60, FT_SCORE_MIN_FOR_BIGGER_TP=70
- [ ] 🟡 **ADD SIZING THRESHOLDS**: SIZE_FULL_FT=80, SIZE_075_FT=70, SIZE_050_FT=60
- [ ] 🟡 **ADD IMPACT**: IMPACT_MAX_FRACTION_OF_TP=0.45

### 📊 Brain Telemetry System

- [ ] 🟡 Add timestamp_ns_created to all decision packets
- [ ] 🟡 Listen for executor telemetry on UDP:45110
- [ ] 🟡 Create /brain/logs/decisions.log
- [ ] 🟡 Create /brain/logs/perf_brain.log
- [ ] 🟡 Implement latency tracking (decision build time)

---

## 🟢 MEDIUM PRIORITY - SOL Price Feed

### 💰 Pyth Oracle Integration

- [ ] 🟢 Subscribe to Pyth SOL/USD price account (J83GarPDKyAq2Z9fV7rMZC6f1SU9JEJrR62x6M8tZ3xZ)
- [ ] 🟢 Parse price/exponent from account data
- [ ] 🟢 Broadcast SolPriceUpdate every 20s via UDP
- [ ] 🟢 Send to both Brain and Executor
- [ ] 🟢 Remove CoinGecko/Jupiter/HTTP price fetching

---

## 🔵 MEDIUM PRIORITY - Mempool Watcher (NEW TOOL)

### 🚀 Create Mempool Watcher Binary

- [ ] 🟢 Create new Rust project: `mempool_watcher.rs`
- [ ] 🟢 Subscribe to local gRPC/TPU feed for pending transactions
- [ ] 🟢 Filter Pump.fun program IDs only
- [ ] 🟢 Extract: mint, amount, wallet, side, lamports, slot, block_time
- [ ] 🟢 Compute heat index every 100-200ms
  - [ ] `heat_score = pending_sol_3s + unique_buyers_3s + Δpending_sol`
- [ ] 🟢 Send UDP messages to Brain (port 45120)
- [ ] 🟢 Send UDP messages to Executor (port 45130) for ultra-fast signals

### 🔍 Tracked Wallet Filters

- [ ] 🟢 Create tracked_wallets.toml config
- [ ] 🟢 Add Pump.fun liquidity wallet filter
- [ ] 🟢 Add team wallet filters
- [ ] 🟢 Add alpha buyer filters
- [ ] 🟢 Emit specialized events:
  - [ ] LiquidityInjection
  - [ ] CreatorBuy
  - [ ] AlphaSell

### ⚙️ Mempool .env Configuration

- [ ] 🟢 MEMPOOL_UDP_PORT_BRAIN=45120
- [ ] 🟢 MEMPOOL_UDP_PORT_EXECUTOR=45130
- [ ] 🟢 HEAT_SCORE_ENTRY_THRESHOLD=15
- [ ] 🟢 HEAT_SCORE_HOLD_THRESHOLD=10
- [ ] 🟢 HEAT_DECAY_FACTOR=0.8
- [ ] 🟢 TRACKED_WALLETS_CONFIG=tracked_wallets.toml

### 🧠 Brain Mempool Integration

- [ ] 🟢 Add UDP listener for mempool heat (port 45120)
- [ ] 🟢 Maintain in-memory HashMap<mint, HeatMetrics>
- [ ] 🟢 Use heat to extend holds if heat↑
- [ ] 🟢 Cancel/widen exit if heat↓ but still positive
- [ ] 🟢 Override TP when heat_score > threshold

### ⚡ Executor Mempool Integration

- [ ] 🟢 Add UDP listener for mempool signals (port 45130)
- [ ] 🟢 Implement priority queue (mempool > brain)
- [ ] 🟢 Add confidence comparison logic
- [ ] 🟢 Apply sanity filters before execution
- [ ] 🟢 Tag packets with source (BRAIN/MEMPOOL) and signal_strength

---

## 🔵 LOW PRIORITY - Performance & Monitoring

### 📈 Executor Telemetry

- [ ] 🔵 Add timestamp_ns_received when decision arrives
- [ ] 🔵 Add timestamp_ns_confirmed when tx confirmed
- [ ] 🔵 Create telemetry UDP packet structure
- [ ] 🔵 Send back to Brain on UDP:45110
- [ ] 🔵 Create /executor/logs/perf_exec.log
- [ ] 🔵 Create /executor/logs/telemetry.log

### 📊 Performance Database

- [ ] 🔵 Create perf_metrics.db SQLite schema
- [ ] 🔵 Add fields: decision_id, mint, brain_ms, exec_ms, total_ms, status, pnl, ts
- [ ] 🔵 Implement async JSON log appender
- [ ] 🔵 Build performance analysis queries

---

## 🎨 FUTURE ENHANCEMENTS (Optional)

### 📊 Analytics & Visualization

- [ ] 🔵 Web dashboard for profit/latency/wallet stats
- [ ] 🔵 Success rate graphs
- [ ] 🔵 Per-wallet performance breakdown

### 🧪 Backtesting v2

- [ ] 🔵 Replay collector DB with Brain logic
- [ ] 🔵 Historical strategy tuning
- [ ] 🔵 Performance comparison tools

### 🤖 AI Training (Offline)

- [ ] 🔵 Collect ≥50k labeled trades
- [ ] 🔵 Train ML model on historical data
- [ ] 🔵 Integrate AI predictions into Brain

### 🛡️ Risk Controller

- [ ] 🔵 Global risk management daemon
- [ ] 🔵 Cap daily loss limits
- [ ] 🔵 Halt after N consecutive losses
- [ ] 🔵 Read executor telemetry for monitoring

---

## 📐 System Architecture Summary

```
┌──────────────────┐
│   gRPC Feeds     │
└────────┬─────────┘
         │
         ▼
┌──────────────────┐         ┌──────────────────┐
│  Data Collector  │────────▶│   Database       │
│  (Unified)       │         │   (SQLite)       │
└──────────────────┘         └────────┬─────────┘
                                      │
         ┌────────────────────────────┘
         │
         ▼
┌──────────────────┐         ┌──────────────────┐
│ Mempool Watcher  │         │      Brain       │
│  - Heat Index    │─────UDP─▶  - Profitability │
│  - Wallet Filter │  45120  │  - FT Scoring    │
│  - Liquidity     │         │  - Decision Mgr  │
└────────┬─────────┘         └────────┬─────────┘
         │                            │
         │ UDP:45130 (hot)           │ UDP:45100 (decisions)
         │                            │
         └────────────┬───────────────┘
                      ▼
              ┌──────────────────┐
              │    Executor      │
              │  - Build & Send  │◀──UDP:45110
              │  - Telegram      │   (telemetry)
              │  - Trade Logs    │
              └────────┬─────────┘
                       │
                       ▼
                  gRPC / TPU
```

---

## 📝 Notes

### Expected Performance Improvements

- Decision latency: 50-200ms (was 500-2000ms)
- Execution latency: 10-40ms (was 500-2500ms)
- Total reaction: <250ms typical (was 1-5s)

### Communication Ports

- 45100: Brain → Executor (decisions)
- 45110: Executor → Brain (telemetry)
- 45120: Mempool → Brain (heat index)
- 45130: Mempool → Executor (hot signals)

### Configuration Philosophy

- **Executor**: Pure execution (gRPC, signing, Telegram, minimal config)
- **Brain**: All strategy/risk parameters
- **Mempool**: Heat thresholds & tracked wallets

---

**Last Updated**: October 26, 2025  
**Status**: ✅ Data Collector Complete | 🔄 Brain & Executor Refactoring In Progress
