# 🧠 Brain Service Architecture

## 📋 Purpose

The **Brain** (Decision Engine) is the intelligent decision-making layer that sits between data collectors and the execution bot. It receives live market data, wallet intelligence, and launch signals, then produces validated trade decisions sent to the executor for immediate execution.

**Key Design Goal**: Move all heavy logic (DB reads, scoring, validation) OUT of the execution bot's hot path, keeping the executor as a pure transaction builder/sender (<30ms latency).

---

## 🏗️ Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        BRAIN SERVICE                             │
│                     (Decision Engine)                            │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                    Feature Caches                          │ │
│  │  ┌──────────────────┐     ┌──────────────────┐            │ │
│  │  │  Mint Cache      │     │  Wallet Cache    │            │ │
│  │  │  (DashMap)       │     │  (DashMap)       │            │ │
│  │  │  - Token stats   │     │  - Trader stats  │            │ │
│  │  │  - Vol/buyers    │     │  - Tier (A/B/C)  │            │ │
│  │  │  - Follow score  │     │  - Confidence    │            │ │
│  │  └────────┬─────────┘     └────────┬─────────┘            │ │
│  │           │                        │                       │ │
│  │           │ 500-1000ms updates     │                       │ │
│  │           │                        │                       │ │
│  │    ┌──────▼────────┐        ┌─────▼──────┐               │ │
│  │    │  SQLite       │        │ Postgres   │               │ │
│  │    │ (LaunchTracker)│       │(WalletTracker)│            │ │
│  │    └───────────────┘        └────────────┘               │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │                 Decision Logic Core                        │ │
│  │                                                             │ │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │ │
│  │  │  Scoring     │  │ Validation   │  │ Tier System  │    │ │
│  │  │  Engine      │  │ Engine       │  │ (A/B/C)      │    │ │
│  │  │              │  │              │  │              │    │ │
│  │  │ - Follow-    │  │ - Fee floor  │  │ - Confidence │    │ │
│  │  │   through    │  │ - Impact cap │  │ - Win rate   │    │ │
│  │  │ - Quality    │  │ - Rug checks │  │ - PnL stats  │    │ │
│  │  └──────────────┘  └──────────────┘  └──────────────┘    │ │
│  │                                                             │ │
│  │  ┌────────────────────────────────────────────────────┐   │ │
│  │  │         Entry Trigger Pathways                      │   │ │
│  │  │  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐ │   │ │
│  │  │  │ Path A  │ │ Path B   │ │ Path C   │ │ Path D │ │   │ │
│  │  │  │ Rank    │ │ Momentum │ │ CopyTrade│ │ Late   │ │   │ │
│  │  │  └─────────┘ └──────────┘ └──────────┘ └────────┘ │   │ │
│  │  └────────────────────────────────────────────────────┘   │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │               Communication Layer                          │ │
│  │                                                             │ │
│  │  ┌─────────────────┐              ┌───────────────────┐   │ │
│  │  │ Advice Bus RX   │              │ Decision Bus TX   │   │ │
│  │  │  UDP :45100     │              │  UDP :45110       │   │ │
│  │  │                 │              │                   │   │ │
│  │  │ Receives from:  │              │ Sends to:         │   │ │
│  │  │ - WalletTracker │              │ - Execution Bot   │   │ │
│  │  │ - LaunchTracker │              │                   │   │ │
│  │  └─────────────────┘              └───────────────────┘   │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │            Guardrails & Monitoring                         │ │
│  │  - Anti-churn backoff                                      │ │
│  │  - Rate limiting                                           │ │
│  │  - Wallet cooling periods                                  │ │
│  │  - Comprehensive logging (CSV/DB)                          │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘

    ▲                                              │
    │                                              │
    │ Advisory Messages                            │ Trade Decisions
    │ (CopyTrade,                                  │ (52-byte packets)
    │  LateOpportunity,                            │
    │  ExtendHold, etc.)                           ▼
    │
