# Mempool Heat Message Fix

## Problem

Brain was receiving messages from mempool-watcher but failing to parse them:

```
[2025-10-30T02:40:30Z WARN] ⚠️ Failed to parse advice message: 33 bytes, type=0
```

## Root Cause

**Message Format Mismatch:**

- **Mempool-watcher** was sending `MempoolHeatMessage` using **bincode serialization** (33 bytes, no type discriminator)
- **Brain** was expecting **AdviceMessage** format with fixed binary layout and type discriminator (10-16)

## Solution

### 1. Added MempoolHeat Message Type to Brain

**File: `brain/src/udp_bus/messages.rs`**

- Added `MempoolHeat = 17` to `AdviceMessageType` enum
- Created `MempoolHeatAdvice` struct (24 bytes fixed layout):
  ```rust
  pub struct MempoolHeatAdvice {
      pub msg_type: u8,           // 17
      pub heat_score: u8,         // 0-100
      pub tx_rate: u16,           // tx/s * 100
      pub whale_activity: u16,    // SOL * 100
      pub bot_density: u16,       // density * 10000
      pub timestamp: u64,         // Unix timestamp
      pub _padding: [u8; 6],
  }
  ```
- Added to `AdviceMessage` enum and parsing logic

### 2. Fixed Mempool-Watcher Message Format

**File: `mempool-watcher/src/udp_publisher.rs`**

- Changed from bincode serialization to **fixed binary layout**
- Message now starts with type discriminator `17`
- Scales floats to fit in u16:
  - `tx_rate`: multiply by 100
  - `whale_activity`: multiply by 100
  - `bot_density`: multiply by 10000

### 3. Updated Brain Receiver

**File: `brain/src/udp_bus/receiver.rs`**

- Added `MempoolHeat` match arm to log heat updates
- Unscales values for display:
  ```rust
  tx_rate = heat.tx_rate as f64 / 100.0
  whale_activity = heat.whale_activity as f64 / 100.0
  bot_density = heat.bot_density as f64 / 10000.0
  ```

### 4. Updated Brain Main Loop

**File: `brain/src/main.rs`**

- Added `MempoolHeat` to advice processing match
- Added to hash function for deduplication
- No immediate action taken (informational only)

## Testing

### Expected Behavior

**Mempool-watcher logs:**

```
📤 Sent heat to Brain: 24 bytes (score: 45)
```

**Brain logs:**

```
🌡️  MempoolHeat: score=45, tx_rate=12.34/s, whale=5.67 SOL, bot=23.4%
```

### No More Errors

Previously:

```
⚠️ Failed to parse advice message: 33 bytes, type=0
```

Now:

- Messages parse successfully
- Heat context available for future decision logic
- Logged at DEBUG level to avoid spam

## Build Status

- ✅ mempool-watcher: `cargo build --release` (13.78s, 21 warnings)
- ✅ brain: `cargo build --release` (3.19s, 116 warnings)
- ✅ execution: `cargo build --release` (6.49s, 120 warnings) - ATA fix applied

## Related Fixes

- **ATA Creation in SELL**: Added missing ATA existence check before SELL transactions to prevent "IllegalOwner" errors
- **Transaction Verification**: 3-second wait + 3 retry attempts + explicit error handling

## Status

🟢 **COMPLETE** - All systems compiled and ready for testing
