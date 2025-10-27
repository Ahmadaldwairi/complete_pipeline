# Brain Service Implementation Status

## ✅ COMPLETED (Steps 1-23)

### Core Infrastructure
- [x] **Project structure** - Cargo workspace, module layout
- [x] **UDP Bus layer** (1,080 lines)
  - `messages.rs` - TradeDecision (52 bytes), HeatPulse (64 bytes), 5 advice types
  - `sender.rs` - Decision Bus (port 45110) with retry logic
  - `receiver.rs` - Advice Bus (port 45100) with statistics
- [x] **Feature caches** (lock-free DashMap)
  - `mint_cache.rs` - Token launch data, volume, buyers
  - `wallet_cache.rs` - Wallet performance, tier classification
- [x] **Decision engine core** (2,196 lines)
  - `scoring.rs` - Follow-through scoring (0-100)
  - `validation.rs` - 9 pre-trade checks, fee/impact validation
  - `triggers.rs` - 4 entry pathways (rank/momentum/copy/late)
  - `guardrails.rs` - Anti-churn protection, loss backoff
  - `logging.rs` - CSV decision logging (17 fields)
- [x] **Configuration system** (394 lines)
  - `.env` file with 30+ parameters
  - Type-safe loading and validation
  - 8 config tests
- [x] **Documentation** (130KB total)
  - `README.md` (21KB) - Complete user guide
  - `CONFIG.md` (9KB) - Configuration reference
  - `ARCHITECTURE.md` (19KB) - System design
- [x] **Testing** - 77 tests passing
- [x] **Build** - Release binary (2.4MB)

### Decision Logic Implemented
- [x] **Entry triggers**
  - Path A: Rank ≤ 2 + score ≥ 60 ✅
  - Path B: Momentum (buyers ≥ 5, vol ≥ 8 SOL) ✅
  - Path C: Copy-trade (tier ≥ C, size ≥ 0.25 SOL) ✅
  - Path D: Late opportunity (age > 20min, sustained activity) ✅
- [x] **Pre-trade validation** (9 checks)
  - Launch age window ✅
  - Liquidity minimum ✅
  - Fee threshold (2.2x multiplier) ✅
  - Impact cap (45% of TP) ✅
  - Confidence minimum ✅
  - Follow-through score ✅
  - Position size bounds ✅
  - Wallet tier check ✅
  - Expected value positive ✅
- [x] **Guardrails**
  - Position limits (max 3, max 2 advisor) ✅
  - Rate limiting (100ms general, 30s advisor) ✅
  - Loss backoff (3 losses → 2min pause) ✅
  - Wallet cooling (90s between copies) ✅
- [x] **Wallet tier system** (confidence levels)
  - Tier A: win ≥ 60%, PnL ≥ 100 SOL ✅
  - Tier B: win ≥ 55%, PnL ≥ 40 SOL ✅
  - Tier C: win ≥ 50%, PnL ≥ 15 SOL ✅
- [x] **Decision logging**
  - 17-field CSV records ✅
  - Trigger type, validation metrics, EV calculation ✅

## 🔨 TO IMPLEMENT (Integration & Runtime)

### 1. Main Service Loop (Critical)
**Status:** Stub exists in main.rs, needs full implementation

**Required:**
- [ ] Initialize all components from config
- [ ] Start feature cache updaters (Postgres + SQLite)
- [ ] Spawn Advice Bus receiver task
- [ ] Create Decision Bus sender
- [ ] Main decision loop:
  ```rust
  loop {
      // 1. Receive advice message
      let advice = receiver.recv().await?;
      
      // 2. Detect trigger
      let trigger = trigger_engine.detect(&advice)?;
      
      // 3. Lookup features
      let mint_features = mint_cache.get(&trigger.mint)?;
      let wallet_features = wallet_cache.get(&trigger.wallet)?;
      
      // 4. Score opportunity
      let score = scorer.score(&mint_features)?;
      
      // 5. Validate trade
      let validated = validator.validate(&trigger, &mint_features, score)?;
      
      // 6. Check guardrails
      if !guardrails.check(&validated)? {
          continue;
      }
      
      // 7. Build decision
      let decision = TradeDecision::from_validated(&validated);
      
      // 8. Log decision
      logger.log_decision(&decision)?;
      
      // 9. Send to executor
      sender.send_decision(&decision).await?;
      
      // 10. Update guardrails
      guardrails.record_decision(&decision);
  }
  ```

**Estimated:** 200-300 lines in main.rs

### 2. SOL Price Updates
**Status:** Not implemented