┌───┴──────────────┐                    ┌──────────────────┐
│  WalletTracker   │                    │  Execution Bot   │
│  LaunchTracker   │                    │  (Pure Executor) │
└──────────────────┘                    └──────────────────┘
```

---

## 📁 Module Structure

```
brain/
├── Cargo.toml
├── .env                          # Configuration
├── decision.md                   # Requirements reference
├── ARCHITECTURE.md               # This file
├── README.md                     # User-facing docs
└── src/
    ├── main.rs                   # Entry point, initialization, main loop
    ├── config.rs                 # Load .env config
    ├── types.rs                  # Shared types (Pubkey, Tier enum, etc.)
    │
    ├── feature_cache/
    │   ├── mod.rs                # Cache management
    │   ├── mint_cache.rs         # Mint features (token stats)
    │   └── wallet_cache.rs       # Wallet features (trader stats)
    │
    ├── decision_engine/
    │   ├── mod.rs                # Core decision orchestration
    │   ├── scoring.rs            # Follow-through scoring algorithm
    │   ├── validation.rs         # Pre-trade validation (fee floor, impact)
    │   ├── tier_system.rs        # Wallet tier classification (A/B/C)
    │   └── entry_triggers/
    │       ├── mod.rs
    │       ├── rank.rs           # Path A: Rank-based
    │       ├── momentum.rs       # Path B: Momentum-based
    │       ├── copy_trade.rs     # Path C: Copy-trade
    │       └── late_opportunity.rs # Path D: Late opportunity
    │
    ├── udp_bus/
    │   ├── mod.rs
    │   ├── advice_receiver.rs    # Listen on :45100 (from collectors)
    │   ├── decision_sender.rs    # Send on :45110 (to executor)
    │   └── messages.rs           # TradeDecision, HeatPulse structs
    │
    ├── guardrails/
    │   ├── mod.rs
    │   ├── backoff.rs            # Loss-based backoff logic
    │   ├── rate_limiter.rs       # Entry rate limiting
    │   └── wallet_cooling.rs     # Prevent copy-trade spam
    │
    └── logging/
        ├── mod.rs
        └── decision_logger.rs    # CSV/DB logging for analysis
```

---

## 🔄 Data Flow

### 1. Initialization
```
1. Load .env config (thresholds, DB connections, ports)
2. Connect to Postgres (WalletTracker DB)
3. Connect to SQLite (LaunchTracker DB)
4. Initialize DashMap caches (mint, wallet)
5. Bind UDP sockets (:45100 RX, :45110 TX)
6. Spawn cache updater tasks (500-1000ms intervals)
7. Enter main decision loop
```

### 2. Cache Updates (Background Tasks)
```
Mint Cache Updater (every 500ms):
├─ SELECT mint, age, price, vol_60s, buyers_60s, buys_sells_ratio 
├─ FROM tokens JOIN windows WHERE ...
├─ Compute follow_through_score for each
└─ DashMap.insert(mint, MintFeatures)

Wallet Cache Updater (every 1000ms):
├─ SELECT wallet, win_rate_7d, pnl_7d, trade_count, avg_size
├─ FROM wallet_stats WHERE ...
├─ Compute tier (A/B/C) and confidence
└─ DashMap.insert(wallet, WalletFeatures)
```

### 3. Main Decision Loop
```
loop {
    // Non-blocking check for incoming advisory messages
    if let Ok(advisory) = advice_rx.try_recv() {
        match advisory {
            CopyTrade { mint, wallet, confidence } => {
                // Look up wallet tier from cache
                if let Some(wallet_features) = wallet_cache.get(&wallet) {
                    if wallet_features.tier >= Tier::C && confidence >= 75 {
                        // Look up mint features
                        if let Some(mint_features) = mint_cache.get(&mint) {
                            // Validate trade
                            if let Ok(validated) = validate_trade(mint_features, wallet_features) {
                                // Create TradeDecision packet
                                let decision = TradeDecision {
                                    msg_type: 1,
                                    mint: mint.to_bytes(),
                                    side: 0, // BUY
                                    size_lamports: compute_size(wallet_features.confidence),
                                    slippage_bps: 150,
                                    confidence: wallet_features.confidence,
                                    _padding: [0; 8],
                                };
                                
                                // Send to executor
                                decision_tx.send(decision)?;
                                
                                // Log decision
                                log_decision(&decision, "copy_trade", mint_features, wallet_features);
                            }
                        }
                    }
                }
            },
            
            LateOpportunity { mint, horizon_sec, score } => {
                // Similar logic for late entries...
            },
            
            ExtendHold { mint, extra_secs, confidence } => {
                // Forward to executor (passthrough)
                executor_advisory_tx.send(advisory)?;
            },
            
            WidenExit { mint, sell_slip_bps, ttl_ms, confidence } => {
                // Forward to executor (passthrough)
                executor_advisory_tx.send(advisory)?;
            },
            
            // ... other advisory types
        }
    }
    
    // Check for rank-based opportunities (Path A)
    // Check for momentum opportunities (Path B)
    // Apply guardrails (backoff, rate limits, cooling)
    
    tokio::time::sleep(Duration::from_micros(100)).await;
}
```

### 4. Trade Decision Creation
```
validate_trade(mint_features, wallet_features):
├─ Compute fees_est = entry_fee + exit_fee + slippage
├─ Enforce min_tp = max(1.00, fees_est * 2.2)
├─ Check impact_usd <= min_tp * 0.45
├─ Verify follow_through_score >= threshold
├─ Check rug/creator flags
└─ Return Ok(ValidatedTrade) or Err(reason)

