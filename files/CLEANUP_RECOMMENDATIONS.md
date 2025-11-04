# Code Cleanup Recommendations

**Generated**: $(date)
**Purpose**: Identify unused code, variables, and files that can be safely removed

---

## Summary

This report consolidates findings from all three service test scripts:
- test_brain.sh → brain_unused_code.log
- test_data_mining.sh → data_mining_unused_code.log
- test_execution.sh → execution_unused_code.log

---

## Brain Service

### Unused Code Warnings

```
warning: unused import: `Context`
 --> src/udp_bus/messages.rs:6:22
  |
6 | use anyhow::{Result, Context};
  |                      ^^^^^^^
  |
  = note: requested on the command line with `-W unused-imports`

warning: unused import: `AdviceMessageType`
  --> src/udp_bus/receiver.rs:12:47
   |
12 | use crate::udp_bus::messages::{AdviceMessage, AdviceMessageType};
   |                                               ^^^^^^^^^^^^^^^^^

warning: unused imports: `HeatPulse` and `MempoolHeatAdvice`
  --> src/udp_bus/mod.rs:12:20
   |
12 |     TradeDecision, HeatPulse, AdviceMessage, 
   |                    ^^^^^^^^^
...
15 |     MempoolHeatAdvice,
   |     ^^^^^^^^^^^^^^^^^

warning: unused import: `DecisionBatchSender`
  --> src/udp_bus/mod.rs:18:37
   |
18 | pub use sender::{DecisionBusSender, DecisionBatchSender};
   |                                     ^^^^^^^^^^^^^^^^^^^

warning: unused import: `ReceiverStats`
  --> src/udp_bus/mod.rs:19:39
   |
19 | pub use receiver::{AdviceBusReceiver, ReceiverStats};
   |                                       ^^^^^^^^^^^^^

warning: this `else { if .. }` block can be collapsed
   --> src/feature_cache/mint_cache.rs:290:20
    |
290 |               } else {
    |  ____________________^
291 | |                 if buys_60s > 0 { 10.0 } else { 1.0 }
292 | |             };
    | |_____________^ help: collapse nested if block: `if buys_60s > 0 { 10.0 } else { 1.0 }`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_else_if
    = note: `#[warn(clippy::collapsible_else_if)]` on by default

warning: unused import: `super::*`
   --> src/feature_cache/wallet_cache.rs:366:9
    |
366 |     use super::*;
    |         ^^^^^^^^

warning: unused import: `LastTrade`
 --> src/feature_cache/mod.rs:5:65
  |
5 | pub use wallet_cache::{WalletCache, WalletFeatures, WalletTier, LastTrade};
  |                                                                 ^^^^^^^^^

warning: unused import: `Context`
  --> src/decision_engine/triggers.rs:10:22
   |
10 | use anyhow::{Result, Context, bail};
   |                      ^^^^^^^

warning: unused imports: `info` and `warn`
  --> src/decision_engine/triggers.rs:11:11
   |
11 | use log::{info, debug, warn};
   |           ^^^^         ^^^^

warning: unused import: `ValidationError`
  --> src/decision_engine/triggers.rs:13:62
   |
13 | use crate::decision_engine::{TradeValidator, ValidatedTrade, ValidationError};
   |                                                              ^^^^^^^^^^^^^^^

warning: unused import: `Context`
  --> src/decision_engine/guardrails.rs:11:22
   |
11 | use anyhow::{Result, Context};
   |                      ^^^^^^^

warning: unused imports: `error` and `warn`
  --> src/decision_engine/logging.rs:11:17
   |
11 | use log::{info, warn, error};
   |                 ^^^^  ^^^^^

warning: unused import: `crate::udp_bus::TradeDecision`
 --> src/decision_engine/position_tracker.rs:9:5
  |
9 | use crate::udp_bus::TradeDecision;
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `ScoreComponents`
  --> src/decision_engine/mod.rs:10:40
   |
10 | pub use scoring::{FollowThroughScorer, ScoreComponents};
   |                                        ^^^^^^^^^^^^^^^

warning: unused imports: `FeeEstimate` and `ValidationConfig`
  --> src/decision_engine/mod.rs:11:54
   |
11 | pub use validation::{TradeValidator, ValidatedTrade, ValidationConfig, ValidationError, FeeEstimate};
   |                                                      ^^^^^^^^^^^^^^^^                   ^^^^^^^^^^^

warning: unused imports: `EntryTrigger` and `TriggerConfig`
  --> src/decision_engine/mod.rs:12:35
   |
12 | pub use triggers::{TriggerEngine, TriggerConfig, EntryTrigger};
   |                                   ^^^^^^^^^^^^^  ^^^^^^^^^^^^

warning: unused imports: `GuardrailConfig`, `GuardrailStats`, and `TradeOutcome`
  --> src/decision_engine/mod.rs:13:34
   |
13 | pub use guardrails::{Guardrails, GuardrailConfig, GuardrailStats, TradeOutcome};
   |                                  ^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^  ^^^^^^^^^^^^

warning: unused import: `DecisionLogBuilder`
  --> src/decision_engine/mod.rs:14:53
   |
14 | pub use logging::{DecisionLogger, DecisionLogEntry, DecisionLogBuilder, TriggerType};
   |                                                     ^^^^^^^^^^^^^^^^^^

warning: unused import: `Counter`
  --> src/metrics.rs:19:5
   |
19 |     Counter, Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry,
   |     ^^^^^^^

warning: empty line after doc comment
 --> src/mint_reservation.rs:7:1
  |
7 | / /// This provides an additional safety layer on top of the state machine.
8 | |
  | |_^
9 |   use std::collections::HashMap;
  |   - the comment documents this `use` import
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#empty_line_after_doc_comments
  = note: `#[warn(clippy::empty_line_after_doc_comments)]` on by default
  = help: if the empty line is unintentional, remove it
help: if the comment should document the parent module use an inner doc comment
  |
1 ~ //! Mint Reservation System
2 ~ //! 
3 ~ //! Prevents duplicate BUY decisions by maintaining time-based leases on mints.
4 ~ //! When Brain sends a BUY decision, it reserves the mint for a TTL period (default 30s).
5 ~ //! Any subsequent BUY attempts during the lease period are rejected.
6 ~ //! 
7 ~ //! This provides an additional safety layer on top of the state machine.
  |

warning: unused import: `std::collections::HashSet`
  --> src/main.rs:27:5
   |
27 | use std::collections::HashSet;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `ExecutionConfirmation`
  --> src/main.rs:39:5
   |
39 |     ExecutionConfirmation,
   |     ^^^^^^^^^^^^^^^^^^^^^

warning: unused variable: `scorer`
    --> src/main.rs:1634:5
     |
1634 |     scorer: &FollowThroughScorer,
     |     ^^^^^^ help: if this is intentional, prefix it with an underscore: `_scorer`
     |
     = note: requested on the command line with `-W unused-variables`

warning: unused variable: `validator`
    --> src/main.rs:1635:5
     |
1635 |     validator: &TradeValidator,
     |     ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_validator`

warning: unused variable: `logger`
    --> src/main.rs:1637:5
     |
1637 |     logger: &DecisionLogger,
     |     ^^^^^^ help: if this is intentional, prefix it with an underscore: `_logger`

warning: unused variable: `mint_features`
    --> src/main.rs:1655:9
     |
1655 |     let mint_features = match mint_cache.get(&mint) {
     |         ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_mint_features`

warning: unused variable: `scorer`
    --> src/main.rs:1784:5
     |
1784 |     scorer: &FollowThroughScorer,
     |     ^^^^^^ help: if this is intentional, prefix it with an underscore: `_scorer`

warning: unused variable: `validator`
    --> src/main.rs:1785:5
     |
1785 |     validator: &TradeValidator,
     |     ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_validator`

warning: unused variable: `logger`
    --> src/main.rs:1787:5
     |
1787 |     logger: &DecisionLogger,
     |     ^^^^^^ help: if this is intentional, prefix it with an underscore: `_logger`

warning: unused variable: `mint_features`
    --> src/main.rs:1805:9
     |
1805 |     let mint_features = match mint_cache.get(&mint) {
     |         ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_mint_features`

warning: unused variable: `avg_hold_time_sec`
    --> src/main.rs:2403:13
     |
2403 |         let avg_hold_time_sec: i64 = row.get(2);
     |             ^^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_avg_hold_time_sec`

warning: unused variable: `follow_through_rate`
    --> src/main.rs:2406:13
     |
2406 |         let follow_through_rate: f64 = row.get(5);
     |             ^^^^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_follow_through_rate`

warning: unused variable: `avg_entry_speed_ms`
    --> src/main.rs:2407:13
     |
2407 |         let avg_entry_speed_ms: i64 = row.get(6);
     |             ^^^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_avg_entry_speed_ms`

warning: unused variable: `losses`
   --> src/feature_cache/wallet_cache.rs:317:36
    |
317 |             let (wallet_str, wins, losses, pnl, trade_count, win_rate, last_seen) = row_result?;
    |                                    ^^^^^^ help: if this is intentional, prefix it with an underscore: `_losses`

warning: unused variable: `last_seen`
   --> src/feature_cache/wallet_cache.rs:317:72
    |
317 |             let (wallet_str, wins, losses, pnl, trade_count, win_rate, last_seen) = row_result?;
    |                                                                        ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_last_seen`

warning: unused variable: `is_tracked`
   --> src/feature_cache/wallet_cache.rs:299:17
    |
299 |             let is_tracked: i32 = row.get(5)?;
    |                 ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_is_tracked`

warning: unused variable: `trigger`
   --> src/decision_engine/triggers.rs:194:9
    |
194 |         trigger: EntryTrigger,
    |         ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_trigger`

warning: unused variable: `pg_client_opt`
   --> src/main.rs:317:9
    |