**Required:**
- [ ] Subscribe to SOL/USD price oracle
- [ ] Receive `SolPriceUpdate` messages via Advice Bus
- [ ] Store latest price in atomic variable
- [ ] Use in validation calculations (USD conversions)

**Options:**
1. **WalletTracker sends updates** (already planned in decision.md)
   - WalletTracker subscribes to price feed
   - Sends SolPriceUpdate every 20s via UDP port 45100
   - Brain receives and updates cached price
   
2. **Brain subscribes directly**
   - Subscribe to Pyth/Jupiter price feed
   - Update internal cache every 10-20s
   - More autonomous but adds dependency

**Recommendation:** Option 1 (WalletTracker sends)
- Already specified in architecture
- Keeps Brain focused on decisions
- WalletTracker better positioned for price monitoring

**Implementation:**
```rust
// In receiver.rs - already has infrastructure
match advice_msg {
    AdviceMessage::SolPriceUpdate(price) => {
        SOL_PRICE.store(price.price_usd_cents, Ordering::Relaxed);
        log::debug!("SOL price updated: ${:.2}", price.price_usd_cents as f64 / 100.0);
    }
    // ... other message types
}
```

**Estimated:** 50-100 lines

### 3. Mempool Heat Monitoring
**Status:** Not implemented (critical for profit optimization)

**Current situation:**
- Execution bot currently watches mempool (decision.md line 558)
- Should be moved to Brain or separate Heat Sentinel service
- Need to track pending Pump.fun buys in real-time

**Required:**
- [ ] Subscribe to Yellowstone gRPC for pending transactions
- [ ] Filter for Pump.fun buy instructions
- [ ] Calculate heat metrics:
  - `pending_buys` - count of pending buy txs
  - `pending_sol` - total SOL in pending buys
  - `uniq_senders` - unique wallet count
  - `heat_score` - 0-100 composite score
- [ ] Send `HeatPulse` messages every 100-200ms
- [ ] Override TP/exit logic when heat is high

**Two approaches:**

**A. Heat Sentinel Service (Separate)**
```
pros: Clean separation, won't slow Brain
cons: Another service to manage
```

**B. Integrate into Brain**
```
pros: All intelligence in one place
cons: Adds gRPC dependency to Brain
```

**Recommendation:** Option A (Separate Heat Sentinel)
- Brain stays focused on decision logic
- Heat Sentinel similar to LaunchTracker (watches chain events)
- Sends HeatPulse messages to Brain via UDP

**HeatPulse structure (already defined):**
```rust
pub struct HeatPulse {
    pub mint: [u8; 32],           // Token being monitored
    pub pending_buys: u16,         // Pending buy count
    pub pending_sol: u32,          // Total SOL in pending (lamports)
    pub uniq_senders: u16,         // Unique buyer count
    pub heat_score: u8,            // 0-100 composite
    pub timestamp: u64,            // Unix timestamp
}
```

**Brain integration:**
```rust
// In decision loop
if let Some(heat) = latest_heat_pulse.get(&decision.mint) {
    if heat.heat_score >= 80 {
        // Override $1 TP, extend hold time
        decision.override_tp_for_heat(heat);
        decision.extend_hold_window();
    }
}
```

**Estimated:** 300-500 lines for Heat Sentinel service

### 4. Database Connection Setup
**Status:** Config exists, connections not established

**Required:**
- [ ] Create Postgres connection pool
  ```rust
  let (client, connection) = tokio_postgres::connect(
      &config.database.postgres_connection_string(),
      NoTls
  ).await?;
  ```
- [ ] Create SQLite connection
  ```rust
  let sqlite_conn = rusqlite::Connection::open(
      &config.database.sqlite_path
  )?;
  ```
- [ ] Test connectivity on startup
- [ ] Pass connections to feature caches

**Estimated:** 100-150 lines

### 5. Feature Cache Updaters
**Status:** Structures exist, updater loops not started

**Required:**
- [ ] Start mint cache updater task
  ```rust
  let mint_cache_clone = mint_cache.clone();
  tokio::spawn(async move {
      mint_cache_clone.start_updater(
          config.cache.cache_refresh_interval_secs * 1000
      ).await;
  });
  ```
- [ ] Start wallet cache updater task
- [ ] Implement SQL queries in cache modules:
  - `query_mint_features()` - from SQLite
  - `query_wallet_features()` - from Postgres
- [ ] Verify <50µs read performance