compute_size(confidence):
├─ If confidence >= 90 (Tier A): return FULL_SIZE
├─ If confidence >= 85 (Tier B): return FULL_SIZE * 0.75
├─ If confidence >= 75 (Tier C): return FULL_SIZE * 0.50
└─ Else: return MIN_SIZE
```

---

## 📡 Communication Protocols

### Advice Bus (Port 45100) - RECEIVE
Receives advisory messages from WalletTracker and LaunchTracker:
- **ExtendHold**: Passthrough to executor
- **WidenExit**: Passthrough to executor  
- **LateOpportunity**: Process and create decision
- **CopyTrade**: Process and create decision
- **SolPriceUpdate**: Update internal SOL price cache
- **EmergencyExit**: Forward to executor with urgency flag

### Decision Bus (Port 45110) - SEND
Sends TradeDecision packets to Execution Bot:
```rust
#[repr(C)]
struct TradeDecision {
    msg_type: u8,           // 1 = TRADE_DECISION
    mint: [u8; 32],        // Token address
    side: u8,              // 0=BUY, 1=SELL
    size_lamports: u64,    // Trade size in lamports
    slippage_bps: u16,     // Slippage in basis points
    confidence: u8,        // Confidence 0-100
    _padding: [u8; 8],     // Pad to 52 bytes
}
```

---

## ⚙️ Configuration (.env)

```env
# Database connections
POSTGRES_URL=postgresql://user:pass@localhost/wallet_tracker
SQLITE_PATH=/path/to/launch_tracker/collector.db

# UDP ports
ADVICE_BUS_PORT=45100
DECISION_BUS_PORT=45110

# Thresholds
MIN_DECISION_CONF=70
MIN_COPYTRADE_CONFIDENCE=75
MIN_FOLLOW_THROUGH_SCORE=60

# Profit optimization
FEE_MULTIPLIER=2.2
IMPACT_CAP_MULTIPLIER=0.45

# Guardrails
MAX_CONCURRENT_POSITIONS=3
RATE_LIMIT_MS=30000
BACKOFF_AFTER_LOSSES=3
BACKOFF_DURATION_SECS=120
WALLET_COOLING_SECS=90

# Sizing
FULL_SIZE_LAMPORTS=1000000000  # 1 SOL
MIN_SIZE_LAMPORTS=250000000    # 0.25 SOL

# Logging
DECISION_LOG_PATH=./logs/decisions.csv
LOG_LEVEL=info
```

---

## 🏆 Performance Targets

| Metric | Target | Why |
|--------|--------|-----|
| Cache read latency | < 50µs | Lock-free DashMap access |
| Decision latency | < 5ms | From advisory to TradeDecision |
| UDP send latency | < 100µs | Localhost, no serialization overhead |
| Cache update frequency | 500-1000ms | Balance freshness vs DB load |
| Memory usage | < 500MB | Reasonable for 24/7 service |

---

## 🔐 Safety Features

### Anti-Churn Guardrails
1. **Backoff**: After 3 losses in 3 minutes, pause advisor entries for 2 minutes
2. **Rate Limiting**: Max 1 advisor entry per 30 seconds
3. **Wallet Cooling**: No copy-trade same wallet >1x per 90s (unless Tier A profitable)
4. **Concurrent Limit**: Max 2-3 advisor positions at once

### Validation Layers
1. **Fee Floor**: Never enter if projected profit < 2.2× estimated fees
2. **Impact Cap**: Skip if price impact > 45% of target profit
3. **Follow-Through Check**: Require minimum buyer/volume momentum
4. **Rug Checks**: Filter known creator addresses and suspicious patterns

---

## �� Monitoring & Logging

### Decision Log (CSV)
```
decision_id,timestamp,mint,trigger,predicted_fees,predicted_impact,tp_usd,follow_through_score,size_lamports,confidence,expected_ev
d_001,2025-10-24T19:15:23,8xK2...,copy_trade,0.35,0.12,1.50,72,500000000,88,0.95
d_002,2025-10-24T19:15:45,3Yad...,rank,0.28,0.08,1.20,85,1000000000,95,1.15
...
```

### Metrics to Track
- Decisions/minute by trigger type
- Average confidence per trigger
- Cache hit rate
- DB query latency
- UDP message loss rate
- Guardrail activation frequency

---

## 🚀 Deployment

```bash
# Build release binary
cargo build --release

# Run Brain service
./target/release/decision_engine

# With custom config
POSTGRES_URL=... SQLITE_PATH=... ./target/release/decision_engine

# Background daemon
nohup ./target/release/decision_engine > brain.log 2>&1 &
```

---

## 🔧 Future Enhancements

1. **Heat Sentinel Integration**: Dedicated mempool monitoring service
2. **Multi-Executor Support**: Send decisions to multiple executors
3. **Strategy Loading**: Load backtested strategies from DB
4. **ML Scoring**: Replace hardcoded scoring with trained models
5. **WebSocket API**: Real-time decision monitoring dashboard
6. **A/B Testing**: Run multiple decision algorithms in parallel

---

**Last Updated**: October 24, 2025
**Version**: 0.1.0
**Status**: Initial Design