317 |     let pg_client_opt = match tokio_postgres::connect(&pg_config, tokio_postgres::NoTls).await {
    |         ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_pg_client_opt`

warning: unused variable: `trigger_engine`
   --> src/main.rs:378:9
    |
378 |     let trigger_engine = TriggerEngine::new();
    |         ^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_trigger_engine`

warning: unused variable: `config_confirm`
   --> src/main.rs:628:9
    |
628 |     let config_confirm = config.clone();
    |         ^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_config_confirm`

warning: unused variable: `features`
   --> src/main.rs:685:49
    |
685 | ...                   if let Some(features) = mint_cache_confirm.get(&mint_pubkey) {
    |                                   ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_features`

warning: function `update_wallet_cache` is never used
    --> src/main.rs:2372:10
     |
2372 | async fn update_wallet_cache(
     |          ^^^^^^^^^^^^^^^^^^^
     |
     = note: requested on the command line with `-W dead-code`

warning: fields `validation`, `cache`, and `performance` are never read
  --> src/config.rs:16:9
   |
14 | pub struct Config {
   |            ------ fields in this struct
15 |     pub decision: DecisionConfig,
16 |     pub validation: ValidationConfig,
   |         ^^^^^^^^^^
...
21 |     pub cache: CacheConfig,
   |         ^^^^^
22 |     pub performance: PerformanceConfig,
   |         ^^^^^^^^^^^
   |
   = note: `Config` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `reserve_sell_ttl_sec` is never read
  --> src/config.rs:38:9
   |
28 | pub struct ConfirmationConfig {
   |            ------------------ field in this struct
...
38 |     pub reserve_sell_ttl_sec: u64,
   |         ^^^^^^^^^^^^^^^^^^^^
   |
   = note: `ConfirmationConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `min_follow_through_score` is never read
  --> src/config.rs:57:9
   |
51 | pub struct DecisionConfig {
   |            -------------- field in this struct
...
57 |     pub min_follow_through_score: u8,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `DecisionConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `fee_multiplier`, `impact_cap_multiplier`, `min_liquidity_usd`, and `max_slippage` are never read
  --> src/config.rs:64:9
   |
62 | pub struct ValidationConfig {
   |            ---------------- fields in this struct
63 |     /// Multiplier for fee estimation (e.g., 2.2)
64 |     pub fee_multiplier: f64,
   |         ^^^^^^^^^^^^^^
65 |     /// Maximum impact as fraction of TP target (e.g., 0.45 = 45%)
66 |     pub impact_cap_multiplier: f64,
   |         ^^^^^^^^^^^^^^^^^^^^^
67 |     /// Minimum liquidity required in USD
68 |     pub min_liquidity_usd: f64,
   |         ^^^^^^^^^^^^^^^^^
69 |     /// Maximum slippage tolerance as fraction (e.g., 0.15 = 15%)
70 |     pub max_slippage: f64,
   |         ^^^^^^^^^^^^
   |
   = note: `ValidationConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `udp_bind_address`, `udp_recv_buffer_size`, and `udp_send_buffer_size` are never read
   --> src/config.rs:133:9
    |
127 | pub struct NetworkConfig {
    |            ------------- fields in this struct
...
133 |     pub udp_bind_address: IpAddr,
    |         ^^^^^^^^^^^^^^^^
134 |     /// UDP receive buffer size
135 |     pub udp_recv_buffer_size: usize,
    |         ^^^^^^^^^^^^^^^^^^^^
136 |     /// UDP send buffer size
137 |     pub udp_send_buffer_size: usize,
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: `NetworkConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `log_level` is never read
   --> src/config.rs:146:9
    |
142 | pub struct LoggingConfig {
    |            ------------- field in this struct
...
146 |     pub log_level: String,
    |         ^^^^^^^^^
    |
    = note: `LoggingConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `mint_cache_capacity`, `wallet_cache_capacity`, and `cache_refresh_interval_secs` are never read
   --> src/config.rs:153:9
    |
151 | pub struct CacheConfig {
    |            ----------- fields in this struct
152 |     /// Mint cache capacity (number of tokens)
153 |     pub mint_cache_capacity: usize,
    |         ^^^^^^^^^^^^^^^^^^^
154 |     /// Wallet cache capacity (number of wallets)
155 |     pub wallet_cache_capacity: usize,
    |         ^^^^^^^^^^^^^^^^^^^^^
156 |     /// Cache refresh interval (seconds)
157 |     pub cache_refresh_interval_secs: u64,
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = note: `CacheConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `worker_threads` is never read
   --> src/config.rs:164:9
    |
162 | pub struct PerformanceConfig {
    |            ----------------- field in this struct
163 |     /// Number of worker threads (0 = auto-detect)
164 |     pub worker_threads: usize,
    |         ^^^^^^^^^^^^^^
    |
    = note: `PerformanceConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: method `validate` is never used
   --> src/config.rs:241:12
    |
167 | impl Config {
    | ----------- method in this implementation
...
241 |     pub fn validate(&self) -> Result<()> {
    |            ^^^^^^^^

warning: associated items `from_bytes`, `is_buy`, `is_sell`, `mint_short`, and `size_sol` are never used
   --> src/udp_bus/messages.rs:209:12
    |
46  | impl TradeDecision {
    | ------------------ associated items in this implementation
...
209 |     pub fn from_bytes(buf: &[u8]) -> Result<Self> {
    |            ^^^^^^^^^^
...
255 |     pub fn is_buy(&self) -> bool {
    |            ^^^^^^
...
260 |     pub fn is_sell(&self) -> bool {
    |            ^^^^^^^
...
265 |     pub fn mint_short(&self) -> String {
    |            ^^^^^^^^^^
...
270 |     pub fn size_sol(&self) -> f64 {
    |            ^^^^^^^^

warning: associated items `new_success`, `new_failure`, and `to_bytes` are never used
   --> src/udp_bus/messages.rs:324:12
    |
313 | impl ExecutionConfirmation {
    | -------------------------- associated items in this implementation
...
324 |     pub fn new_success(
    |            ^^^^^^^^^^^
...
351 |     pub fn new_failure(mint: [u8; 32], side: u8) -> Self {
    |            ^^^^^^^^^^^
...
372 |     pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    |            ^^^^^^^^

warning: struct `HeatPulse` is never constructed
   --> src/udp_bus/messages.rs:475:12
    |
475 | pub struct HeatPulse {
    |            ^^^^^^^^^
    |
    = note: `HeatPulse` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: multiple associated items are never used
   --> src/udp_bus/messages.rs:509:15
    |
507 | impl HeatPulse {
    | -------------- associated items in this implementation
508 |     /// Total packet size in bytes
509 |     pub const SIZE: usize = 64;
    |               ^^^^
...
512 |     pub const MSG_TYPE: u8 = 6;
    |               ^^^^^^^^
...
515 |     pub fn new(
    |            ^^^
...
540 |     pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    |            ^^^^^^^^
...
556 |     pub fn from_bytes(buf: &[u8]) -> Result<Self> {
    |            ^^^^^^^^^^
...
591 |     pub fn pending_sol(&self) -> f64 {
    |            ^^^^^^^^^^^
...
596 |     pub fn has_jito(&self) -> bool {
    |            ^^^^^^^^
...
601 |     pub fn mint_short(&self) -> String {
    |            ^^^^^^^^^^

warning: method `reason_string` is never used
    --> src/udp_bus/messages.rs:1235:12
     |
1203 | impl TradeFailedAdvice {
     | ---------------------- method in this implementation
...
1235 |     pub fn reason_string(&self) -> String {
     |            ^^^^^^^^^^^^^

warning: associated items `MSG_TYPE`, `new`, and `to_bytes` are never used
    --> src/udp_bus/messages.rs:1385:15
     |
1383 | impl ExitAck {
     | ------------ associated items in this implementation
1384 |     pub const SIZE: usize = 64;
1385 |     pub const MSG_TYPE: u8 = 24;
     |               ^^^^^^^^
...
1388 |     pub fn new(mint: [u8; 32], trade_id: &str) -> Self {
     |            ^^^
...
1408 |     pub fn to_bytes(&self) -> Vec<u8> {
     |            ^^^^^^^^

warning: associated items `MSG_TYPE`, `new`, and `to_bytes` are never used
    --> src/udp_bus/messages.rs:1462:15
     |
1460 | impl EnterAck {
     | ------------- associated items in this implementation
1461 |     pub const SIZE: usize = 64;
1462 |     pub const MSG_TYPE: u8 = 27;
     |               ^^^^^^^^
...
1465 |     pub fn new(mint: [u8; 32], trade_id: &str) -> Self {
     |            ^^^
...
1485 |     pub fn to_bytes(&self) -> Vec<u8> {
     |            ^^^^^^^^

warning: associated items `STATUS_FAILED` and `is_failure` are never used
    --> src/udp_bus/messages.rs:1539:15
     |
1534 | impl TxConfirmed {
     | ---------------- associated items in this implementation
...
1539 |     pub const STATUS_FAILED: u8 = 1;
     |               ^^^^^^^^^^^^^
...
1586 |     pub fn is_failure(&self) -> bool {
     |            ^^^^^^^^^^

warning: associated items `STATUS_CONFIRMED`, `STATUS_FAILED`, `STATUS_TIMEOUT`, `new`, and `to_bytes` are never used
    --> src/udp_bus/messages.rs:1612:15
     |
1608 | impl TradeClosed {
     | ---------------- associated items in this implementation
...
1612 |     pub const STATUS_CONFIRMED: u8 = 0;
     |               ^^^^^^^^^^^^^^^^
1613 |     pub const STATUS_FAILED: u8 = 1;
     |               ^^^^^^^^^^^^^
1614 |     pub const STATUS_TIMEOUT: u8 = 2;
     |               ^^^^^^^^^^^^^^
...
1617 |     pub fn new(mint: [u8; 32], trade_id: &str, side: u8, final_status: u8) -> Self {
     |            ^^^
...
1639 |     pub fn to_bytes(&self) -> Vec<u8> {
     |            ^^^^^^^^

warning: associated items `new` and `to_bytes` are never used
    --> src/udp_bus/messages.rs:1709:12
     |
1704 | impl WindowMetrics {
     | ------------------ associated items in this implementation
...
1709 |     pub fn new(
     |            ^^^
...
1735 |     pub fn to_bytes(&self) -> Vec<u8> {
     |            ^^^^^^^^

warning: field `0` is never read
    --> src/udp_bus/messages.rs:1792:16
     |
1792 |     ExtendHold(ExtendHoldAdvice),
     |     ---------- ^^^^^^^^^^^^^^^^
     |     |
     |     field in this variant
     |
     = note: `AdviceMessage` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis
help: consider changing the field to be of unit type to suppress this warning while preserving the field numbering, or remove the field
     |
1792 -     ExtendHold(ExtendHoldAdvice),
1792 +     ExtendHold(()),
     |

warning: field `0` is never read
    --> src/udp_bus/messages.rs:1793:15
     |
1793 |     WidenExit(WidenExitAdvice),
     |     --------- ^^^^^^^^^^^^^^^
     |     |
     |     field in this variant
     |
     = note: `AdviceMessage` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis
help: consider changing the field to be of unit type to suppress this warning while preserving the field numbering, or remove the field
     |
1793 -     WidenExit(WidenExitAdvice),
1793 +     WidenExit(()),
     |

warning: associated items `new_default`, `send_with_retry`, `stats`, and `reset_stats` are never used
   --> src/udp_bus/sender.rs:66:18
    |
21  | impl DecisionBusSender {
    | ---------------------- associated items in this implementation
...
66  |     pub async fn new_default() -> Result<Self> {
    |                  ^^^^^^^^^^^
...
136 |     pub async fn send_with_retry(
    |                  ^^^^^^^^^^^^^^^
...
166 |     pub fn stats(&self) -> (u64, u64) {
    |            ^^^^^
...
173 |     pub fn reset_stats(&self) {
    |            ^^^^^^^^^^^

warning: struct `DecisionBatchSender` is never constructed
   --> src/udp_bus/sender.rs:180:12
    |
180 | pub struct DecisionBatchSender {
    |            ^^^^^^^^^^^^^^^^^^^

warning: associated items `new` and `send_batch` are never used
   --> src/udp_bus/sender.rs:186:18
    |
184 | impl DecisionBatchSender {
    | ------------------------ associated items in this implementation
185 |     /// Create new batch sender
186 |     pub async fn new(target_addr: SocketAddr) -> Result<Self> {
    |                  ^^^
...
195 |     pub async fn send_batch(
    |                  ^^^^^^^^^^

warning: multiple fields are never read
  --> src/udp_bus/receiver.rs:17:9
   |
16 | pub struct ReceiverStats {
   |            ------------- fields in this struct
17 |     pub total_received: u64,
   |         ^^^^^^^^^^^^^^
18 |     pub extend_hold: u64,
   |         ^^^^^^^^^^^
19 |     pub widen_exit: u64,
   |         ^^^^^^^^^^
20 |     pub late_opportunity: u64,
   |         ^^^^^^^^^^^^^^^^
21 |     pub copy_trade: u64,
   |         ^^^^^^^^^^
22 |     pub sol_price_update: u64,
   |         ^^^^^^^^^^^^^^^^
23 |     pub parse_errors: u64,
   |         ^^^^^^^^^^^^
   |
   = note: `ReceiverStats` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `stats` is never read
  --> src/udp_bus/receiver.rs:29:5
   |
27 | pub struct AdviceBusReceiver {
   |            ----------------- field in this struct
28 |     socket: Arc<UdpSocket>,
29 |     stats: Arc<ReceiverStats>,
   |     ^^^^^

warning: methods `stop`, `stats`, `reset_stats`, and `print_stats` are never used
   --> src/udp_bus/receiver.rs:344:12
    |
40  | impl AdviceBusReceiver {
    | ---------------------- methods in this implementation
...
344 |     pub fn stop(&self) {
    |            ^^^^
...
350 |     pub fn stats(&self) -> ReceiverStats {
    |            ^^^^^
...
363 |     pub fn reset_stats(&self) {
    |            ^^^^^^^^^^^
...
374 |     pub fn print_stats(&self) {
    |            ^^^^^^^^^^^

warning: fields `mempool_pending_sells` and `mempool_volume_sol` are never read
  --> src/feature_cache/mint_cache.rs:55:9
   |
17 | pub struct MintFeatures {
   |            ------------ fields in this struct
...
55 |     pub mempool_pending_sells: u32,
   |         ^^^^^^^^^^^^^^^^^^^^^
...
58 |     pub mempool_volume_sol: f64,
   |         ^^^^^^^^^^^^^^^^^^
   |
   = note: `MintFeatures` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `db_path` is never read
   --> src/feature_cache/mint_cache.rs:103:5
    |
101 | pub struct MintCache {
    |            --------- field in this struct
102 |     cache: Arc<DashMap<Pubkey, MintFeatures>>,
103 |     db_path: String,
    |     ^^^^^^^
    |
    = note: `MintCache` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: associated items `contains`, `is_empty`, `start_updater`, `update_cache`, and `query_mint_features` are never used
   --> src/feature_cache/mint_cache.rs:139:12
    |
106 | impl MintCache {
    | -------------- associated items in this implementation
...
139 |     pub fn contains(&self, mint: &Pubkey) -> bool {
    |            ^^^^^^^^
...
149 |     pub fn is_empty(&self) -> bool {
    |            ^^^^^^^^
...
154 |     pub fn start_updater(self: Arc<Self>, update_interval_ms: u64) -> tokio::task::JoinHandle<()> {
    |            ^^^^^^^^^^^^^
...
175 |     async fn update_cache(&self) -> Result<usize> {
    |              ^^^^^^^^^^^^
...
204 |     fn query_mint_features(db_path: &str) -> Result<Vec<(Pubkey, MintFeatures)>> {
    |        ^^^^^^^^^^^^^^^^^^^

warning: method `meets_copy_threshold` is never used
  --> src/feature_cache/wallet_cache.rs:36:12
   |
24 | impl WalletTier {
   | --------------- method in this implementation
...
36 |     pub fn meets_copy_threshold(&self) -> bool {
   |            ^^^^^^^^^^^^^^^^^^^^

warning: fields `mint`, `side`, `size_sol`, and `timestamp` are never read
  --> src/feature_cache/wallet_cache.rs:44:9
   |
43 | pub struct LastTrade {
   |            --------- fields in this struct
44 |     pub mint: Pubkey,
   |         ^^^^
45 |     pub side: u8,           // 0=BUY, 1=SELL
   |         ^^^^
46 |     pub size_sol: f64,
   |         ^^^^^^^^
47 |     pub timestamp: u64,     // Unix seconds
   |         ^^^^^^^^^
   |
   = note: `LastTrade` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `win_rate_7d`, `realized_pnl_7d`, `trade_count`, `avg_size`, `last_trade`, and `bootstrap_score` are never read
  --> src/feature_cache/wallet_cache.rs:54:9
   |
52 | pub struct WalletFeatures {
   |            -------------- fields in this struct
53 |     /// Win rate over last 7 days (0.0-1.0)
54 |     pub win_rate_7d: f64,
   |         ^^^^^^^^^^^
...
57 |     pub realized_pnl_7d: f64,
   |         ^^^^^^^^^^^^^^^
...
60 |     pub trade_count: u32,
   |         ^^^^^^^^^^^
...
63 |     pub avg_size: f64,
   |         ^^^^^^^^
...
72 |     pub last_trade: Option<LastTrade>,
   |         ^^^^^^^^^^
...
79 |     pub bootstrap_score: u8,
   |         ^^^^^^^^^^^^^^^
   |
   = note: `WalletFeatures` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: associated items `is_stale` and `wallet_short` are never used
   --> src/feature_cache/wallet_cache.rs:100:12
    |
98  | impl WalletFeatures {
    | ------------------- associated items in this implementation
99  |     /// Check if data is stale (>5 seconds old)
100 |     pub fn is_stale(&self) -> bool {
    |            ^^^^^^^^
...
109 |     pub fn wallet_short(wallet: &Pubkey) -> String {
    |            ^^^^^^^^^^^^

warning: methods `insert`, `contains`, `is_empty`, and `start_updater` are never used
   --> src/feature_cache/wallet_cache.rs:194:12
    |
179 | impl WalletCache {
    | ---------------- methods in this implementation
...
194 |     pub fn insert(&self, wallet: Pubkey, features: WalletFeatures) {
    |            ^^^^^^
...
199 |     pub fn contains(&self, wallet: &Pubkey) -> bool {
    |            ^^^^^^^^
...
209 |     pub fn is_empty(&self) -> bool {
    |            ^^^^^^^^
...
214 |     pub fn start_updater(self: Arc<Self>, update_interval_ms: u64) -> tokio::task::JoinHandle<()> {
    |            ^^^^^^^^^^^^^

warning: type alias `Scorer` is never used
  --> src/decision_engine/mod.rs:19:10
   |
19 | pub type Scorer = FollowThroughScorer;
   |          ^^^^^^

warning: type alias `Validator` is never used
  --> src/decision_engine/mod.rs:20:10
   |
20 | pub type Validator = TradeValidator;
   |          ^^^^^^^^^

warning: type alias `ValidationResult` is never used
  --> src/decision_engine/mod.rs:21:10
   |
21 | pub type ValidationResult = Result<ValidatedTrade, ValidationError>;
   |          ^^^^^^^^^^^^^^^^

warning: fields `buyers_2s`, `vol_5s_sol`, and `avg_wallet_confidence` are never read
  --> src/decision_engine/scoring.rs:29:9
   |
15 | pub struct ScoreComponents {
   |            --------------- fields in this struct
...
29 |     pub buyers_2s: u32,
   |         ^^^^^^^^^
30 |     pub vol_5s_sol: f64,
   |         ^^^^^^^^^^
31 |     pub avg_wallet_confidence: f64,
   |         ^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `ScoreComponents` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: method `breakdown` is never used
  --> src/decision_engine/scoring.rs:36:12
   |
34 | impl ScoreComponents {
   | -------------------- method in this implementation
35 |     /// Create a breakdown string for logging
36 |     pub fn breakdown(&self) -> String {
   |            ^^^^^^^^^

warning: multiple associated items are never used
   --> src/decision_engine/scoring.rs:87:12
    |
80  | impl FollowThroughScorer {
    | ------------------------ associated items in this implementation
...
87  |     pub fn with_thresholds(max_buyers: u32, max_volume: f64) -> Self {
    |            ^^^^^^^^^^^^^^^
...
96  |     pub fn with_weights(buyer_weight: f64, volume_weight: f64, quality_weight: f64) -> Self {
    |            ^^^^^^^^^^^^
...
155 |     pub fn calculate_with_wallets(
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
256 |     fn score_wallet_quality(&self, wallets: &[WalletFeatures]) -> u8 {
    |        ^^^^^^^^^^^^^^^^^^^^
...
276 |     pub fn meets_threshold(&self, score: u8, threshold: u8) -> bool {
    |            ^^^^^^^^^^^^^^^
...
288 |     pub fn position_size_multiplier(&self, score: u8) -> f64 {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^
...
303 |     pub fn estimate_success_probability(&self, score: u8) -> f64 {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: fields `mint`, `size_lamports`, `size_usd`, `slippage_bps`, and `follow_through_score` are never read
   --> src/decision_engine/validation.rs:102:9
    |
100 | pub struct ValidatedTrade {
    |            -------------- fields in this struct
101 |     /// Token mint address
102 |     pub mint: Pubkey,
    |         ^^^^
...
105 |     pub size_lamports: u64,
    |         ^^^^^^^^^^^^^
...
108 |     pub size_usd: f64,
    |         ^^^^^^^^
...
111 |     pub slippage_bps: u16,
    |         ^^^^^^^^^^^^
...
114 |     pub follow_through_score: u8,
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: `ValidatedTrade` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: variants `LaunchTooOld` and `InsufficientLiquidity` are never constructed
   --> src/decision_engine/validation.rs:150:5
    |
131 | pub enum ValidationError {
    |          --------------- variants in this enum
...
150 |     LaunchTooOld {
    |     ^^^^^^^^^^^^
...
154 |     InsufficientLiquidity {
    |     ^^^^^^^^^^^^^^^^^^^^^
    |
    = note: `ValidationError` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: associated function `with_config` is never used
   --> src/decision_engine/validation.rs:210:12
    |
201 | impl TradeValidator {
    | ------------------- associated function in this implementation
...
210 |     pub fn with_config(config: ValidationConfig) -> Self {
    |            ^^^^^^^^^^^

warning: variants `RankBased`, `Momentum`, `CopyTrade`, and `LateOpportunity` are never constructed
  --> src/decision_engine/triggers.rs:19:5
   |
18 | pub enum EntryTrigger {
   |          ------------ variants in this enum
19 |     RankBased,      // Path A: Top-ranked launch
   |     ^^^^^^^^^
20 |     Momentum,       // Path B: High recent activity
   |     ^^^^^^^^
21 |     CopyTrade,      // Path C: Following wallet
   |     ^^^^^^^^^
22 |     LateOpportunity, // Path D: Mature launch
   |     ^^^^^^^^^^^^^^^
   |
   = note: `EntryTrigger` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: method `as_str` is never used
  --> src/decision_engine/triggers.rs:26:12
   |
25 | impl EntryTrigger {
   | ----------------- method in this implementation
26 |     pub fn as_str(&self) -> &'static str {
   |            ^^^^^^

warning: multiple fields are never read
  --> src/decision_engine/triggers.rs:40:9
   |
38 | pub struct TriggerConfig {
   |            ------------- fields in this struct
39 |     // Path A: Rank-based
40 |     pub max_rank_for_instant: u8,          // Default: 2
   |         ^^^^^^^^^^^^^^^^^^^^
41 |     pub min_follow_through_rank: u8,       // Default: 60
   |         ^^^^^^^^^^^^^^^^^^^^^^^
42 |     pub rank_position_size_sol: f64,       // Default: 10.0 SOL
   |         ^^^^^^^^^^^^^^^^^^^^^^
...
45 |     pub min_buyers_2s: u32,                // Default: 5
   |         ^^^^^^^^^^^^^
46 |     pub min_vol_5s_sol: f64,               // Default: 8.0 SOL
   |         ^^^^^^^^^^^^^^
47 |     pub min_follow_through_momentum: u8,   // Default: 60
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^
48 |     pub momentum_position_size_sol: f64,   // Default: 8.0 SOL
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^
...
51 |     pub min_copy_tier: u8,                 // Default: 1 (Tier C)
   |         ^^^^^^^^^^^^^
52 |     pub min_copy_confidence: u8,           // Default: 75
   |         ^^^^^^^^^^^^^^^^^^^
53 |     pub min_copy_size_sol: f64,            // Default: 0.25 SOL
   |         ^^^^^^^^^^^^^^^^^
54 |     pub copy_multiplier: f64,              // Default: 1.2x wallet's size
   |         ^^^^^^^^^^^^^^^
...
57 |     pub min_launch_age_seconds: u64,       // Default: 1200 (20 min)
   |         ^^^^^^^^^^^^^^^^^^^^^^
58 |     pub min_vol_60s_late: f64,             // Default: 35.0 SOL
   |         ^^^^^^^^^^^^^^^^
59 |     pub min_buyers_60s_late: u32,          // Default: 40
   |         ^^^^^^^^^^^^^^^^^^^
60 |     pub min_follow_through_late: u8,       // Default: 70
   |         ^^^^^^^^^^^^^^^^^^^^^^^
61 |     pub late_position_size_sol: f64,       // Default: 5.0 SOL
   |         ^^^^^^^^^^^^^^^^^^^^^^
...
64 |     pub default_slippage_bps: u16,         // Default: 150 (1.5%)
   |         ^^^^^^^^^^^^^^^^^^^^
   |
   = note: `TriggerConfig` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `config` and `validator` are never read
   --> src/decision_engine/triggers.rs:117:5
    |
116 | pub struct TriggerEngine {
    |            ------------- fields in this struct
117 |     config: TriggerConfig,
    |     ^^^^^^
118 |     validator: TradeValidator,
    |     ^^^^^^^^^

warning: associated items `with_config`, `try_rank_based`, and `to_trade_decision` are never used
   --> src/decision_engine/triggers.rs:131:12
    |
121 | impl TriggerEngine {
    | ------------------ associated items in this implementation
...
131 |     pub fn with_config(config: TriggerConfig) -> Self {
    |            ^^^^^^^^^^^
...
148 |     pub fn try_rank_based(
    |            ^^^^^^^^^^^^^^
...
191 |     pub fn to_trade_decision(
    |            ^^^^^^^^^^^^^^^^^

warning: methods `try_momentum`, `try_copy_trade`, and `try_late_opportunity` are never used
   --> src/decision_engine/triggers.rs:307:12
    |
295 | impl TriggerEngine {
    | ------------------ methods in this implementation
...
307 |     pub fn try_momentum(
    |            ^^^^^^^^^^^^
...
373 |     pub fn try_copy_trade(
    |            ^^^^^^^^^^^^^^
...
447 |     pub fn try_late_opportunity(
    |            ^^^^^^^^^^^^^^^^^^^^

warning: variants `Win` and `Loss` are never constructed
  --> src/decision_engine/guardrails.rs:17:5
   |
16 | pub enum TradeOutcome {
   |          ------------ variants in this enum
17 |     Win,
   |     ^^^
18 |     Loss,
   |     ^^^^
   |
   = note: `TradeOutcome` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `timestamp` and `mint` are never read
  --> src/decision_engine/guardrails.rs:24:5
   |
23 | struct LossEntry {
   |        --------- fields in this struct
24 |     timestamp: u64,
   |     ^^^^^^^^^
25 |     mint: [u8; 32],
   |     ^^^^
   |
   = note: `LossEntry` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `trigger_type` and `timestamp` are never read
  --> src/decision_engine/guardrails.rs:39:5
   |
38 | struct RateLimitEntry {
   |        -------------- fields in this struct
39 |     trigger_type: u8, // 0=rank, 1=momentum, 2=copy, 3=late
   |     ^^^^^^^^^^^^
40 |     timestamp: u64,
   |     ^^^^^^^^^
   |
   = note: `RateLimitEntry` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `recent_losses` is never read
   --> src/decision_engine/guardrails.rs:100:5
    |
95  | pub struct Guardrails {
    |            ---------- field in this struct
...
100 |     recent_losses: Arc<Mutex<VecDeque<LossEntry>>>,
    |     ^^^^^^^^^^^^^
    |
    = note: `Guardrails` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: associated items `new`, `cleanup_old_creator_trades`, `record_outcome`, `stats`, and `print_stats` are never used
   --> src/decision_engine/guardrails.rs:120:12
    |
118 | impl Guardrails {
    | --------------- associated items in this implementation
119 |     /// Create new guardrails system with default configuration
120 |     pub fn new() -> Self {
    |            ^^^
...
473 |     pub fn cleanup_old_creator_trades(&self) -> Result<usize> {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^^
...
497 |     pub fn record_outcome(
    |            ^^^^^^^^^^^^^^
...
560 |     pub fn stats(&self) -> GuardrailStats {
    |            ^^^^^
...
585 |     pub fn print_stats(&self) {
    |            ^^^^^^^^^^^

warning: struct `GuardrailStats` is never constructed
   --> src/decision_engine/guardrails.rs:612:12
    |
612 | pub struct GuardrailStats {
    |            ^^^^^^^^^^^^^^
    |
    = note: `GuardrailStats` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: variants `Rank` and `Momentum` are never constructed
  --> src/decision_engine/logging.rs:17:5
   |
16 | pub enum TriggerType {
   |          ----------- variants in this enum
17 |     Rank,
   |     ^^^^
18 |     Momentum,
   |     ^^^^^^^^
   |
   = note: `TriggerType` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: associated function `from_u8` is never used
  --> src/decision_engine/logging.rs:33:12
   |
23 | impl TriggerType {
   | ---------------- associated function in this implementation
...
33 |     pub fn from_u8(value: u8) -> Self {
   |            ^^^^^^^

warning: methods `entries_logged` and `next_decision_id` are never used
   --> src/decision_engine/logging.rs:187:12
    |
115 | impl DecisionLogger {
    | ------------------- methods in this implementation
...
187 |     pub fn entries_logged(&self) -> u64 {
    |            ^^^^^^^^^^^^^^
...
192 |     pub fn next_decision_id(&self) -> u64 {
    |            ^^^^^^^^^^^^^^^^

warning: struct `DecisionLogBuilder` is never constructed
   --> src/decision_engine/logging.rs:198:12
    |
198 | pub struct DecisionLogBuilder {
    |            ^^^^^^^^^^^^^^^^^^

warning: multiple associated items are never used
   --> src/decision_engine/logging.rs:223:12
    |
221 | impl DecisionLogBuilder {
    | ----------------------- associated items in this implementation
222 |     /// Create new log builder
223 |     pub fn new(mint: [u8; 32], trigger_type: TriggerType, side: u8) -> Self {
    |            ^^^
...
250 |     pub fn validation(mut self, fees: f64, impact: f64, tp: f64) -> Self {
    |            ^^^^^^^^^^
...
258 |     pub fn score(mut self, score: u8) -> Self {
    |            ^^^^^
...
264 |     pub fn position(mut self, size_sol: f64, size_usd: f64, confidence: u8) -> Self {
    |            ^^^^^^^^
...
272 |     pub fn ev(mut self, ev_usd: f64, success_prob: f64) -> Self {
    |            ^^
...
279 |     pub fn rank(mut self, rank: u8) -> Self {
    |            ^^^^
...
285 |     pub fn wallet(mut self, wallet: [u8; 32], tier: u8) -> Self {
    |            ^^^^^^
...
292 |     pub fn build(self) -> DecisionLogEntry {
    |            ^^^^^

warning: variants `Confirmed` and `Failed` are never constructed
  --> src/decision_engine/position_tracker.rs:18:5
   |
14 | pub enum PositionState {
   |          ------------- variants in this enum
...
18 |     Confirmed,
   |     ^^^^^^^^^
19 |     /// Transaction failed or timed out
20 |     Failed,
   |     ^^^^^^
   |
   = note: `PositionState` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `mint`, `signature`, `expected_tokens`, `expected_sol_lamports`, `expected_slip_bps`, and `state` are never read
  --> src/decision_engine/position_tracker.rs:27:9
   |
25 | pub struct ProvisionalPosition {
   |            ------------------- fields in this struct
26 |     /// Token mint address (hex string)
27 |     pub mint: String,
   |         ^^^^
...
30 |     pub signature: String,
   |         ^^^^^^^^^
...
36 |     pub expected_tokens: u64,
   |         ^^^^^^^^^^^^^^^
...
39 |     pub expected_sol_lamports: u64,
   |         ^^^^^^^^^^^^^^^^^^^^^
...
42 |     pub expected_slip_bps: u16,
   |         ^^^^^^^^^^^^^^^^^
...
51 |     pub state: PositionState,
   |         ^^^^^
   |
   = note: `ProvisionalPosition` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `entry_timestamp` and `trigger_source` are never read
  --> src/decision_engine/position_tracker.rs:64:9
   |
56 | pub struct ActivePosition {
   |            -------------- fields in this struct
...
64 |     pub entry_timestamp: u64,
   |         ^^^^^^^^^^^^^^^
...
91 |     pub trigger_source: String,
   |         ^^^^^^^^^^^^^^
   |
   = note: `ActivePosition` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: methods `current_value_usd` and `unrealized_pnl_usd` are never used
   --> src/decision_engine/position_tracker.rs:197:12
    |
97  | impl ActivePosition {
    | ------------------- methods in this implementation
...
197 |     pub fn current_value_usd(&self, current_price_sol: f64, sol_price_usd: f64) -> f64 {
    |            ^^^^^^^^^^^^^^^^^
...
202 |     pub fn unrealized_pnl_usd(&self, current_price_sol: f64, sol_price_usd: f64) -> f64 {
    |            ^^^^^^^^^^^^^^^^^^

warning: variant `Emergency` is never constructed
   --> src/decision_engine/position_tracker.rs:245:5
    |
209 | pub enum ExitReason {
    |          ---------- variant in this enum
...
245 |     Emergency {
    |     ^^^^^^^^^
    |
    = note: `ExitReason` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: methods `provisional_count`, `is_full`, and `adjust_profit_targets` are never used
   --> src/decision_engine/position_tracker.rs:376:12
    |
285 | impl PositionTracker {
    | -------------------- methods in this implementation
...
376 |     pub fn provisional_count(&self) -> usize {
    |            ^^^^^^^^^^^^^^^^^
...
427 |     pub fn is_full(&self) -> bool {
    |            ^^^^^^^
...
451 |     pub fn adjust_profit_targets(&mut self, mint: &str, multiplier: f64) -> bool {
    |            ^^^^^^^^^^^^^^^^^^^^^

warning: variant `Loss` is never constructed
  --> src/decision_engine/position_sizer.rs:14:5
   |
12 | pub enum TradeResult {
   |          ----------- variant in this enum
13 |     Win,
14 |     Loss,
   |     ^^^^
   |
   = note: `TradeResult` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: variants `Fixed`, `KellyCriterion`, and `Tiered` are never constructed
  --> src/decision_engine/position_sizer.rs:21:5
   |
19 | pub enum SizingStrategy {
   |          -------------- variants in this enum
20 |     /// Fixed size regardless of confidence
21 |     Fixed { size_sol: f64 },
   |     ^^^^^
...
30 |     KellyCriterion { 
   |     ^^^^^^^^^^^^^^
...
36 |     Tiered {
   |     ^^^^^^
   |
   = note: `SizingStrategy` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: methods `get_recommended_size`, `record_outcome`, and `reset_adaptive_scaling` are never used
   --> src/decision_engine/position_sizer.rs:246:12
    |
103 | impl PositionSizer {
    | ------------------ methods in this implementation
...
246 |     pub fn get_recommended_size(&self, confidence: u8) -> f64 {
    |            ^^^^^^^^^^^^^^^^^^^^
...
251 |     pub fn record_outcome(&self, result: TradeResult) {
    |            ^^^^^^^^^^^^^^
...
286 |     pub fn reset_adaptive_scaling(&self) {
    |            ^^^^^^^^^^^^^^^^^^^^^^

warning: fields `advice_processing_latency`, `db_query_duration`, `db_errors`, and `udp_parse_errors` are never read
  --> src/metrics.rs:72:9
   |
38 | pub struct BrainMetrics {
   |            ------------ fields in this struct
...
72 |     pub advice_processing_latency: Histogram,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^
...
81 |     pub db_query_duration: Histogram,
   |         ^^^^^^^^^^^^^^^^^
82 |     pub db_errors: IntCounter,
   |         ^^^^^^^^^
...
87 |     pub udp_parse_errors: IntCounter,
   |         ^^^^^^^^^^^^^^^^

warning: variant `WalletActivity` is never constructed
   --> src/metrics.rs:402:5
    |
399 | pub enum DecisionPathway {
    |          --------------- variant in this enum
...
402 |     WalletActivity,
    |     ^^^^^^^^^^^^^^

warning: variants `LossBackoff` and `PositionLimit` are never constructed
   --> src/metrics.rs:420:5
    |
419 | pub enum GuardrailType {
    |          ------------- variants in this enum
420 |     LossBackoff,
    |     ^^^^^^^^^^^
421 |     PositionLimit,
    |     ^^^^^^^^^^^^^

warning: function `update_active_positions` is never used
   --> src/metrics.rs:449:8
    |
449 | pub fn update_active_positions(count: i64) {
    |        ^^^^^^^^^^^^^^^^^^^^^^^

warning: function `record_udp_parse_error` is never used
   --> src/metrics.rs:487:8
    |
487 | pub fn record_udp_parse_error() {
    |        ^^^^^^^^^^^^^^^^^^^^^^

warning: function `record_db_error` is never used
   --> src/metrics.rs:492:8
    |
492 | pub fn record_db_error() {
    |        ^^^^^^^^^^^^^^^

warning: field `start` is never read
   --> src/metrics.rs:498:5
    |
497 | pub struct DecisionTimer {
    |            ------------- field in this struct
498 |     start: std::time::Instant,
    |     ^^^^^

warning: method `observe` is never used
   --> src/metrics.rs:508:12
    |
501 | impl DecisionTimer {
    | ------------------ method in this implementation
...
508 |     pub fn observe(self) {
    |            ^^^^^^^

warning: struct `AdviceTimer` is never constructed
   --> src/metrics.rs:515:12
    |
515 | pub struct AdviceTimer {
    |            ^^^^^^^^^^^

warning: associated items `start` and `observe` are never used
   --> src/metrics.rs:520:12
    |
519 | impl AdviceTimer {
    | ---------------- associated items in this implementation
520 |     pub fn start() -> Self {
    |            ^^^^^
...
526 |     pub fn observe(self) {
    |            ^^^^^^^

warning: field `start` is never read
   --> src/metrics.rs:534:5
    |
533 | pub struct DbQueryTimer {
    |            ------------ field in this struct
534 |     start: std::time::Instant,
    |     ^^^^^

warning: method `observe` is never used
   --> src/metrics.rs:544:12
    |
537 | impl DbQueryTimer {
    | ----------------- method in this implementation
...
544 |     pub fn observe(self) {
    |            ^^^^^^^

warning: methods `is_pending`, `trade_id`, and `age` are never used
  --> src/trade_state.rs:67:12
   |
55 | impl TradeState {
   | --------------- methods in this implementation
...
67 |     pub fn is_pending(&self) -> bool {
   |            ^^^^^^^^^^
...
72 |     pub fn trade_id(&self) -> Option<&str> {
   |            ^^^^^^^^
...
83 |     pub fn age(&self) -> Duration {
   |            ^^^

warning: methods `mark_buy_failed` and `reconcile_state` are never used
   --> src/trade_state.rs:209:12
    |
101 | impl TradeStateTracker {
    | ---------------------- methods in this implementation
...
209 |     pub fn mark_buy_failed(&mut self, mint: String, trade_id: String) {
    |            ^^^^^^^^^^^^^^^
...
296 |     pub fn reconcile_state(&mut self, mint: String, trade_id: String, confirmed: bool) {
    |            ^^^^^^^^^^^^^^^

warning: field `mint` is never read
  --> src/mint_reservation.rs:15:9
   |
14 | pub struct MintReservation {
   |            --------------- field in this struct
15 |     pub mint: String,
   |         ^^^^
   |
   = note: `MintReservation` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: methods `release` and `total_count` are never used
   --> src/mint_reservation.rs:89:12
    |
39  | impl MintReservationManager {
    | --------------------------- methods in this implementation
...
89  |     pub fn release(&mut self, mint: &str) {
    |            ^^^^^^^
...
108 |     pub fn total_count(&self) -> usize {
    |            ^^^^^^^^^^^

warning: this function has too many arguments (8/7)
  --> src/udp_bus/messages.rs:57:5
   |
57 | /     fn calculate_checksum(msg_type: u8, protocol_version: u8, mint: &[u8; 32], side: u8, 
58 | |                           size_lamports: u64, slippage_bps: u16, confidence: u8, retry_count: u8) -> u8 {
   | |_______________________________________________________________________________________________________^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments
   = note: `#[warn(clippy::too_many_arguments)]` on by default

warning: methods with the following characteristics: (`to_*` and `self` type is `Copy`) usually take `self` by value
   --> src/udp_bus/messages.rs:193:21
    |
193 |     pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    |                     ^^^^^
    |
    = help: consider choosing a less ambiguous name
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#wrong_self_convention
    = note: `#[warn(clippy::wrong_self_convention)]` on by default

warning: methods with the following characteristics: (`to_*` and `self` type is `Copy`) usually take `self` by value
   --> src/udp_bus/messages.rs:372:21
    |
372 |     pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    |                     ^^^^^
    |
    = help: consider choosing a less ambiguous name
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#wrong_self_convention

warning: this function has too many arguments (8/7)
   --> src/udp_bus/messages.rs:515:5
    |
515 | /     pub fn new(
516 | |         mint: [u8; 32],
517 | |         window_ms: u16,
518 | |         pending_buys: u16,
...   |
523 | |         ttl_ms: u16,
524 | |     ) -> Self {
    | |_____________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: methods with the following characteristics: (`to_*` and `self` type is `Copy`) usually take `self` by value
   --> src/udp_bus/messages.rs:540:21
    |
540 |     pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    |                     ^^^^^
    |
    = help: consider choosing a less ambiguous name
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#wrong_self_convention

warning: called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
   --> src/decision_engine/guardrails.rs:245:18
    |
245 |                 .last() 
    |                  ^^^^^^ help: try: `next_back()`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#double_ended_iterator_last
    = note: `#[warn(clippy::double_ended_iterator_last)]` on by default

warning: implementation of inherent method `to_string(&self) -> String` for type `decision_engine::position_tracker::ExitReason`
   --> src/decision_engine/position_tracker.rs:252:5
    |
252 | /     pub fn to_string(&self) -> String {
253 | |         match self {
254 | |             ExitReason::ProfitTarget { tier, pnl_pct, exit_percent } => {
255 | |                 format!("TP{} ({:+.1}%, exit {}%)", tier, pnl_pct, exit_percent)
...   |
273 | |     }
    | |_____^
    |
    = help: implement trait `Display` for type `decision_engine::position_tracker::ExitReason` instead
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#inherent_to_string
    = note: `#[warn(clippy::inherent_to_string)]` on by default

warning: this function has too many arguments (8/7)
   --> src/decision_engine/position_tracker.rs:295:5
    |
295 | /     pub fn add_provisional(&mut self, mint: String, signature: String, expected_tokens: u64, 
296 | |                           expected_sol_lamports: u64, expected_slip_bps: u16, side: u8, 
297 | |                           mempool_pending_buys: u32) {
    | |____________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this function has too many arguments (12/7)
    --> src/main.rs:1377:1
     |
1377 | / async fn process_late_opportunity(
1378 | |     late: &LateOpportunityAdvice,
1379 | |     mint_cache: &MintCache,
1380 | |     scorer: &FollowThroughScorer,
...    |
1389 | |     config: &Config,
1390 | | ) -> Result<()> {
     | |_______________^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: the borrowed expression implements the required traits
    --> src/main.rs:1541:27
     |
1541 |         mint: hex::encode(&late.mint),
     |                           ^^^^^^^^^^ help: change this to: `late.mint`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args
     = note: `#[warn(clippy::needless_borrows_for_generic_args)]` on by default

warning: this function has too many arguments (12/7)
    --> src/main.rs:1631:1
     |
1631 | / async fn process_momentum_opportunity(
1632 | |     momentum: &MomentumOpportunityAdvice,
1633 | |     mint_cache: &MintCache,
1634 | |     scorer: &FollowThroughScorer,
...    |
1643 | |     config: &Config,
1644 | | ) -> Result<()> {
     | |_______________^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: the borrowed expression implements the required traits
    --> src/main.rs:1728:32
     |
1728 |     let mint_str = hex::encode(&momentum.mint);
     |                                ^^^^^^^^^^^^^^ help: change this to: `momentum.mint`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args

warning: this function has too many arguments (12/7)
    --> src/main.rs:1781:1
     |
1781 | / async fn process_rank_opportunity(
1782 | |     rank: &RankOpportunityAdvice,
1783 | |     mint_cache: &MintCache,
1784 | |     scorer: &FollowThroughScorer,
...    |
1793 | |     config: &Config,
1794 | | ) -> Result<()> {
     | |_______________^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: casting to the same type is unnecessary (`u8` -> `u8`)
    --> src/main.rs:1828:22
     |
1828 |     let confidence = (rank.score + rank_bonus).min(100) as u8;
     |                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: try: `(rank.score + rank_bonus).min(100)`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unnecessary_cast
     = note: `#[warn(clippy::unnecessary_cast)]` on by default

warning: the borrowed expression implements the required traits
    --> src/main.rs:1875:32
     |
1875 |     let mint_str = hex::encode(&rank.mint);
     |                                ^^^^^^^^^^ help: change this to: `rank.mint`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args

warning: this function has too many arguments (13/7)
    --> src/main.rs:1928:1
     |
1928 | / async fn process_copy_trade(
1929 | |     copy: &CopyTradeAdvice,
1930 | |     mint_cache: &MintCache,
1931 | |     wallet_cache: &WalletCache,
...    |
1941 | |     config: &Config,
1942 | | ) -> Result<()> {
     | |_______________^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: the borrowed expression implements the required traits
    --> src/main.rs:2111:27
     |
2111 |         mint: hex::encode(&copy.mint),
     |                           ^^^^^^^^^^ help: change this to: `copy.mint`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args

warning: the borrowed expression implements the required traits
    --> src/main.rs:2124:34
     |
2124 |         wallet: Some(hex::encode(&copy.wallet)),
     |                                  ^^^^^^^^^^^^ help: change this to: `copy.wallet`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args

warning: the borrowed expression implements the required traits
    --> src/main.rs:2131:32
     |
2131 |     let mint_str = hex::encode(&copy.mint);
     |                                ^^^^^^^^^^ help: change this to: `copy.mint`
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args

warning: clamp-like pattern without using clamp function
    --> src/main.rs:2427:31
     |
2427 |           let bootstrap_score = ((50 + wins * 2) as i32 + (total_pnl_sol / 5.0) as i32)
     |  _______________________________^
2428 | |             .min(90)
2429 | |             .max(0) as u8;
     | |___________________^ help: replace with clamp: `((50 + wins * 2) as i32 + (total_pnl_sol / 5.0) as i32).clamp(0, 90)`
     |
     = note: clamp will panic if max < min
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#manual_clamp
     = note: `#[warn(clippy::manual_clamp)]` on by default

error[E0063]: missing fields `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol` in initializer of `feature_cache::mint_cache::MintFeatures`
   --> src/decision_engine/validation.rs:474:29
    |
474 |         let mint_features = MintFeatures {
    |                             ^^^^^^^^^^^^ missing `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol`

error[E0063]: missing fields `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol` in initializer of `feature_cache::mint_cache::MintFeatures`
   --> src/decision_engine/triggers.rs:212:9
    |
212 |         MintFeatures {
    |         ^^^^^^^^^^^^ missing `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol`

error[E0063]: missing fields `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol` in initializer of `feature_cache::mint_cache::MintFeatures`
   --> src/decision_engine/triggers.rs:518:9
    |
518 |         MintFeatures {
    |         ^^^^^^^^^^^^ missing `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol`

error[E0063]: missing fields `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol` in initializer of `feature_cache::mint_cache::MintFeatures`
   --> src/decision_engine/triggers.rs:589:9
    |
589 |         MintFeatures {
    |         ^^^^^^^^^^^^ missing `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol`

error[E0063]: missing fields `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol` in initializer of `feature_cache::mint_cache::MintFeatures`
   --> src/decision_engine/triggers.rs:648:9
    |
648 |         MintFeatures {
    |         ^^^^^^^^^^^^ missing `mempool_pending_buys`, `mempool_pending_sells` and `mempool_volume_sol`

For more information about this error, try `rustc --explain E0063`.
error: could not compile `decision_engine` (bin "decision_engine" test) due to 5 previous errors; 41 warnings emitted
```

**Total warnings**: 149

### Recommendations

1. **Padding fields in messages.rs**: These are intentional for fixed-size UDP packets - DO NOT REMOVE
2. **Unused imports**: Review and remove if genuinely unused
3. **Unused variables**: Add `_` prefix if intentionally unused for future use
4. **Dead code**: Review each warning individually

---

## Data-Mining Service

### Unused Code Warnings

```
warning: unused import: `TradeSide`
 --> src/parser/raydium.rs:1:31
  |
1 | use crate::types::{PumpEvent, TradeSide};
  |                               ^^^^^^^^^
  |
  = note: requested on the command line with `-W unused-imports`

warning: empty line after doc comment
  --> src/udp/mod.rs:9:1
   |
9  | / /// - Type 5: SolPriceUpdate - SOL price update from oracle
10 | |
   | |_^
11 |   use std::net::UdpSocket;
   |   - the comment documents this `use` import
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#empty_line_after_doc_comments
   = note: `#[warn(clippy::empty_line_after_doc_comments)]` on by default
   = help: if the empty line is unintentional, remove it
help: if the comment should document the parent module use an inner doc comment
   |
1  ~ //! UDP Advisory Sender - Send advisories to execution bot
2  ~ //! 
3  ~ //! Sends various advisory types via UDP to help execution bot make better decisions.
4  ~ //! Advisory types:
5  ~ //! - Type 1: ExtendHold - Hold position longer than normal
6  ~ //! - Type 2: WidenExit - Increase exit slippage tolerance
7  ~ //! - Type 3: LateOpportunity - New token with strong momentum
8  ~ //! - Type 4: CopyTrade - Alpha wallet activity detected
9  ~ //! - Type 5: SolPriceUpdate - SOL price update from oracle
   |

warning: empty line after doc comment
 --> src/momentum_tracker.rs:4:1
  |
4 | / /// and volume spikes as they happen.
5 | |
  | |_^
6 |   use std::collections::{HashMap, VecDeque};
  |   - the comment documents this `use` import
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#empty_line_after_doc_comments
  = help: if the empty line is unintentional, remove it
help: if the comment should document the parent module use an inner doc comment
  |
1 ~ //! Real-time momentum tracker for confirmed transactions
2 ~ //! 
3 ~ //! Tracks rolling windows of buy/sell activity to detect momentum patterns
4 ~ //! and volume spikes as they happen.
  |

warning: unused import: `anyhow::Result`
 --> src/momentum_tracker.rs:8:5
  |
8 | use anyhow::Result;
  |     ^^^^^^^^^^^^^^

warning: this `if` statement can be collapsed
  --> src/momentum_tracker.rs:72:13
   |
72 | /             if event.timestamp_ms >= window_start_ms {
73 | |                 if matches!(event.side, TradeSide::Buy) {
74 | |                     buys += 1;
75 | |                     volume_sol += event.amount_sol;
...  |
78 | |             }
   | |_____________^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if
   = note: `#[warn(clippy::collapsible_if)]` on by default
help: collapse nested if block
   |
72 ~             if event.timestamp_ms >= window_start_ms && matches!(event.side, TradeSide::Buy) {
73 +                 buys += 1;
74 +                 volume_sol += event.amount_sol;
75 +                 unique_buyers.insert(&event.trader);
76 +             }
   |

warning: empty line after doc comment
 --> src/window_tracker.rs:7:1
  |
7 | / /// - alpha_wallet_hits_10s: Alpha wallet buys in last 10 seconds
8 | |
  | |_^
9 |   use std::collections::{HashMap, HashSet, VecDeque};
  |   - the comment documents this `use` import
  |
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#empty_line_after_doc_comments
  = help: if the empty line is unintentional, remove it
help: if the comment should document the parent module use an inner doc comment
  |
1 ~ //! Real-time sliding window tracker for market metrics
2 ~ //! 
3 ~ //! Tracks rolling windows of trading activity to calculate:
4 ~ //! - volume_sol_1s: SOL volume in last 1 second
5 ~ //! - unique_buyers_1s: Unique buyers in last 1 second
6 ~ //! - price_change_bps_2s: Price change over 2 seconds (basis points)
7 ~ //! - alpha_wallet_hits_10s: Alpha wallet buys in last 10 seconds
  |

warning: unused variable: `signature`
   --> src/parser/mod.rs:189:9
    |
189 |         signature: &Signature,
    |         ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_signature`
    |
    = note: requested on the command line with `-W unused-variables`

warning: unused variable: `slot`
   --> src/parser/mod.rs:190:9
    |
190 |         slot: u64,
    |         ^^^^ help: if this is intentional, prefix it with an underscore: `_slot`

warning: unused variable: `block_time`
   --> src/parser/mod.rs:191:9
    |
191 |         block_time: i64,
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_block_time`

warning: unused variable: `signature`
   --> src/parser/mod.rs:241:9
    |
241 |         signature: &Signature,
    |         ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_signature`

warning: unused variable: `slot`
   --> src/parser/mod.rs:242:9
    |
242 |         slot: u64,
    |         ^^^^ help: if this is intentional, prefix it with an underscore: `_slot`

warning: unused variable: `block_time`
   --> src/parser/mod.rs:243:9
    |
243 |         block_time: i64,
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_block_time`

warning: unused variable: `uri`
   --> src/parser/mod.rs:301:13
    |
301 |         let uri = self.read_borsh_string(data, &mut offset)?;
    |             ^^^ help: if this is intentional, prefix it with an underscore: `_uri`

warning: unused variable: `bonding_curve`
   --> src/parser/mod.rs:320:13
    |
320 |         let bonding_curve = &account_keys[accounts[2]];
    |             ^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_bonding_curve`

warning: unused variable: `signature`
   --> src/parser/mod.rs:289:9
    |
289 |         signature: &Signature,
    |         ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_signature`

warning: unused variable: `slot`
   --> src/parser/mod.rs:290:9
    |
290 |         slot: u64,
    |         ^^^^ help: if this is intentional, prefix it with an underscore: `_slot`

warning: unused variable: `block_time`
   --> src/parser/mod.rs:291:9
    |
291 |         block_time: i64,
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_block_time`

warning: unused variable: `signature`
   --> src/parser/raydium.rs:156:9
    |
156 |         signature: &Signature,
    |         ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_signature`

warning: unused variable: `slot`
   --> src/parser/raydium.rs:157:9
    |
157 |         slot: u64,
    |         ^^^^ help: if this is intentional, prefix it with an underscore: `_slot`

warning: unused variable: `block_time`
   --> src/parser/raydium.rs:158:9
    |
158 |         block_time: i64,
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_block_time`

warning: unused variable: `signature`
   --> src/parser/raydium.rs:216:9
    |
216 |         signature: &Signature,
    |         ^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_signature`

warning: unused variable: `slot`
   --> src/parser/raydium.rs:217:9
    |
217 |         slot: u64,
    |         ^^^^ help: if this is intentional, prefix it with an underscore: `_slot`

warning: unused variable: `block_time`
   --> src/parser/raydium.rs:218:9
    |
218 |         block_time: i64,
    |         ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_block_time`

warning: constant `COMPLETE_EVENT_DISCRIMINATOR` is never used
  --> src/parser/mod.rs:17:7
   |
17 | const COMPLETE_EVENT_DISCRIMINATOR: [u8; 8] = [95, 114, 97, 156, 212, 46, 152, 8];
   |       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: requested on the command line with `-W dead-code`

warning: accessing first element with `volumes.get(0)`
   --> src/db/aggregator.rs:148:24
    |
148 |             let top1 = volumes.get(0).copied().unwrap_or(0.0) / total_buy_vol.max(1e-9);
    |                        ^^^^^^^^^^^^^^ help: try: `volumes.first()`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#get_first
    = note: `#[warn(clippy::get_first)]` on by default

warning: very complex type used. Consider factoring parts into `type` definitions
   --> src/db/mod.rs:352:71
    |
352 |     pub fn get_recent_windows(&self, mint: &str, time_cutoff: i64) -> Result<Vec<(u32, f64, u32, i64, f64)>> {
    |                                                                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#type_complexity
    = note: `#[warn(clippy::type_complexity)]` on by default

warning: accessing first element with `accounts.get(0)`
   --> src/decoder/mod.rs:101:27
    |
101 |         Action::Create => accounts.get(0).cloned(),
    |                           ^^^^^^^^^^^^^^^ help: try: `accounts.first()`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#get_first

warning: doc list item without indentation
   --> src/udp/mod.rs:124:9
    |
124 |     /// Send CopyTrade advisory (Type 13)
    |         ^
    |
    = help: if this is supposed to be its own paragraph, add a blank line
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#doc_lazy_continuation
    = note: `#[warn(clippy::doc_lazy_continuation)]` on by default
help: indent this line
    |
124 |     ///   Send CopyTrade advisory (Type 13)
    |         ++

warning: this `if let` can be collapsed into the outer `if let`
   --> src/pyth_subscriber.rs:130:33
    |
130 | / ...                   if let UpdateOneof::Account(account_update) = update_oneof {
131 | | ...                       // Parse Pyth price from account data
132 | | ...                       if let Some(price) = self.parse_pyth_price(&account_update.account.unwrap().data) {
133 | | ...                           latest_price = Some(price);
...   |
142 | | ...                   }
    | |_______________________^
    |
help: the outer pattern can be modified to include the inner pattern
   --> src/pyth_subscriber.rs:129:41
    |
129 | ...                   if let Some(update_oneof) = update.update_oneof {
    |                                   ^^^^^^^^^^^^ replace this binding
130 | ...                       if let UpdateOneof::Account(account_update) = update_oneof {
    |                                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ with this pattern
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_match
    = note: `#[warn(clippy::collapsible_match)]` on by default

warning: this `map_or` can be simplified
  --> src/pyth_subscriber_rpc.rs:91:49
   |
91 | ...                   let price_changed = latest_price.map_or(true, |old| (price - old).abs() > 0.01);
   |                                           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unnecessary_map_or
   = note: `#[warn(clippy::unnecessary_map_or)]` on by default
help: use is_none_or instead
   |
91 -                             let price_changed = latest_price.map_or(true, |old| (price - old).abs() > 0.01);
91 +                             let price_changed = latest_price.is_none_or(|old| (price - old).abs() > 0.01);
   |

warning: this `map_or` can be simplified
   --> src/pyth_subscriber_rpc.rs:114:48
    |
114 |                         let should_broadcast = last_broadcast_price.map_or(true, |last| (price - last).abs() > 0.001);
    |                                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unnecessary_map_or
help: use is_none_or instead
    |
114 -                         let should_broadcast = last_broadcast_price.map_or(true, |last| (price - last).abs() > 0.001);
114 +                         let should_broadcast = last_broadcast_price.is_none_or(|last| (price - last).abs() > 0.001);
    |

warning: manual `!RangeInclusive::contains` implementation
   --> src/pyth_subscriber_rpc.rs:166:12
    |
166 |         if price < 1.0 || price > 10_000.0 {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: use: `!(1.0..=10_000.0).contains(&price)`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#manual_range_contains
    = note: `#[warn(clippy::manual_range_contains)]` on by default

warning: this `map_or` can be simplified
   --> src/pyth_http.rs:161:44
    |
161 |                       let should_broadcast = latest_price.map_or(true, |old| {
    |  ____________________________________________^
162 | |                         (filtered_price - old).abs() > 0.10
163 | |                     });
    | |______________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unnecessary_map_or
help: use is_none_or instead
    |
161 -                     let should_broadcast = latest_price.map_or(true, |old| {
161 +                     let should_broadcast = latest_price.is_none_or(|old| {
    |

warning: manual `!RangeInclusive::contains` implementation
   --> src/pyth_http.rs:271:12
    |
271 |         if price < 1.0 || price > 10_000.0 {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: use: `!(1.0..=10_000.0).contains(&price)`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#manual_range_contains

warning: `alpha_hits_10s` is never greater than `255` and has therefore no effect
   --> src/window_tracker.rs:123:36
    |
123 |             alpha_wallet_hits_10s: alpha_hits_10s.min(255),
    |                                    ^^^^^^^^^^^^^^^^^^^^^^^ help: try: `alpha_hits_10s`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unnecessary_min_or_max
    = note: `#[warn(clippy::unnecessary_min_or_max)]` on by default

error[E0432]: unresolved import `tempfile`
  --> src/checkpoint.rs:91:9
   |
91 |     use tempfile::tempdir;
   |         ^^^^^^^^ use of unresolved module or unlinked crate `tempfile`
   |
   = help: if you wanted to use a crate named `tempfile`, use `cargo add tempfile` to add it to your `Cargo.toml`

warning: unused variable: `side_str`
   --> src/main.rs:475:21
    |
475 |                 let side_str = if is_buy { "buy" } else { "sell" };
    |                     ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_side_str`
    |
    = note: requested on the command line with `-W unused-variables`

warning: unused variable: `slot`
   --> src/main.rs:714:47
    |
714 |             PumpEvent::Migrated { mint, pool, slot, block_time, signature } => {
    |                                               ^^^^ help: try ignoring the field: `slot: _`

warning: unused variable: `block_time`
   --> src/main.rs:714:53
    |
714 |             PumpEvent::Migrated { mint, pool, slot, block_time, signature } => {
    |                                                     ^^^^^^^^^^ help: try ignoring the field: `block_time: _`

warning: unused variable: `signature`
   --> src/main.rs:714:65
    |
714 |             PumpEvent::Migrated { mint, pool, slot, block_time, signature } => {
    |                                                                 ^^^^^^^^^ help: try ignoring the field: `signature: _`

warning: unused import: `std::fs`
  --> src/checkpoint.rs:90:9
   |
90 |     use std::fs;
   |         ^^^^^^^

warning: unused variable: `price`
   --> src/main.rs:772:54
    |
772 |     if let Some((_, vol_60s, buyers_60s, start_time, price)) = w60s {
    |                                                      ^^^^^ help: if this is intentional, prefix it with an underscore: `_price`

warning: unused variable: `buyers_60s`
   --> src/main.rs:830:38
    |
830 |             if let Some((_, vol_60s, buyers_60s, _, _)) = w60s {
    |                                      ^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_buyers_60s`

warning: useless use of `format!`
   --> src/main.rs:163:9
    |
163 |         format!("{}", &wallet[..8])
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: consider using `.to_string()`: `(&wallet[..8]).to_string()`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#useless_format
    = note: `#[warn(clippy::useless_format)]` on by default

warning: this function has too many arguments (11/7)
   --> src/main.rs:170:1
    |
170 | / async fn run_unified_collector(
171 | |     checkpoint: &mut Checkpoint,
172 | |     checkpoint_path: &str,
173 | |     endpoint: &str,
...   |
181 | |     window_aggregator: &WindowAggregator,
182 | | ) -> Result<()> {
    | |_______________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments
    = note: `#[warn(clippy::too_many_arguments)]` on by default

warning: this `match` can be collapsed into the outer `if let`
   --> src/main.rs:237:21
    |
237 | /                     match update {
238 | |                         UpdateOneof::Transaction(tx_update) => {
239 | |                             // Extract signature for dedup check
240 | |                             if let Some(transaction) = &tx_update.transaction {
...   |
305 | |                     }
    | |_____________________^
    |
help: the outer pattern can be modified to include the inner pattern
   --> src/main.rs:236:29
    |
236 |                 if let Some(update) = msg.update_oneof {
    |                             ^^^^^^ replace this binding
237 |                     match update {
238 |                         UpdateOneof::Transaction(tx_update) => {
    |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ with this pattern
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_match
    = note: `#[warn(clippy::collapsible_match)]` on by default

warning: this function has too many arguments (11/7)
   --> src/main.rs:321:1
    |
321 | / async fn process_transaction(
322 | |     tx: &SubscribeUpdateTransaction,
323 | |     db: &Arc<Mutex<Database>>,
324 | |     pump_program: &Pubkey,
...   |
332 | |     window_aggregator: &WindowAggregator,
333 | | ) -> Result<()> {
    | |_______________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: accessing first element with `account_keys.get(0)`
   --> src/main.rs:349:21
    |
349 |     let fee_payer = account_keys.get(0).map(|s| s.as_str());
    |                     ^^^^^^^^^^^^^^^^^^^ help: try: `account_keys.first()`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#get_first
    = note: `#[warn(clippy::get_first)]` on by default

warning: this expression creates a reference which is immediately dereferenced by the compiler
   --> src/main.rs:631:25
    |
631 |                         &db,
    |                         ^^^ help: change this to: `db`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrow
    = note: `#[warn(clippy::needless_borrow)]` on by default

warning: this expression creates a reference which is immediately dereferenced by the compiler
   --> src/main.rs:632:25
    |
632 |                         &advisory_sender,
    |                         ^^^^^^^^^^^^^^^^ help: change this to: `advisory_sender`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrow

For more information about this error, try `rustc --explain E0432`.
error: could not compile `data-mining` (lib test) due to 1 previous error; 24 warnings emitted
```

**Total warnings**: 49

### Recommendations

1. **Unused variables in main.rs**: Check lines 772, 830 (price, buyers_60s)
   - If these are placeholders for future features, prefix with `_`
   - If genuinely unused, remove
2. **Pyth modules**: If not using Pyth price feeds, these can be removed
3. **Parser modules**: Verify all parser functions are being called

---

## Execution Service

### Unused Code Warnings

```
warning: unused import: `instruction::Instruction`
  --> src/trading.rs:15:5
   |
15 |     instruction::Instruction,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: requested on the command line with `-W unused-imports`

warning: unused import: `get_associated_token_address`
  --> src/trading.rs:21:5
   |
21 |     get_associated_token_address,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `Counter`
  --> src/metrics.rs:19:5
   |
19 |     Counter, Gauge, Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry,
   |     ^^^^^^^

warning: unused import: `EncodedConfirmedTransactionWithStatusMeta`
  --> src/slippage.rs:18:5
   |
18 |     EncodedConfirmedTransactionWithStatusMeta,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `std::str::FromStr`
  --> src/slippage.rs:20:5
   |
20 | use std::str::FromStr;
   |     ^^^^^^^^^^^^^^^^^

warning: unused import: `warn`
  --> src/performance_log.rs:13:18
   |
13 | use log::{error, warn};
   |                  ^^^^

warning: unused import: `warn`
  --> src/trade_closed.rs:11:17
   |
11 | use log::{info, warn};
   |                 ^^^^

warning: unused import: `error`
 --> src/pump_bonding_curve.rs:4:17
  |
4 | use log::{info, error};
  |                 ^^^^^

warning: unused imports: `Keypair` and `Signer`
 --> src/jito.rs:5:17
  |
5 |     signature::{Keypair, Signer},
  |                 ^^^^^^^  ^^^^^^

warning: unused import: `error`
 --> src/jito.rs:9:30
  |
9 | use log::{info, debug, warn, error};
  |                              ^^^^^

warning: unused import: `anyhow`
 --> src/pump_instructions.rs:1:14
  |
1 | use anyhow::{anyhow, Result};
  |              ^^^^^^

warning: unused import: `sysvar`
 --> src/pump_instructions.rs:6:5
  |
6 |     sysvar,
  |     ^^^^^^

warning: unused import: `std::collections::HashMap`
  --> src/tpu_client.rs:11:5
   |
11 | use std::collections::HashMap;
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^

warning: unused import: `params`
 --> src/data/strategy_loader.rs:2:28
  |
2 | use rusqlite::{Connection, params};
  |                            ^^^^^^

warning: unused imports: `LiveContext`, `LiveStrategy`, `ParsedRules`, `StrategyConfig`, `StrategyStore`, `load_live_strategies`, `pick_strategy`, `strategy_reloader`, and `strategy_store_init`
  --> src/data/mod.rs:4:5
   |
4  |     LiveStrategy,
   |     ^^^^^^^^^^^^
5  |     LiveContext,
   |     ^^^^^^^^^^^
6  |     ParsedRules,
   |     ^^^^^^^^^^^
7  |     StrategyConfig,
   |     ^^^^^^^^^^^^^^
8  |     StrategyStore,
   |     ^^^^^^^^^^^^^
9  |     load_live_strategies,
   |     ^^^^^^^^^^^^^^^^^^^^
10 |     strategy_store_init,
   |     ^^^^^^^^^^^^^^^^^^^
11 |     strategy_reloader,
   |     ^^^^^^^^^^^^^^^^^
12 |     pick_strategy,
   |     ^^^^^^^^^^^^^

warning: use of deprecated module `solana_sdk::system_instruction`: Use `solana_system_interface` crate instead
  --> src/trading.rs:17:5
   |
17 |     system_instruction,
   |     ^^^^^^^^^^^^^^^^^^
   |
   = note: `#[warn(deprecated)]` on by default

warning: use of deprecated module `solana_sdk::system_program`: Use `solana_system_interface::program` instead
 --> src/pump_instructions.rs:5:5
  |
5 |     system_program,
  |     ^^^^^^^^^^^^^^

warning: unused variable: `t_race_start`
   --> src/trading.rs:528:13
    |
528 |         let t_race_start = Instant::now();
    |             ^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_t_race_start`
    |
    = note: requested on the command line with `-W unused-variables`

warning: value assigned to `last_error` is never read
   --> src/trading.rs:641:21
    |
641 |             let mut last_error = None;
    |                     ^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?
    = note: `#[warn(unused_assignments)]` on by default

warning: unused variable: `t_start`
    --> src/trading.rs:1073:13
     |
1073 |         let t_start = std::time::Instant::now();  // Track overall timing
     |             ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_t_start`

warning: unused variable: `ata`
    --> src/trading.rs:1724:13
     |
1724 |         let ata = spl_associated_token_account::get_associated_token_address(
     |             ^^^ help: if this is intentional, prefix it with an underscore: `_ata`

warning: value assigned to `total_launches` is never read
   --> src/grpc_client.rs:247:37
    |
247 | ...                   total_launches += 1;
    |                       ^^^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: variable `our_tx_found` is assigned to, but never used
   --> src/grpc_client.rs:702:17
    |
702 |         let mut our_tx_found = false;
    |                 ^^^^^^^^^^^^
    |
    = note: consider using `_our_tx_found` instead

warning: value assigned to `our_tx_found` is never read
   --> src/grpc_client.rs:738:37
    |
738 | ...                   our_tx_found = true;
    |                       ^^^^^^^^^^^^
    |
    = help: maybe it is overwritten before being read?

warning: unused variable: `estimated_position`
   --> src/trading.rs:511:46
    |
511 |     pub fn get_dynamic_profit_targets(&self, estimated_position: u32) -> (f64, f64, f64) {
    |                                              ^^^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_estimated_position`

warning: unused variable: `protocol_version`
  --> src/advice_bus.rs:44:13
   |
44 |         let protocol_version = buf[1];  // Added - was missing!
   |             ^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_protocol_version`

warning: unused variable: `telegram_clone`
   --> src/main.rs:127:9
    |
127 |     let telegram_clone = telegram.clone();
    |         ^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_telegram_clone`

warning: unused variable: `db_clone`
   --> src/main.rs:128:9
    |
128 |     let db_clone = db.clone();
    |         ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_db_clone`

warning: unused variable: `telemetry_clone`
   --> src/main.rs:130:9
    |
130 |     let telemetry_clone = telemetry.clone();
    |         ^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_telemetry_clone`

warning: unused variable: `trading_mempool`
   --> src/main.rs:596:9
    |
596 |     let trading_mempool = trading.clone();
    |         ^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_trading_mempool`

warning: unused variable: `timestamp_received`
   --> src/main.rs:150:37
    |
150 | ...                   let timestamp_received = telemetry::now_ns();
    |                           ^^^^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_timestamp_received`

warning: unused variable: `build_ms`
   --> src/main.rs:246:42
    |
246 | ...                   let (build_ms, send_ms, total_ms) = match (result.t_build, result.t_send) {
    |                            ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_build_ms`

warning: unused variable: `send_ms`
   --> src/main.rs:246:52
    |
246 | ...                   let (build_ms, send_ms, total_ms) = match (result.t_build, result.t_send) {
    |                                      ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_send_ms`

warning: unused variable: `total_ms`
   --> src/main.rs:246:61
    |
246 | ...                   let (build_ms, send_ms, total_ms) = match (result.t_build, result.t_send) {
    |                                               ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_total_ms`

warning: unused variable: `build_ms`
   --> src/main.rs:439:46
    |
439 | ...                   let (build_ms, send_ms, total_ms) = match (exit_result.t_build, exit_result.t_send) {
    |                            ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_build_ms`

warning: unused variable: `send_ms`
   --> src/main.rs:439:56
    |
439 | ...                   let (build_ms, send_ms, total_ms) = match (exit_result.t_build, exit_result.t_send) {
    |                                      ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_send_ms`

warning: unused variable: `total_ms`
   --> src/main.rs:439:65
    |
439 | ...                   let (build_ms, send_ms, total_ms) = match (exit_result.t_build, exit_result.t_send) {
    |                                               ^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_total_ms`

warning: fields `token_address` and `entry_time` are never read
  --> src/main.rs:38:5
   |
37 | struct ActivePosition {
   |        -------------- fields in this struct
38 |     token_address: String,
   |     ^^^^^^^^^^^^^
39 |     entry_time: std::time::Instant,
   |     ^^^^^^^^^^
   |
   = note: requested on the command line with `-W dead-code`

warning: multiple fields are never read
  --> src/config.rs:11:9
   |
7  | pub struct Config {
   |            ------ fields in this struct
...
11 |     pub grpc_endpoint: String,
   |         ^^^^^^^^^^^^^
...
31 |     pub jito_tip_account: String,
   |         ^^^^^^^^^^^^^^^^
...
42 |     pub telegram_async_queue: usize,
   |         ^^^^^^^^^^^^^^^^^^^^
...
48 |     pub advisor_enabled: bool,
   |         ^^^^^^^^^^^^^^^
49 |     pub advisor_queue_size: usize,
   |         ^^^^^^^^^^^^^^^^^^
50 |     pub advice_only_mode: bool,
   |         ^^^^^^^^^^^^^^^^
...
53 |     pub advice_min_confidence: u8,
   |         ^^^^^^^^^^^^^^^^^^^^^
54 |     pub advice_max_hold_extension_secs: u64,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
55 |     pub advice_max_exit_slippage_bps: u16,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
76 |     pub max_builder_threads: usize,
   |         ^^^^^^^^^^^^^^^^^^^
77 |     pub network_timeout_ms: u64,
   |         ^^^^^^^^^^^^^^^^^^
78 |     pub retry_on_fail: bool,
   |         ^^^^^^^^^^^^^
79 |     pub max_retries: u32,
   |         ^^^^^^^^^^^
80 |     pub price_check_interval: u64,
   |         ^^^^^^^^^^^^^^^^^^^^
   |
   = note: `Config` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: method `send_alert` is never used
  --> src/telegram.rs:46:18
   |
11 | impl TelegramClient {
   | ------------------- method in this implementation
...
46 |     pub async fn send_alert(&self, token: &str, profit: f64, volume: f64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
   |                  ^^^^^^^^^^

warning: struct `LatencyTrace` is never constructed
  --> src/database.rs:10:12
   |
10 | pub struct LatencyTrace {
   |            ^^^^^^^^^^^^
   |
   = note: `LatencyTrace` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: multiple associated items are never used
  --> src/database.rs:27:12
   |
26 | impl LatencyTrace {
   | ----------------- associated items in this implementation
27 |     pub fn new(trace_id: String) -> Self {
   |            ^^^
...
44 |     pub fn mark_decide(&mut self) {
   |            ^^^^^^^^^^^
...
49 |     pub fn mark_build(&mut self) {
   |            ^^^^^^^^^^
...
54 |     pub fn mark_send(&mut self) {
   |            ^^^^^^^^^
...
59 |     pub fn mark_landed(&mut self, slot: u64, tx_index: u32) {
   |            ^^^^^^^^^^^
...
66 |     pub fn mark_confirm(&mut self) {
   |            ^^^^^^^^^^^^
...
71 |     pub fn latency_detect_to_send_us(&self) -> Option<u64> {
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^
...
76 |     pub fn latency_send_to_land_us(&self) -> Option<u64> {
   |            ^^^^^^^^^^^^^^^^^^^^^^^
...
84 |     pub fn latency_detect_to_land_us(&self) -> Option<u64> {
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^
...
89 |     pub fn latency_land_to_confirm_us(&self) -> Option<u64> {
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^
...
97 |     pub fn print_breakdown(&self) {
   |            ^^^^^^^^^^^^^^^

warning: field `client` is never read
   --> src/database.rs:135:5
    |
134 | pub struct Database {
    |            -------- field in this struct
135 |     client: Client,
    |     ^^^^^^

warning: multiple methods are never used
   --> src/database.rs:256:18
    |
138 | impl Database {
    | ------------- methods in this implementation
...
256 |     pub async fn log_trade(
    |                  ^^^^^^^^^
...
330 |     pub async fn update_trace_landing(
    |                  ^^^^^^^^^^^^^^^^^^^^
...
352 |     pub async fn update_trace_confirm(
    |                  ^^^^^^^^^^^^^^^^^^^^
...
396 |     pub async fn insert_execution(
    |                  ^^^^^^^^^^^^^^^^
...
432 |     pub async fn update_execution_exit(
    |                  ^^^^^^^^^^^^^^^^^^^^^
...
495 |     pub async fn mark_execution_failed(
    |                  ^^^^^^^^^^^^^^^^^^^^^
...
513 |     pub async fn get_pnl_stats(&self) -> Result<(f64, f64, i64), Box<dyn std::error::Error + Send + Sync>> {
    |                  ^^^^^^^^^^^^^

warning: multiple fields are never read
   --> src/trading.rs:197:9
    |
196 | pub struct BuyResult {
    |            --------- fields in this struct
197 |     pub trade_id: String,          // UUID for tracking across components
    |         ^^^^^^^^
198 |     pub status: ExecutionStatus,   // Transaction lifecycle status
    |         ^^^^^^
199 |     pub token_address: String,
    |         ^^^^^^^^^^^^^
...
203 |     pub actual_token_amount: Option<f64>,  // Actual tokens received (from tx parsing)
    |         ^^^^^^^^^^^^^^^^^^^
...
206 |     pub estimated_position: u32,   // From mempool (for comparison)
    |         ^^^^^^^^^^^^^^^^^^
207 |     pub mempool_volume: f64,       // SOL volume in mempool at entry (for Tier 3)
    |         ^^^^^^^^^^^^^^
...
210 |     pub trace_id: Option<String>,  // For latency tracking
    |         ^^^^^^^^
211 |     pub slippage_bps: Option<i32>, // Actual slippage in basis points
    |         ^^^^^^^^^^^^
    |
    = note: `BuyResult` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: variants `Confirmed`, `Failed`, and `Timeout` are never constructed
   --> src/trading.rs:223:5
    |
221 | pub enum ExecutionStatus {
    |          --------------- variants in this enum
222 |     Pending,    // Transaction submitted, waiting for confirmation
223 |     Confirmed,  // Transaction confirmed on-chain (success)
    |     ^^^^^^^^^
224 |     Failed,     // Transaction failed (reverted or error)
    |     ^^^^^^
225 |     Timeout,    // Transaction did not confirm within expected time
    |     ^^^^^^^
    |
    = note: `ExecutionStatus` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `trade_id`, `status`, `gross_profit`, `exit_fees`, `tier`, and `submission_path` are never read
   --> src/trading.rs:230:9
    |
229 | pub struct ExitResult {
    |            ---------- fields in this struct
230 |     pub trade_id: String,          // UUID for tracking across components
    |         ^^^^^^^^
231 |     pub status: ExecutionStatus,   // Transaction lifecycle status
    |         ^^^^^^
...
234 |     pub gross_profit: f64,
    |         ^^^^^^^^^^^^
235 |     pub exit_fees: FeeBreakdown,
    |         ^^^^^^^^^
...
238 |     pub tier: String,
    |         ^^^^
...
246 |     pub submission_path: Option<String>,      // How tx was submitted: "TPU", "JITO", "JITO-RACE", "RPC"
    |         ^^^^^^^^^^^^^^^
    |
    = note: `ExitResult` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: multiple methods are never used
    --> src/trading.rs:369:18
     |
265  | impl TradingEngine {
     | ------------------ methods in this implementation
...
369  |     pub async fn get_curve_cache_stats(&self) -> pump_bonding_curve::CacheStats {
     |                  ^^^^^^^^^^^^^^^^^^^^^
...
374  |     pub async fn get_curve_cache_size(&self) -> usize {
     |                  ^^^^^^^^^^^^^^^^^^^^
...
414  |     pub async fn fetch_bonding_curve(&self, token_mint: &solana_sdk::pubkey::Pubkey) 
     |                  ^^^^^^^^^^^^^^^^^^^
...
504  |     pub fn get_fee_tracker(&self) -> PriorityFeeTracker {
     |            ^^^^^^^^^^^^^^^
...
511  |     pub fn get_dynamic_profit_targets(&self, estimated_position: u32) -> (f64, f64, f64) {
     |            ^^^^^^^^^^^^^^^^^^^^^^^^^^
...
838  |     pub async fn buy_with_retry(
     |                  ^^^^^^^^^^^^^^
...
889  |     pub async fn sell_with_retry(
     |                  ^^^^^^^^^^^^^^^
...
941  |     pub async fn resubmit_with_fee_bump(
     |                  ^^^^^^^^^^^^^^^^^^^^^^
...
1033 |     pub fn spawn_resubmit_with_fee_bump(
     |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
1245 |     pub async fn get_current_price(&self, token_address: &str) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
     |                  ^^^^^^^^^^^^^^^^^
...
1262 |     pub async fn calculate_net_profit(&self, buy_result: &BuyResult, current_price: f64) -> f64 {
     |                  ^^^^^^^^^^^^^^^^^^^^
...
1282 |     async fn execute_jito_buy(
     |              ^^^^^^^^^^^^^^^^
...
1677 |     async fn execute_tpu_buy(
     |              ^^^^^^^^^^^^^^^
...
2008 |     pub fn get_balance(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
     |            ^^^^^^^^^^^
...
2013 |     pub fn get_wallet_balance(&self) -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
     |            ^^^^^^^^^^^^^^^^^^
...
2018 |     pub async fn get_recent_blockhash(&self) -> Result<solana_sdk::hash::Hash, Box<dyn std::error::Error + Send + Sync>> {
     |                  ^^^^^^^^^^^^^^^^^^^^
...
2023 |     pub fn get_tpu_client(&self) -> Option<&FastTpuClient> {
     |            ^^^^^^^^^^^^^^
...
2028 |     pub fn get_rpc_client(&self) -> &RpcClient {
     |            ^^^^^^^^^^^^^^
...
2033 |     pub async fn monitor_confirmation(
     |                  ^^^^^^^^^^^^^^^^^^^^
...
2115 |     pub async fn get_actual_transaction_fee(
     |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^
...
2157 |     pub async fn calculate_buy_slippage(
     |                  ^^^^^^^^^^^^^^^^^^^^^^

warning: associated items `SIZE`, `MSG_TYPE`, and `is_sell` are never used
  --> src/advice_bus.rs:32:15
   |
31 | impl TradeDecision {
   | ------------------ associated items in this implementation
32 |     pub const SIZE: usize = 52;
   |               ^^^^
33 |     pub const MSG_TYPE: u8 = 1;
   |               ^^^^^^^^
...
72 |     pub fn is_sell(&self) -> bool {
   |            ^^^^^^^

warning: method `mint_str` is never used
   --> src/advice_bus.rs:480:12
    |
177 | impl Advisory {
    | ------------- method in this implementation
...
480 |     pub fn mint_str(&self) -> String {
    |            ^^^^^^^^

warning: methods `try_recv`, `try_recv_trade_decision`, `drain`, and `disable` are never used
   --> src/advice_bus.rs:620:12
    |
518 | impl AdviceBusListener {
    | ---------------------- methods in this implementation
...
620 |     pub fn try_recv(&self) -> Option<Advisory> {
    |            ^^^^^^^^
...
670 |     pub fn try_recv_trade_decision(&self) -> Option<TradeDecision> {
    |            ^^^^^^^^^^^^^^^^^^^^^^^
...
720 |     pub fn drain(&self, max_per_tick: usize) -> Vec<Advisory> {
    |            ^^^^^
...
734 |     pub fn disable(&mut self) {
    |            ^^^^^^^

warning: method `local_addr` is never used
  --> src/mempool_bus.rs:72:12
   |
28 | impl MempoolBusListener {
   | ----------------------- method in this implementation
...
72 |     pub fn local_addr(&self) -> Result<std::net::SocketAddr, std::io::Error> {
   |            ^^^^^^^^^^

warning: fields `system`, `trading`, `advice_bus`, `errors`, and `monitoring` are never read
  --> src/emoji.rs:10:9
   |
9  | pub struct EmojiMap {
   |            -------- fields in this struct
10 |     pub system: SystemEmojis,
   |         ^^^^^^
11 |     pub trading: TradingEmojis,
   |         ^^^^^^^
12 |     pub advice_bus: AdviceBusEmojis,
   |         ^^^^^^^^^^
13 |     pub errors: ErrorEmojis,
   |         ^^^^^^
14 |     pub monitoring: MonitoringEmojis,
   |         ^^^^^^^^^^
   |
   = note: `EmojiMap` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `startup`, `config`, `database`, `network`, `wallet`, and `shutdown` are never read
  --> src/emoji.rs:19:9
   |
18 | pub struct SystemEmojis {
   |            ------------ fields in this struct
19 |     pub startup: String,
   |         ^^^^^^^
20 |     pub config: String,
   |         ^^^^^^
21 |     pub database: String,
   |         ^^^^^^^^
22 |     pub network: String,
   |         ^^^^^^^
23 |     pub wallet: String,
   |         ^^^^^^
24 |     pub shutdown: String,
   |         ^^^^^^^^
   |
   = note: `SystemEmojis` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: multiple fields are never read
  --> src/emoji.rs:29:9
   |
28 | pub struct TradingEmojis {
   |            ------------- fields in this struct
29 |     pub launch_detected: String,
   |         ^^^^^^^^^^^^^^^
30 |     pub entry_signal: String,
   |         ^^^^^^^^^^^^
31 |     pub position_entered: String,
   |         ^^^^^^^^^^^^^^^^
32 |     pub position_opened: String,
   |         ^^^^^^^^^^^^^^^
33 |     pub exit_triggered: String,
   |         ^^^^^^^^^^^^^^
34 |     pub exit_completed: String,
   |         ^^^^^^^^^^^^^^
35 |     pub strategy_matched: String,
   |         ^^^^^^^^^^^^^^^^
36 |     pub profit_recorded: String,
   |         ^^^^^^^^^^^^^^^
37 |     pub loss_recorded: String,
   |         ^^^^^^^^^^^^^
38 |     pub mempool_check: String,
   |         ^^^^^^^^^^^^^
39 |     pub volume_check: String,
   |         ^^^^^^^^^^^^
40 |     pub buyer_check: String,
   |         ^^^^^^^^^^^
41 |     pub price_check: String,
   |         ^^^^^^^^^^^
   |
   = note: `TradingEmojis` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: multiple fields are never read
  --> src/emoji.rs:46:9
   |
45 | pub struct AdviceBusEmojis {
   |            --------------- fields in this struct
46 |     pub listening: String,
   |         ^^^^^^^^^
47 |     pub advisory_sent: String,
   |         ^^^^^^^^^^^^^
48 |     pub advisory_received: String,
   |         ^^^^^^^^^^^^^^^^^
49 |     pub hold_extended: String,
   |         ^^^^^^^^^^^^^
50 |     pub exit_widened: String,
   |         ^^^^^^^^^^^^
51 |     pub urgent_exit: String,
   |         ^^^^^^^^^^^
52 |     pub advisory_rejected: String,
   |         ^^^^^^^^^^^^^^^^^
53 |     pub advisory_applied: String,
   |         ^^^^^^^^^^^^^^^^
   |
   = note: `AdviceBusEmojis` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `warning`, `error`, `retry`, `success`, `failed`, and `timeout` are never read
  --> src/emoji.rs:58:9
   |
57 | pub struct ErrorEmojis {
   |            ----------- fields in this struct
58 |     pub warning: String,
   |         ^^^^^^^
59 |     pub error: String,
   |         ^^^^^
60 |     pub retry: String,
   |         ^^^^^
61 |     pub success: String,
   |         ^^^^^^^
62 |     pub failed: String,
   |         ^^^^^^
63 |     pub timeout: String,
   |         ^^^^^^^
   |
   = note: `ErrorEmojis` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `heartbeat`, `status`, `metrics`, and `alert` are never read
  --> src/emoji.rs:68:9
   |
67 | pub struct MonitoringEmojis {
   |            ---------------- fields in this struct
68 |     pub heartbeat: String,
   |         ^^^^^^^^^
69 |     pub status: String,
   |         ^^^^^^
70 |     pub metrics: String,
   |         ^^^^^^^
71 |     pub alert: String,
   |         ^^^^^
   |
   = note: `MonitoringEmojis` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: associated functions `load` and `default_toml` are never used
  --> src/emoji.rs:75:8
   |
74 | impl EmojiMap {
   | ------------- associated functions in this implementation
75 |     fn load() -> Self {
   |        ^^^^
...
88 |     fn default_toml() -> String {
   |        ^^^^^^^^^^^^

warning: static `EMOJIS` is never used
   --> src/emoji.rs:141:12
    |
141 | pub static EMOJIS: Lazy<EmojiMap> = Lazy::new(|| EmojiMap::load());
    |            ^^^^^^

warning: multiple fields are never read
  --> src/metrics.rs:39:5
   |
37 | pub struct BrainMetrics {
   |            ------------ fields in this struct
38 |     // Registry for Prometheus
39 |     registry: Registry,
   |     ^^^^^^^^
...
42 |     pub decisions_total: IntCounter,
   |         ^^^^^^^^^^^^^^^
43 |     pub decisions_approved: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^
44 |     pub decisions_rejected: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^
...
47 |     pub copytrade_decisions: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^
48 |     pub newlaunch_decisions: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^
49 |     pub wallet_activity_decisions: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^
...
52 |     pub rejected_low_confidence: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^^
53 |     pub rejected_guardrails: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^
54 |     pub rejected_validation: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^
...
57 |     pub mint_cache_hits: IntCounter,
   |         ^^^^^^^^^^^^^^^
58 |     pub mint_cache_misses: IntCounter,
   |         ^^^^^^^^^^^^^^^^^
59 |     pub wallet_cache_hits: IntCounter,
   |         ^^^^^^^^^^^^^^^^^
60 |     pub wallet_cache_misses: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^
...
63 |     pub guardrail_loss_backoff: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^
64 |     pub guardrail_position_limit: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
65 |     pub guardrail_rate_limit: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^
66 |     pub guardrail_wallet_cooling: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
...
69 |     pub decision_latency: Histogram,
   |         ^^^^^^^^^^^^^^^^
70 |     pub advice_processing_latency: Histogram,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^
...
73 |     pub sol_price_usd: Gauge,
   |         ^^^^^^^^^^^^^
74 |     pub active_positions: IntGauge,
   |         ^^^^^^^^^^^^^^^^
75 |     pub advice_messages_received: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
76 |     pub decision_messages_sent: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^
...
79 |     pub db_query_duration: Histogram,
   |         ^^^^^^^^^^^^^^^^^
80 |     pub db_errors: IntCounter,
   |         ^^^^^^^^^
...
83 |     pub udp_packets_received: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^
84 |     pub udp_packets_sent: IntCounter,
   |         ^^^^^^^^^^^^^^^^
85 |     pub udp_parse_errors: IntCounter,
   |         ^^^^^^^^^^^^^^^^

warning: method `registry` is never used
   --> src/metrics.rs:278:12
    |
88  | impl BrainMetrics {
    | ----------------- method in this implementation
...
278 |     pub fn registry(&self) -> &Registry {
    |            ^^^^^^^^

warning: function `metrics` is never used
   --> src/metrics.rs:284:8
    |
284 | pub fn metrics() -> Arc<BrainMetrics> {
    |        ^^^^^^^

warning: function `start_metrics_server` is never used
   --> src/metrics.rs:296:14
    |
296 | pub async fn start_metrics_server(port: u16) -> anyhow::Result<()> {
    |              ^^^^^^^^^^^^^^^^^^^^

warning: function `metrics_handler` is never used
   --> src/metrics.rs:317:10
    |
317 | async fn metrics_handler() -> Response {
    |          ^^^^^^^^^^^^^^^

warning: function `health_handler` is never used
   --> src/metrics.rs:340:10
    |
340 | async fn health_handler() -> Response {
    |          ^^^^^^^^^^^^^^

warning: function `record_decision_approved` is never used
   --> src/metrics.rs:353:8
    |
353 | pub fn record_decision_approved() {
    |        ^^^^^^^^^^^^^^^^^^^^^^^^

warning: function `record_decision_rejected` is never used
   --> src/metrics.rs:360:8
    |
360 | pub fn record_decision_rejected(reason: RejectionReason) {
    |        ^^^^^^^^^^^^^^^^^^^^^^^^

warning: enum `RejectionReason` is never used
   --> src/metrics.rs:373:10
    |
373 | pub enum RejectionReason {
    |          ^^^^^^^^^^^^^^^

warning: function `record_decision_pathway` is never used
   --> src/metrics.rs:380:8
    |
380 | pub fn record_decision_pathway(pathway: DecisionPathway) {
    |        ^^^^^^^^^^^^^^^^^^^^^^^

warning: enum `DecisionPathway` is never used
   --> src/metrics.rs:390:10
    |
390 | pub enum DecisionPathway {
    |          ^^^^^^^^^^^^^^^

warning: function `record_guardrail_block` is never used
   --> src/metrics.rs:397:8
    |
397 | pub fn record_guardrail_block(guardrail: GuardrailType) {
    |        ^^^^^^^^^^^^^^^^^^^^^^

warning: enum `GuardrailType` is never used
   --> src/metrics.rs:410:10
    |
410 | pub enum GuardrailType {
    |          ^^^^^^^^^^^^^

warning: function `record_cache_access` is never used
   --> src/metrics.rs:418:8
    |
418 | pub fn record_cache_access(cache: CacheType, hit: bool) {
    |        ^^^^^^^^^^^^^^^^^^^

warning: enum `CacheType` is never used
   --> src/metrics.rs:429:10
    |
429 | pub enum CacheType {
    |          ^^^^^^^^^

warning: function `update_sol_price` is never used
   --> src/metrics.rs:435:8
    |
435 | pub fn update_sol_price(price: f32) {
    |        ^^^^^^^^^^^^^^^^

warning: function `update_active_positions` is never used
   --> src/metrics.rs:440:8
    |
440 | pub fn update_active_positions(count: i64) {
    |        ^^^^^^^^^^^^^^^^^^^^^^^

warning: function `record_advice_received` is never used
   --> src/metrics.rs:445:8
    |
445 | pub fn record_advice_received() {
    |        ^^^^^^^^^^^^^^^^^^^^^^

warning: function `record_decision_sent` is never used
   --> src/metrics.rs:452:8
    |
452 | pub fn record_decision_sent() {
    |        ^^^^^^^^^^^^^^^^^^^^

warning: function `record_udp_parse_error` is never used
   --> src/metrics.rs:459:8
    |
459 | pub fn record_udp_parse_error() {
    |        ^^^^^^^^^^^^^^^^^^^^^^

warning: function `record_db_error` is never used
   --> src/metrics.rs:464:8
    |
464 | pub fn record_db_error() {
    |        ^^^^^^^^^^^^^^^

warning: struct `DecisionTimer` is never constructed
   --> src/metrics.rs:469:12
    |
469 | pub struct DecisionTimer {
    |            ^^^^^^^^^^^^^

warning: associated items `start` and `observe` are never used
   --> src/metrics.rs:474:12
    |
473 | impl DecisionTimer {
    | ------------------ associated items in this implementation
474 |     pub fn start() -> Self {
    |            ^^^^^
...
480 |     pub fn observe(self) {
    |            ^^^^^^^

warning: struct `AdviceTimer` is never constructed
   --> src/metrics.rs:487:12
    |
487 | pub struct AdviceTimer {
    |            ^^^^^^^^^^^

warning: associated items `start` and `observe` are never used
   --> src/metrics.rs:492:12
    |
491 | impl AdviceTimer {
    | ---------------- associated items in this implementation
492 |     pub fn start() -> Self {
    |            ^^^^^
...
498 |     pub fn observe(self) {
    |            ^^^^^^^

warning: struct `DbQueryTimer` is never constructed
   --> src/metrics.rs:505:12
    |
505 | pub struct DbQueryTimer {
    |            ^^^^^^^^^^^^

warning: associated items `start` and `observe` are never used
   --> src/metrics.rs:510:12
    |
509 | impl DbQueryTimer {
    | ----------------- associated items in this implementation
510 |     pub fn start() -> Self {
    |            ^^^^^
...
516 |     pub fn observe(self) {
    |            ^^^^^^^

warning: fields `socket`, `brain_addr`, and `enabled` are never read
  --> src/telemetry.rs:41:5
   |
40 | pub struct TelemetrySender {
   |            --------------- fields in this struct
41 |     socket: UdpSocket,
   |     ^^^^^^
42 |     brain_addr: String,
   |     ^^^^^^^^^^
43 |     enabled: bool,
   |     ^^^^^^^

warning: associated items `send`, `buy_success`, `sell_success`, and `execution_failed` are never used
   --> src/telemetry.rs:74:12
    |
46  | impl TelemetrySender {
    | -------------------- associated items in this implementation
...
74  |     pub fn send(&self, telemetry: ExecutionTelemetry) {
    |            ^^^^
...
112 |     pub fn buy_success(
    |            ^^^^^^^^^^^
...
134 |     pub fn sell_success(
    |            ^^^^^^^^^^^^
...
157 |     pub fn execution_failed(
    |            ^^^^^^^^^^^^^^^^

warning: function `parse_actual_tokens_from_buy` is never used
  --> src/slippage.rs:86:14
   |
86 | pub async fn parse_actual_tokens_from_buy(
   |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: function `calculate_buy_slippage` is never used
   --> src/slippage.rs:218:14
    |
218 | pub async fn calculate_buy_slippage(
    |              ^^^^^^^^^^^^^^^^^^^^^^

warning: function `calculate_sell_slippage` is never used
   --> src/slippage.rs:230:14
    |
230 | pub async fn calculate_sell_slippage(
    |              ^^^^^^^^^^^^^^^^^^^^^^^

warning: methods `write_to_file` and `log` are never used
  --> src/performance_log.rs:67:12
   |
65 | impl TradePerformanceLog {
   | ------------------------ methods in this implementation
66 |     /// Log performance data to JSONL file
67 |     pub fn write_to_file(&self, log_path: &str) -> Result<(), Box<dyn std::error::Error>> {
   |            ^^^^^^^^^^^^^
...
87 |     pub fn log(&self, log_path: &str) {
   |            ^^^

warning: struct `PerformanceLogBuilder` is never constructed
   --> src/performance_log.rs:100:12
    |
100 | pub struct PerformanceLogBuilder {
    |            ^^^^^^^^^^^^^^^^^^^^^

warning: multiple associated items are never used
   --> src/performance_log.rs:106:12
    |
104 | impl PerformanceLogBuilder {
    | -------------------------- associated items in this implementation
105 |     /// Create new builder with required fields
106 |     pub fn new(
    |            ^^^
...
155 |     pub fn decision_timestamp(mut self, ts_ns: u64) -> Self {
    |            ^^^^^^^^^^^^^^^^^^
...
168 |     pub fn signature(mut self, sig: String) -> Self {
    |            ^^^^^^^^^
...
173 |     pub fn position_size(mut self, size_usd: f64) -> Self {
    |            ^^^^^^^^^^^^^
...
178 |     pub fn actual_fee(mut self, lamports: u64) -> Self {
    |            ^^^^^^^^^^
...
184 |     pub fn priority_fee(mut self, micro_lamports: u64) -> Self {
    |            ^^^^^^^^^^^^
...
189 |     pub fn compute_units(mut self, cu: u64) -> Self {
    |            ^^^^^^^^^^^^^
...
194 |     pub fn slippage(mut self, expected: f64, actual: f64, slippage_bps: i32) -> Self {
    |            ^^^^^^^^
...
203 |     pub fn pnl(mut self, entry_price: f64, exit_price: f64, pnl_usd: f64) -> Self {
    |            ^^^
...
213 |     pub fn status(mut self, status: String) -> Self {
    |            ^^^^^^
...
218 |     pub fn error(mut self, error_msg: String) -> Self {
    |            ^^^^^
...
223 |     pub fn tier(mut self, tier: String) -> Self {
    |            ^^^^
...
228 |     pub fn jito_bundle(mut self, enabled: bool) -> Self {
    |            ^^^^^^^^^^^
...
233 |     pub fn resubmitted(mut self, resubmitted: bool) -> Self {
    |            ^^^^^^^^^^^
...
238 |     pub fn build(self) -> TradePerformanceLog {
    |            ^^^^^

warning: function `now_ns` is never used
   --> src/performance_log.rs:244:8
    |
244 | pub fn now_ns() -> u64 {
    |        ^^^^^^

warning: field `signature` is never read
  --> src/confirmation_task.rs:20:9
   |
18 | pub struct PendingTx {
   |            --------- field in this struct
19 |     pub mint: [u8; 32],
20 |     pub signature: Signature,
   |         ^^^^^^^^^
   |
   = note: `PendingTx` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `msg_type` and `timestamp_ns` are never read
  --> src/tx_confirmed.rs:14:9
   |
13 | pub struct TxConfirmed {
   |            ----------- fields in this struct
14 |     pub msg_type: u8,
   |         ^^^^^^^^
...
20 |     pub timestamp_ns: u64,
   |         ^^^^^^^^^^^^
   |
   = note: `TxConfirmed` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: method `is_failure` is never used
   --> src/tx_confirmed.rs:107:12
    |
23  | impl TxConfirmed {
    | ---------------- method in this implementation
...
107 |     pub fn is_failure(&self) -> bool {
    |            ^^^^^^^^^^

warning: struct `GrpcClient` is never constructed
  --> src/grpc_client.rs:19:12
   |
19 | pub struct GrpcClient {
   |            ^^^^^^^^^^

warning: struct `LaunchEvent` is never constructed
  --> src/grpc_client.rs:26:12
   |
26 | pub struct LaunchEvent {
   |            ^^^^^^^^^^^
   |
   = note: `LaunchEvent` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: struct `TransactionLandedEvent` is never constructed
  --> src/grpc_client.rs:37:12
   |
37 | pub struct TransactionLandedEvent {
   |            ^^^^^^^^^^^^^^^^^^^^^^
   |
   = note: `TransactionLandedEvent` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: field `max_entries` is never read
  --> src/grpc_client.rs:51:5
   |
48 | pub struct PriorityFeeTracker {
   |            ------------------ field in this struct
...
51 |     max_entries: usize, // Max entries to prevent unbounded growth (default 100)
   |     ^^^^^^^^^^^
   |
   = note: `PriorityFeeTracker` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis

warning: method `add_fee` is never used
  --> src/grpc_client.rs:64:12
   |
54 | impl PriorityFeeTracker {
   | ----------------------- method in this implementation
...
64 |     pub fn add_fee(&self, fee_microlamports: u64) {
   |            ^^^^^^^

warning: struct `VolumeTracker` is never constructed
   --> src/grpc_client.rs:119:8
    |
119 | struct VolumeTracker {
    |        ^^^^^^^^^^^^^

warning: associated items `new`, `add_transaction`, `elapsed_seconds`, and `get_stats` are never used
   --> src/grpc_client.rs:127:8
    |
126 | impl VolumeTracker {
    | ------------------ associated items in this implementation
127 |     fn new(token_address: String) -> Self {
    |        ^^^
...
136 |     fn add_transaction(&mut self, wallet: String, sol_amount: f64) {
    |        ^^^^^^^^^^^^^^^
...
141 |     fn elapsed_seconds(&self) -> u64 {
    |        ^^^^^^^^^^^^^^^
...
145 |     fn get_stats(&self) -> (f64, u32) {
    |        ^^^^^^^^^

warning: multiple associated items are never used
   --> src/grpc_client.rs:151:18
    |
150 | impl GrpcClient {
    | --------------- associated items in this implementation
151 |     pub async fn new(config: &Config) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
    |                  ^^^
...
166 |     pub async fn monitor_launches(&self) -> Result<LaunchEvent, Box<dyn std::error::Error + Send + Sync>> {
    |                  ^^^^^^^^^^^^^^^^
...
301 |     pub async fn track_token_volume(
    |                  ^^^^^^^^^^^^^^^^^^
...
393 |     pub async fn track_token_volume_with_early_exit(
    |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
498 |     fn parse_transaction(&self, tx: &SubscribeUpdateTransaction) -> Option<LaunchEvent> {
    |        ^^^^^^^^^^^^^^^^^
...
572 |     fn parse_buy_transaction(&self, tx: &SubscribeUpdateTransaction) -> Option<(String, f64)> {
    |        ^^^^^^^^^^^^^^^^^^^^^
...
600 |     fn extract_priority_fee(
    |        ^^^^^^^^^^^^^^^^^^^^
...
628 |     fn calculate_initial_metrics(
    |        ^^^^^^^^^^^^^^^^^^^^^^^^^
...
655 |     pub async fn monitor_transaction_landing(
    |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^
...
778 |     fn extract_trace_id_from_tx(
    |        ^^^^^^^^^^^^^^^^^^^^^^^^

warning: fields `real_sol_reserves` and `token_total_supply` are never read
  --> src/pump_bonding_curve.rs:28:9
   |
24 | pub struct BondingCurveState {
   |            ----------------- fields in this struct
...
28 |     pub real_sol_reserves: u64,
   |         ^^^^^^^^^^^^^^^^^
29 |     pub token_total_supply: u64,
   |         ^^^^^^^^^^^^^^^^^^
   |
   = note: `BondingCurveState` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: method `calculate_sell_sol` is never used
   --> src/pump_bonding_curve.rs:130:12
    |
34  | impl BondingCurveState {
    | ---------------------- method in this implementation
...
130 |     pub fn calculate_sell_sol(&self, token_amount: f64, fee_basis_points: u64) -> f64 {
    |            ^^^^^^^^^^^^^^^^^^

warning: field `expirations` is never read
   --> src/pump_bonding_curve.rs:232:9
    |
229 | pub struct CacheStats {
    |            ---------- field in this struct
...
232 |     pub expirations: u64,
    |         ^^^^^^^^^^^
    |
    = note: `CacheStats` has derived impls for the traits `Debug` and `Clone`, but these are intentionally ignored during dead code analysis

warning: method `hit_rate` is never used
   --> src/pump_bonding_curve.rs:236:12
    |
235 | impl CacheStats {
    | --------------- method in this implementation
236 |     pub fn hit_rate(&self) -> f64 {
    |            ^^^^^^^^

warning: methods `get_stats`, `clear`, `prune_expired`, and `size` are never used
   --> src/pump_bonding_curve.rs:328:18
    |
249 | impl BondingCurveCache {
    | ---------------------- methods in this implementation
...
328 |     pub async fn get_stats(&self) -> CacheStats {
    |                  ^^^^^^^^^
...
334 |     pub async fn clear(&self) {
    |                  ^^^^^
...
341 |     pub async fn prune_expired(&self) -> usize {
    |                  ^^^^^^^^^^^^^
...
354 |     pub async fn size(&self) -> usize {
    |                  ^^^^

warning: fields `confirmation_status` and `err` are never read
   --> src/jito.rs:387:9
    |
386 | pub struct BundleStatus {
    |            ------------ fields in this struct
387 |     pub confirmation_status: Option<String>,
    |         ^^^^^^^^^^^^^^^^^^^
388 |     pub err: Option<serde_json::Value>,
    |         ^^^
    |
    = note: `BundleStatus` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: methods `is_confirmed` and `has_error` are never used
   --> src/jito.rs:393:12
    |
392 | impl BundleStatus {
    | ----------------- methods in this implementation
393 |     pub fn is_confirmed(&self) -> bool {
    |            ^^^^^^^^^^^^
...
400 |     pub fn has_error(&self) -> bool {
    |            ^^^^^^^^^

warning: fields `current_slot`, `current_leader`, `next_leader`, and `last_update` are never read
  --> src/tpu_client.rs:19:9
   |
18 | pub struct LeaderScheduleCache {
   |            ------------------- fields in this struct
19 |     pub current_slot: u64,
   |         ^^^^^^^^^^^^
20 |     pub current_leader: Option<Pubkey>,
   |         ^^^^^^^^^^^^^^
21 |     pub next_leader: Option<Pubkey>,
   |         ^^^^^^^^^^^
22 |     pub last_update: std::time::Instant,
   |         ^^^^^^^^^^^
   |
   = note: `LeaderScheduleCache` has derived impls for the traits `Debug` and `Clone`, but these are intentionally ignored during dead code analysis

warning: method `is_stale` is never used
  --> src/tpu_client.rs:35:12
   |
25 | impl LeaderScheduleCache {
   | ------------------------ method in this implementation
...
35 |     pub fn is_stale(&self) -> bool {
   |            ^^^^^^^^

warning: fields `leader_cache` and `rpc_client_cache` are never read
  --> src/tpu_client.rs:45:5
   |
42 | pub struct FastTpuClient {
   |            ------------- fields in this struct
...
45 |     leader_cache: Arc<RwLock<LeaderScheduleCache>>,
   |     ^^^^^^^^^^^^
46 |     // TIER 4: TPU client cache for connection reuse
47 |     rpc_client_cache: Arc<RwLock<Option<Arc<AsyncRpcClient>>>>,
   |     ^^^^^^^^^^^^^^^^

warning: methods `refresh_leader_schedule`, `get_leader_info`, `is_near_slot_boundary`, and `send_and_confirm_transaction` are never used
   --> src/tpu_client.rs:74:18
    |
50  | impl FastTpuClient {
    | ------------------ methods in this implementation
...
74  |     pub async fn refresh_leader_schedule(&self) -> Result<()> {
    |                  ^^^^^^^^^^^^^^^^^^^^^^^
...
119 |     pub async fn get_leader_info(&self) -> (u64, Option<Pubkey>, Option<Pubkey>, bool) {
    |                  ^^^^^^^^^^^^^^^
...
131 |     pub async fn is_near_slot_boundary(&self) -> bool {
    |                  ^^^^^^^^^^^^^^^^^^^^^
...
140 |     pub async fn send_and_confirm_transaction(
    |                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: fields `path`, `reload_secs`, and `min_confidence` are never read
  --> src/data/strategy_loader.rs:10:9
   |
9  | pub struct StrategyConfig {
   |            -------------- fields in this struct
10 |     pub path: String,
   |         ^^^^
11 |     pub reload_secs: u64,
   |         ^^^^^^^^^^^
12 |     pub min_confidence: f64,
   |         ^^^^^^^^^^^^^^
   |
   = note: `StrategyConfig` has derived impls for the traits `Debug` and `Clone`, but these are intentionally ignored during dead code analysis

warning: struct `LiveStrategy` is never constructed
  --> src/data/strategy_loader.rs:27:12
   |
27 | pub struct LiveStrategy {
   |            ^^^^^^^^^^^^
   |
   = note: `LiveStrategy` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: fields `min_volume_sol`, `min_unique_buyers`, `profit_target_usd`, and `max_hold_sec` are never read
  --> src/data/strategy_loader.rs:48:9
   |
47 | pub struct ParsedRules {
   |            ----------- fields in this struct
48 |     pub min_volume_sol: Option<f64>,
   |         ^^^^^^^^^^^^^^
49 |     pub min_unique_buyers: Option<u32>,
   |         ^^^^^^^^^^^^^^^^^
50 |     pub profit_target_usd: Option<f64>,
   |         ^^^^^^^^^^^^^^^^^
51 |     pub max_hold_sec: Option<u64>,
   |         ^^^^^^^^^^^^
   |
   = note: `ParsedRules` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: methods `parse_rules` and `is_valid` are never used
  --> src/data/strategy_loader.rs:56:12
   |
54 | impl LiveStrategy {
   | ----------------- methods in this implementation
55 |     /// Parse entry and exit rules into structured data
56 |     pub fn parse_rules(&self) -> ParsedRules {
   |            ^^^^^^^^^^^
...
98 |     pub fn is_valid(&self) -> bool {
   |            ^^^^^^^^

warning: function `load_live_strategies` is never used
   --> src/data/strategy_loader.rs:109:8
    |
109 | pub fn load_live_strategies(db_path: &str, min_conf: f64) -> Result<Vec<LiveStrategy>> {
    |        ^^^^^^^^^^^^^^^^^^^^

warning: type alias `StrategyStore` is never used
   --> src/data/strategy_loader.rs:173:10
    |
173 | pub type StrategyStore = Arc<RwLock<Vec<LiveStrategy>>>;
    |          ^^^^^^^^^^^^^

warning: function `strategy_store_init` is never used
   --> src/data/strategy_loader.rs:176:8
    |
176 | pub fn strategy_store_init(initial: Vec<LiveStrategy>) -> StrategyStore {
    |        ^^^^^^^^^^^^^^^^^^^

warning: function `strategy_reloader` is never used
   --> src/data/strategy_loader.rs:181:14
    |
181 | pub async fn strategy_reloader(store: StrategyStore, cfg: StrategyConfig) {
    |              ^^^^^^^^^^^^^^^^^

warning: fields `volume_last_5s_sol`, `unique_buyers_last_2s`, `token_age_seconds`, and `price_surge_detected` are never read
   --> src/data/strategy_loader.rs:212:9
    |
211 | pub struct LiveContext {
    |            ----------- fields in this struct
212 |     pub volume_last_5s_sol: f64,
    |         ^^^^^^^^^^^^^^^^^^
213 |     pub unique_buyers_last_2s: u32,
    |         ^^^^^^^^^^^^^^^^^^^^^
214 |     pub token_age_seconds: u64,
    |         ^^^^^^^^^^^^^^^^^
215 |     pub price_surge_detected: bool,
    |         ^^^^^^^^^^^^^^^^^^^^
    |
    = note: `LiveContext` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

warning: function `pick_strategy` is never used
   --> src/data/strategy_loader.rs:219:8
    |
219 | pub fn pick_strategy<'a>(
    |        ^^^^^^^^^^^^^

warning: this function has too many arguments (8/7)
   --> src/database.rs:396:5
    |
396 | /     pub async fn insert_execution(
397 | |         &self,
398 | |         decision_id: &str,
399 | |         mint: &str,
...   |
404 | |         sol_price_usd: f64,
405 | |     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    | |_____________________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments
    = note: `#[warn(clippy::too_many_arguments)]` on by default

warning: this function has too many arguments (9/7)
   --> src/database.rs:432:5
    |
432 | /     pub async fn update_execution_exit(
433 | |         &self,
434 | |         decision_id: &str,
435 | |         close_sig: &str,
...   |
441 | |         sol_price_usd: f64,
442 | |     ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    | |_____________________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: question mark operator is useless here
   --> src/trading.rs:416:9
    |
416 |         Ok(self.curve_cache.get_or_fetch(&self.rpc_client, token_mint).await?)
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: try removing question mark and `Ok()`: `self.curve_cache.get_or_fetch(&self.rpc_client, token_mint).await`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_question_mark
    = note: `#[warn(clippy::needless_question_mark)]` on by default

warning: this function has too many arguments (9/7)
   --> src/trading.rs:617:5
    |
617 | /     pub async fn buy(
618 | |         &self,
619 | |         trade_id: String,          // NEW: UUID for tracking across components
620 | |         token_address: &str,
...   |
626 | |         cached_blockhash: Option<solana_sdk::hash::Hash>,  // NEW: Pre-warmed blockhash
627 | |     ) -> Result<BuyResult, Box<dyn std::error::Error + Send + Sync>> {
    | |____________________________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this function has too many arguments (10/7)
   --> src/trading.rs:838:5
    |
838 | /     pub async fn buy_with_retry(
839 | |         &self,
840 | |         trade_id: String,          // NEW: UUID for tracking across components
841 | |         token_address: &str,
...   |
848 | |         max_attempts: u32,
849 | |     ) -> Result<BuyResult, Box<dyn std::error::Error + Send + Sync>> {
    | |____________________________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this function has too many arguments (8/7)
   --> src/trading.rs:889:5
    |
889 | /     pub async fn sell_with_retry(
890 | |         &self,
891 | |         trade_id: String,          // NEW: UUID for tracking across components
892 | |         token_address: &str,
...   |
897 | |         max_attempts: u32,
898 | |     ) -> Result<ExitResult, Box<dyn std::error::Error + Send + Sync>> {
    | |_____________________________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this function has too many arguments (10/7)
   --> src/trading.rs:941:5
    |
941 | /     pub async fn resubmit_with_fee_bump(
942 | |         &self,
943 | |         original_signature: &str,
944 | |         trade_id: String,          // NEW: UUID for tracking across components
...   |
951 | |         fee_bump_multiplier: f64, // e.g., 1.5 = 50% fee increase
952 | |     ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    | |_________________________________________________________________^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this function has too many arguments (10/7)
    --> src/trading.rs:1033:5
     |
1033 | /     pub fn spawn_resubmit_with_fee_bump(
1034 | |         self: Arc<Self>,
1035 | |         original_signature: String,
1036 | |         trade_id: String,          // NEW: UUID for tracking across components
...    |
1043 | |         fee_bump_multiplier: f64,
1044 | |     ) -> tokio::task::JoinHandle<Result<String, Box<dyn std::error::Error + Send + Sync>>> {
     | |__________________________________________________________________________________________^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this function has too many arguments (8/7)
    --> src/trading.rs:1061:5
     |
1061 | /     pub async fn sell(
1062 | |         &self,
1063 | |         trade_id: String,          // NEW: UUID for tracking across components
1064 | |         token_address: &str,
...    |
1069 | |         widen_exit_slippage_bps: Option<u16>,  // Override slippage if WidenExit is active
1070 | |     ) -> Result<ExitResult, Box<dyn std::error::Error + Send + Sync>> {
     | |_____________________________________________________________________^
     |
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: this `if` has identical blocks
    --> src/trading.rs:1103:91
     |
1103 |               let base_slippage_bps = if tier == "ALPHA_WALLET_EXIT" || tier == "STOP_LOSS" {
     |  ___________________________________________________________________________________________^
1104 | |                 900  // 9% base for emergency exits
1105 | |             } else {
     | |_____________^
     |
note: same as this
    --> src/trading.rs:1105:20
     |
1105 |               } else {
     |  ____________________^
1106 | |                 900  // 9% base for normal exits
1107 | |             };
     | |_____________^
     = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#if_same_then_else
     = note: `#[warn(clippy::if_same_then_else)]` on by default

warning: methods with the following characteristics: (`to_*` and `self` type is `Copy`) usually take `self` by value
   --> src/advice_bus.rs:380:21
    |
380 |     pub fn to_bytes(&self) -> Vec<u8> {
    |                     ^^^^^
    |
    = help: consider choosing a less ambiguous name
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#wrong_self_convention
    = note: `#[warn(clippy::wrong_self_convention)]` on by default

warning: this loop could be written as a `while let` loop
   --> src/advice_bus.rs:533:9
    |
533 | /         loop {
534 | |             match socket.recv(&mut flush_buffer) {
535 | |                 Ok(_) => {
536 | |                     flushed_count += 1;
...   |
543 | |         }
    | |_________^ help: try: `while let Ok(_) = socket.recv(&mut flush_buffer) { .. }`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#while_let_loop
    = note: `#[warn(clippy::while_let_loop)]` on by default

warning: redundant closure
   --> src/emoji.rs:141:47
    |
141 | pub static EMOJIS: Lazy<EmojiMap> = Lazy::new(|| EmojiMap::load());
    |                                               ^^^^^^^^^^^^^^^^^^^ help: replace the closure with the function itself: `EmojiMap::load`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#redundant_closure
    = note: `#[warn(clippy::redundant_closure)]` on by default

warning: methods with the following characteristics: (`to_*` and `self` type is `Copy`) usually take `self` by value
   --> src/execution_confirmation.rs:105:21
    |
105 |     pub fn to_bytes(&self) -> [u8; Self::SIZE] {
    |                     ^^^^^
    |
    = help: consider choosing a less ambiguous name
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#wrong_self_convention

warning: this function has too many arguments (9/7)
   --> src/confirmation_task.rs:185:5
    |
185 | /     fn send_trade_confirmed(
186 | |         &self,
187 | |         mint: &[u8; 32],
188 | |         signature: &Signature,
...   |
194 | |         fast_confirm: bool,
195 | |     ) {
    | |_____^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments

warning: `to_string` applied to a type that implements `Display` in `debug!` args
   --> src/confirmation_task.rs:223:20
    |
223 |                    bs58::encode(mint).into_string()[..12].to_string(),
    |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: use this: `&bs58::encode(mint).into_string()[..12]`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#to_string_in_format_args
    = note: `#[warn(clippy::to_string_in_format_args)]` on by default

warning: `to_string` applied to a type that implements `Display` in `debug!` args
   --> src/confirmation_task.rs:268:20
    |
268 |                    bs58::encode(mint).into_string()[..12].to_string(),
    |                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ help: use this: `&bs58::encode(mint).into_string()[..12]`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#to_string_in_format_args

warning: the borrowed expression implements the required traits
  --> src/tx_confirmed.rs:80:21
   |
80 |         hex::encode(&self.trade_id)
   |                     ^^^^^^^^^^^^^^ help: change this to: `self.trade_id`
   |
   = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#needless_borrows_for_generic_args
   = note: `#[warn(clippy::needless_borrows_for_generic_args)]` on by default

warning: you seem to use `.enumerate()` and immediately discard the index
   --> src/grpc_client.rs:639:34
    |
639 |         for (_i, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
    |                                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unused_enumerate_index
    = note: `#[warn(clippy::unused_enumerate_index)]` on by default
help: remove the `.enumerate()` call
    |
639 -         for (_i, (pre, post)) in pre_balances.iter().zip(post_balances.iter()).enumerate() {
639 +         for (pre, post) in pre_balances.iter().zip(post_balances.iter()) {
    |

warning: this `if let` can be collapsed into the outer `if let`
   --> src/grpc_client.rs:716:25
    |
716 | /                         if let UpdateOneof::Transaction(tx_update) = update {
717 | |                             if let Some(tx) = tx_update.transaction {
718 | |                                 let tx_signature = bs58::encode(&tx.signature).into_string();
719 | |                                 let tx_slot = tx_update.slot;
...   |
760 | |                         }
    | |_________________________^
    |
help: the outer pattern can be modified to include the inner pattern
   --> src/grpc_client.rs:715:33
    |
715 |                     if let Some(update) = msg.update_oneof {
    |                                 ^^^^^^ replace this binding
716 |                         if let UpdateOneof::Transaction(tx_update) = update {
    |                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ with this pattern
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_match
    = note: `#[warn(clippy::collapsible_match)]` on by default

warning: useless conversion to the same type: `anyhow::Error`
   --> src/jito.rs:284:36
    |
284 |                         return Err(e.into());
    |                                    ^^^^^^^^ help: consider removing `.into()`: `e`
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#useless_conversion
    = note: `#[warn(clippy::useless_conversion)]` on by default

warning: this `if let` can be collapsed into the outer `if let`
   --> src/tpu_client.rs:189:17
    |
189 | /                 if let Some(result) = status {
190 | |                     if result.is_ok() {
191 | |                         info!("✅ Transaction confirmed via TPU: {}", signature);
192 | |                         return Ok(signature);
...   |
196 | |                 }
    | |_________________^
    |
help: the outer pattern can be modified to include the inner pattern
   --> src/tpu_client.rs:188:23
    |
188 |             if let Ok(status) = self.rpc_client.get_signature_status(&signature) {
    |                       ^^^^^^ replace this binding
189 |                 if let Some(result) = status {
    |                        ^^^^^^^^^^^^ with this pattern
    = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_match

warning: multiple fields are never read
  --> src/metrics.rs:39:5
   |
37 | pub struct BrainMetrics {
   |            ------------ fields in this struct
38 |     // Registry for Prometheus
39 |     registry: Registry,
   |     ^^^^^^^^
...
69 |     pub decision_latency: Histogram,
   |         ^^^^^^^^^^^^^^^^
70 |     pub advice_processing_latency: Histogram,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^
...
75 |     pub advice_messages_received: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^^^
76 |     pub decision_messages_sent: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^^^
...
79 |     pub db_query_duration: Histogram,
   |         ^^^^^^^^^^^^^^^^^
80 |     pub db_errors: IntCounter,
   |         ^^^^^^^^^
...
83 |     pub udp_packets_received: IntCounter,
   |         ^^^^^^^^^^^^^^^^^^^^
84 |     pub udp_packets_sent: IntCounter,
   |         ^^^^^^^^^^^^^^^^
85 |     pub udp_parse_errors: IntCounter,
   |         ^^^^^^^^^^^^^^^^

warning: variants `Guardrails` and `Validation` are never constructed
   --> src/metrics.rs:375:5
    |
373 | pub enum RejectionReason {
    |          --------------- variants in this enum
374 |     LowConfidence,
375 |     Guardrails,
    |     ^^^^^^^^^^
376 |     Validation,
    |     ^^^^^^^^^^

warning: variants `NewLaunch` and `WalletActivity` are never constructed
   --> src/metrics.rs:392:5
    |
390 | pub enum DecisionPathway {
    |          --------------- variants in this enum
391 |     CopyTrade,
392 |     NewLaunch,
    |     ^^^^^^^^^
393 |     WalletActivity,
    |     ^^^^^^^^^^^^^^

warning: variants `LossBackoff`, `RateLimit`, and `WalletCooling` are never constructed
   --> src/metrics.rs:411:5
    |
410 | pub enum GuardrailType {
    |          ------------- variants in this enum
411 |     LossBackoff,
    |     ^^^^^^^^^^^
412 |     PositionLimit,
413 |     RateLimit,
    |     ^^^^^^^^^
414 |     WalletCooling,
    |     ^^^^^^^^^^^^^

warning: variant `Wallet` is never constructed
   --> src/metrics.rs:431:5
    |
429 | pub enum CacheType {
    |          --------- variant in this enum
430 |     Mint,
431 |     Wallet,
    |     ^^^^^^

warning: multiple methods are never used
   --> src/performance_log.rs:178:12
    |
104 | impl PerformanceLogBuilder {
    | -------------------------- methods in this implementation
...
178 |     pub fn actual_fee(mut self, lamports: u64) -> Self {
    |            ^^^^^^^^^^
...
184 |     pub fn priority_fee(mut self, micro_lamports: u64) -> Self {
    |            ^^^^^^^^^^^^
...
189 |     pub fn compute_units(mut self, cu: u64) -> Self {
    |            ^^^^^^^^^^^^^
...
194 |     pub fn slippage(mut self, expected: f64, actual: f64, slippage_bps: i32) -> Self {
    |            ^^^^^^^^
...
203 |     pub fn pnl(mut self, entry_price: f64, exit_price: f64, pnl_usd: f64) -> Self {
    |            ^^^
...
218 |     pub fn error(mut self, error_msg: String) -> Self {
    |            ^^^^^
...
223 |     pub fn tier(mut self, tier: String) -> Self {
    |            ^^^^
...
228 |     pub fn jito_bundle(mut self, enabled: bool) -> Self {
    |            ^^^^^^^^^^^
...
233 |     pub fn resubmitted(mut self, resubmitted: bool) -> Self {
    |            ^^^^^^^^^^^

warning: multiple fields are never read
  --> src/data/strategy_loader.rs:28:9
   |
27 | pub struct LiveStrategy {
   |            ------------ fields in this struct
28 |     pub id: String,
   |         ^^
29 |     pub strategy_type: String,   // e.g. "pattern_based", "scalp"
   |         ^^^^^^^^^^^^^
...
36 |     pub win_rate: f64,            // win rate from backtest
   |         ^^^^^^^^
37 |     pub avg_profit_usd: f64,      // average profit from backtest
   |         ^^^^^^^^^^^^^^
38 |     pub profit_factor: f64,       // profit factor from backtest
   |         ^^^^^^^^^^^^^
39 |     pub execution_confidence: f64, // confidence score
40 |     pub rank: i64,                // strategy rank (1 = best)
   |         ^^^^
41 |     pub score: f64,               // overall score
   |         ^^^^^
   |
   = note: `LiveStrategy` has derived impls for the traits `Clone` and `Debug`, but these are intentionally ignored during dead code analysis

```

**Total warnings**: 158

### Recommendations

1. **main_failed.rs**: This appears to be an old/failed implementation
   - **RECOMMEND DELETION** if main.rs is working
   - Back up first if unsure
2. **metrics.rs (root level)**: Duplicate of src/metrics.rs?
   - Check if this is used
   - If not, **RECOMMEND DELETION**
3. **Mempool modules**: If mempool watching is not critical, consider removing
4. **Test scripts**: Many test scripts in execution/ - consolidate or archive old ones

---

## Mempool-Watcher Service

### Unused Code Warnings

```
```

**Total warnings**: 0
0

### Recommendations

1. **Unused imports**: Review and remove if genuinely unused
2. **WebSocket modules**: Ensure all WebSocket handling is actively used
3. **Database queries**: Verify all SQLite queries are necessary
4. **Audit logging**: Check if audit feature is fully implemented

---

## Files Identified for Potential Deletion

### High Confidence (Safe to Delete)

1. **execution/src/main_failed.rs** - Old implementation (back up first)
2. **execution/metrics.rs** (if duplicate of src/metrics.rs)

### Medium Confidence (Review First)

1. **Pyth price feed modules** (if not using Pyth):
   - data-mining/src/pyth_http.rs
   - data-mining/src/pyth_subscriber.rs
   - data-mining/src/pyth_subscriber_rpc.rs

2. **Mempool modules** (if not using mempool watching):
   - execution/src/mempool.rs
   - execution/src/mempool_bus.rs

3. **Old test scripts** (check if still relevant):
   - execution/test_*.py files that are duplicates

### Low Confidence (Keep for Now)

1. **Parser/raydium.rs** - May be for future Raydium integration
2. **Backtesting module** - Useful for strategy testing
3. **Test data files** (*.json, *.csv in execution/)

---

## Variables to Review

### Brain

Run: `grep -n "unused variable" brain_unused_code.log`

Action items:
- Add `_` prefix if intentionally unused
- Remove if genuinely not needed

### Data-Mining

Specific variables flagged:
- `price` at line 772 in main.rs
- `buyers_60s` at line 830 in main.rs

Recommendation: Prefix with `_` if these are for future window analysis features

### Execution

Run: `grep -n "unused variable" execution_unused_code.log`

---

## Cleanup Checklist

### Phase 1: Safe Deletions (Do First)

- [ ] Back up main_failed.rs
- [ ] Delete execution/src/main_failed.rs (if main.rs works)
- [ ] Check and delete execution/metrics.rs (if duplicate)
- [ ] Archive old test scripts to tests_archive/ folder

### Phase 2: Review and Decide

- [ ] Review each unused variable warning
- [ ] Add `_` prefix to intentionally unused variables
- [ ] Remove genuinely unused variables
- [ ] Review unused import warnings
- [ ] Remove unused imports

### Phase 3: Optional Cleanup

- [ ] Remove Pyth modules if not using Pyth price feed
- [ ] Remove mempool modules if not using mempool watching
- [ ] Consider removing raydium parser if not trading Raydium

### Phase 4: Verification

- [ ] Run all tests again: `./run_all_tests.sh`
- [ ] Ensure all services compile
- [ ] Verify no functionality broken
- [ ] Update documentation to reflect removed features

---

## How to Apply Recommendations

### 1. Back Up Everything First

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot
tar -czf backup_before_cleanup_$(date +%Y%m%d).tar.gz brain/ data-mining/ execution/
```

### 2. Delete Confirmed Unused Files

```bash
# Example: Delete main_failed.rs
mv execution/src/main_failed.rs execution/src/main_failed.rs.bak
cargo build --release -p execution

# If builds successfully:
rm execution/src/main_failed.rs.bak
```

### 3. Fix Unused Variables

```bash
# Example: Fix unused variable
# Change: let price = ...
# To:     let _price = ...
```

### 4. Remove Unused Imports

Use `cargo fix` to automatically fix simple issues:

```bash
cd brain && cargo fix --allow-dirty
cd data-mining && cargo fix --allow-dirty
cd execution && cargo fix --allow-dirty
```

### 5. Verify Everything Still Works

```bash
./run_all_tests.sh
```

---

## Notes

- **Padding fields**: DO NOT remove `_padding` fields in message structs - these are for UDP packet alignment
- **TODO comments**: Not a problem, but track for future implementation
- **FIXME comments**: Should be addressed eventually
- **Dead code warnings**: May indicate genuinely unused code OR code that will be used in future features

---

## Contact

For questions about what's safe to delete, review:
- files/BRAIN_STRUCTURE.md
- files/DATA_MINING_STRUCTURE.md
- files/EXECUTION_STRUCTURE.md

These documents explain what each file does.

---

**Next Steps**: Review this report, make decisions on each recommendation, and execute cleanup in phases.