**SQL Queries needed:**
```sql
-- Mint cache (SQLite)
SELECT mint, buyers_2s, vol_2s_sol, vol_60s_sol, liquidity_usd, 
       launch_timestamp, buys_count, sells_count
FROM tokens 
WHERE launch_timestamp > ? -- last 4 hours

-- Wallet cache (Postgres)
SELECT address, win_rate_7d, realized_pnl_7d, trade_count_7d,
       avg_trade_size_sol, total_volume_7d
FROM wallet_stats
WHERE last_update > NOW() - INTERVAL '7 days'
```

**Estimated:** 200-300 lines

### 6. Executor Integration
**Status:** Not started (depends on executor bot refactoring)

**Required on Brain side:**
- [x] Decision Bus sender (already implemented)
- [ ] Test UDP communication with executor
- [ ] Verify TradeDecision packet parsing
- [ ] Monitor decision latency (target <5ms)

**Required on Executor side:**
- [ ] Remove all decision logic from executor
- [ ] Listen on UDP port 45110
- [ ] Parse TradeDecision packets
- [ ] Execute trades immediately
- [ ] Send position updates back to Brain (optional)

**Estimated:** Already complete on Brain side, executor needs refactoring

### 7. Testing & Validation
**Status:** Unit tests done (77/77), integration tests needed

**Required:**
- [ ] Integration test with mock databases
- [ ] Load test (100+ decisions/sec)
- [ ] Latency measurement (<5ms target)
- [ ] Memory leak test (24h+ runtime)
- [ ] UDP message delivery verification
- [ ] Cache hit rate monitoring (>90% target)

**Estimated:** 200-300 lines of integration tests

### 8. Monitoring & Metrics
**Status:** Basic logging exists, metrics not implemented

**Optional but recommended:**
- [ ] Prometheus metrics exporter
- [ ] Decision rate gauge
- [ ] Validation rejection counters
- [ ] Cache hit rate metrics
- [ ] Guardrail trigger counters
- [ ] UDP message latency histogram

**Estimated:** 150-200 lines

## 📊 Completion Status

### Code Implementation
- **Completed:** ~4,500 lines (85%)
- **Remaining:** ~1,200 lines (15%)

### Functionality
- **Completed:** Core logic, validation, guardrails, logging (90%)
- **Remaining:** Runtime integration, mempool heat, live testing (10%)

### Critical Path Items
1. **Main service loop** (200 lines) - CRITICAL
2. **Database connections** (150 lines) - CRITICAL  
3. **Feature cache updaters** (300 lines) - CRITICAL
4. **SOL price updates** (100 lines) - HIGH
5. **Mempool heat sentinel** (500 lines) - HIGH
6. **Executor integration testing** - MEDIUM
7. **Monitoring/metrics** (200 lines) - LOW

**Estimated time to production:**
- Core integration: 4-6 hours
- Heat Sentinel: 4-6 hours
- Testing & validation: 4-8 hours
- **Total: 12-20 hours**

## 🎯 Recommended Next Steps

### Phase 1: Get Brain Running (4-6 hours)
1. Implement main service loop
2. Connect to databases
3. Start feature cache updaters
4. Test SOL price updates
5. Verify decision flow end-to-end

### Phase 2: Add Heat Monitoring (4-6 hours)
1. Create Heat Sentinel service
2. Subscribe to gRPC pending transactions
3. Calculate heat metrics
4. Send HeatPulse messages
5. Integrate heat overrides in Brain

### Phase 3: Production Testing (4-8 hours)
1. Integration test with executor
2. Load test (100+ decisions/sec)
3. 24-hour stability test
4. Tune thresholds based on results
5. Monitor $1 profit target achievement

## 📝 Architecture Notes

**Current system:**
```
RankBot ──┐
          ├──> [UDP 45100] ──> Brain ──> [UDP 45110] ──> Executor
AdvisorBot┘                      ↑
                                 │
                    ┌────────────┴────────────┐
                    │                         │
              [Postgres]                  [SQLite]
           (WalletTracker)            (LaunchTracker)
```

**With Heat Sentinel:**
```
RankBot ──────┐
              ├──> [UDP 45100] ──> Brain ──> [UDP 45110] ──> Executor
AdvisorBot────┤                      ↑
              │                      │
HeatSentinel──┘         ┌───────────┴────────────┐
(gRPC mempool)          │                        │
                   [Postgres]                [SQLite]
                (WalletTracker)          (LaunchTracker)
```

**Key design decisions:**
1. Brain is **stateless** (only in-memory caches)
2. All persistence in Postgres/SQLite
3. UDP for <1ms inter-process communication
4. Lock-free DashMap for <50µs cache reads
5. Separate concerns: Brain decides, Executor executes, Sentinel monitors

