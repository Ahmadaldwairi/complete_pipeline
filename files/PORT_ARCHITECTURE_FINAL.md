# UDP Port Architecture - Final Configuration ✅

## Port Assignments

| Port      | From → To                       | Purpose                                         | Listener                 | Status |
| --------- | ------------------------------- | ----------------------------------------------- | ------------------------ | ------ |
| **45100** | **Data-mining → Brain**         | Token creation, volume, buyer, momentum signals | **Brain listens** ✅     | ✅     |
| **45110** | **Brain → Execution**           | Trade decisions (`Buy`, `Sell`, `Hold`)         | **Execution listens** ✅ | ✅     |
| **45120** | **Data-mining → Brain**         | Sol price updates, token MC refresh             | **Brain listens** ✅     | ✅     |
| **45130** | **Execution → Mempool-watcher** | Transaction watch requests (TxSig, Mint, Type)  | **Mempool listens** ✅   | ✅     |
| **45131** | **Mempool-watcher → Brain**     | Post-confirmation info & volume intelligence    | **Brain listens** ✅     | ✅     |
| **45132** | **Mempool-watcher → Execution** | Confirmation notice for submitted Tx            | **Execution listens** ✅ | ✅     |

## Architecture Diagram

```
             ┌──────────────┐
             │ Data-Mining  │
             └──────┬───────┘
         45100 ↓    │    ↓ 45120
                  Brain
                  │  ↑
           45110  │  │ 45131
                  ↓  │
              Execution
                  │
               45130 ↓
            Mempool Watcher
               ↓ 45132
```

## Design Principles

### 1. **Execution is Reactive**

- Only receives:
  - Trade decisions from Brain (45110)
  - Confirmation notices from Mempool (45132)
- Never makes strategic decisions
- Minimal logic, fast execution

### 2. **Brain Makes All Decisions**

- Receives:
  - Token signals from Data-mining (45100, 45120)
  - Post-confirmation intelligence from Mempool (45131)
- Decides:
  - Entry timing
  - Position sizing
  - Exit strategy
  - Hold vs Exit based on momentum

### 3. **Unidirectional Flow**

- No port overlaps
- Clear message ownership
- Each listener has one purpose

### 4. **No Message Duplication**

- Hot signals (45131) → Brain only
- TxConfirmed (45132) → Execution only
- Brain and Execution get different intel

## Implementation Status

### ✅ Completed

- [x] Data-mining → Brain (45100, 45120)
- [x] Brain → Execution (45110)
- [x] Execution → Mempool (45130)
- [x] Mempool → Execution (45132)
- [x] Removed hot signals from Execution
- [x] Updated Mempool to send hot signals to Brain (45131)
- [x] Added 45131 listener in Brain to receive hot signals
- [x] Brain logic to process hot signals and issue Hold/Exit decisions

### Ready for Testing

- [x] All binaries compiled successfully
- [x] All port listeners configured
- [ ] Test full pipeline with all 4 services running

## Code Changes Summary

### mempool-watcher/src/config.rs

```rust
pub struct UdpConfig {
    pub brain_port: u16,                 // 45120
    pub watch_listen_port: u16,          // 45130
    pub brain_confirmation_port: u16,    // 45131 (hot signals)
    pub executor_confirmed_port: u16,    // 45132 (TxConfirmed)
    pub bind_address: String,
}
```

### mempool-watcher/src/udp_publisher.rs

- Renamed `executor_socket` → `brain_confirmation_socket`
- Hot signals now sent to Brain (45131) instead of Execution
- Method: `send_hot_signal_to_brain()` (was `send_hot_signal_to_executor()`)

### execution/src/main.rs

- Removed entire Mempool Bus Listener (was port 45131)
- Kept only TxConfirmed listener (45132)
- Updated startup message to reflect correct ports

### brain/src/main.rs ✅ COMPLETE

- **Added HotSignalMessage struct** - Deserializes bincode messages from mempool-watcher
- **Added port 45131 UDP listener** - Receives hot signals with urgency/momentum intelligence
- **Smart position logic**:
  - If we have active position + high urgency (≥80) + buy action → **HOLD** (ride momentum)
  - If we have active position + low urgency (<30) + sell action → **EXIT** (protect profits)
  - If no active position → ignore signal (not our token)
- **Added bincode dependency** to Cargo.toml for message deserialization
- Process hot signals to update position strategies
- Issue real-time Hold/Exit commands based on momentum

## Testing Checklist

1. **Data-mining → Brain (45100, 45120)**

   - [ ] Brain receives token signals
   - [ ] Brain receives SOL price updates

2. **Brain → Execution (45110)**

   - [ ] Execution receives trade decisions
   - [ ] Execution executes Buy/Sell commands

3. **Execution → Mempool (45130)**

   - [ ] Mempool receives watch requests
   - [ ] Mempool starts tracking signatures

4. **Mempool → Brain (45131)**

   - [ ] Brain receives hot signals
   - [ ] Brain processes momentum intelligence
   - [ ] Brain issues Hold/Exit decisions

5. **Mempool → Execution (45132)**
   - [ ] Execution receives TxConfirmed
   - [ ] Execution notifies Telegram
   - [ ] Execution updates position state

## Next Steps

1. **✅ COMPLETE - Brain 45131 listener implemented**

   - Added HotSignalMessage struct in brain/src/main.rs
   - UDP listener bound to 127.0.0.1:45131
   - Processes hot signals and checks for active positions
   - Logic to issue Hold/Exit decisions based on momentum

2. **Start all 4 services for live testing**:

   ```bash
   # Terminal 1
   cd data-mining && RUST_LOG=info ./target/release/data-mining

   # Terminal 2
   cd brain && RUST_LOG=info ./target/release/decision_engine

   # Terminal 3
   cd mempool-watcher && RUST_LOG=info ./target/release/mempool-watcher

   # Terminal 4
   cd execution && RUST_LOG=info ./target/release/execution-bot
   ```

3. **Expected startup logs**:

   - Brain: `✅ Hot Signal receiver bound to 127.0.0.1:45131 (mempool intelligence)`
   - Brain: `🔥 Listening for hot signals from mempool-watcher...`
   - Execution: `Listening for TxConfirmed from Mempool on port 45132`
   - Mempool: `📡 Publishing hot signals to Brain on 127.0.0.1:45131`
   - Mempool: `🎧 Listening for signature registration on 127.0.0.1:45130`

4. **Verify message flow**:
   - Watch for hot signals in Brain logs with urgency scores
   - Confirm Brain makes Hold/Exit decisions for active positions
   - Validate no port binding errors

## Port Conflict Resolution History

### Issues Fixed

1. ❌ Both Advice Bus and TxConfirmed tried to bind to 45110

   - ✅ Fixed: Advice Bus on 45110, TxConfirmed on 45132

2. ❌ Both Execution and Mempool tried to listen on 45130

   - ✅ Fixed: Mempool listens on 45130, Execution sends to it

3. ❌ Hot signals went to Execution (45131)

   - ✅ Fixed: Hot signals now go to Brain (45131)

4. ❌ Mempool had `executor_port` for both watch requests and hot signals

   - ✅ Fixed: Separate ports - `watch_listen_port` (45130), `brain_confirmation_port` (45131)

5. ❌ Brain had no listener for port 45131
   - ✅ Fixed: Added HotSignalMessage struct and UDP listener with position-aware logic

---

**Generated**: November 2, 2025  
**Status**: ✅ All implementations complete - Ready for live testing  
**All Binaries Built**: data-mining, brain, mempool-watcher, execution-bot
