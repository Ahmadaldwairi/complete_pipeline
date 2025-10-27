#!/usr/bin/env python3
"""
Test script to verify executor's mempool bus listener (port 45130)

This simulates mempool-watcher sending hot signals to executor.
Run executor first, then run this script to send test signals.
"""

import socket
import struct
import time
import sys


def serialize_hot_signal(
    mint: str,
    whale_wallet: str,
    amount_sol: float,
    action: str,
    urgency: int,
    timestamp: int,
) -> bytes:
    """
    Serialize HotSignalMessage using simple binary format
    (bincode uses specific Rust serialization, this is simplified)

    For proper testing, would need actual bincode serialization
    or use msgpack as intermediate format.
    """
    # Simple format: all strings as 32-byte fixed
    # Real bincode would use variable length encoding
    mint_bytes = mint.encode("utf-8")[:32].ljust(32, b"\x00")
    whale_bytes = whale_wallet.encode("utf-8")[:32].ljust(32, b"\x00")
    action_bytes = action.encode("utf-8")[:6].ljust(6, b"\x00")

    # Pack: 32 bytes mint + 32 bytes wallet + 8 bytes f64 + 6 bytes action + 1 byte urgency + 8 bytes timestamp
    return (
        mint_bytes
        + whale_bytes
        + struct.pack("<d", amount_sol)  # f64 little-endian
        + action_bytes
        + struct.pack("<B", urgency)  # u8
        + struct.pack("<Q", timestamp)
    )  # u64 little-endian


def send_hot_signal(urgency: int, action: str = "buy", amount: float = 15.5):
    """Send a test hot signal to executor on port 45130"""

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    mint = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"  # Example Pump.fun token
    whale_wallet = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1"
    timestamp = int(time.time())

    # NOTE: This is SIMPLIFIED serialization
    # Real test would need proper bincode format matching Rust
    message = serialize_hot_signal(
        mint=mint,
        whale_wallet=whale_wallet,
        amount_sol=amount,
        action=action,
        urgency=urgency,
        timestamp=timestamp,
    )

    try:
        sock.sendto(message, ("127.0.0.1", 45130))
        print(
            f"✅ Sent {action.upper()} signal: urgency={urgency}, amount={amount:.2f} SOL"
        )
        print(f"   Mint: {mint[:8]}...")
        print(f"   Whale: {whale_wallet[:8]}...")
        return True
    except Exception as e:
        print(f"❌ Failed to send: {e}")
        return False
    finally:
        sock.close()


def main():
    print("🔥 Mempool Hot Signal Test Sender")
    print("=" * 50)
    print("Target: 127.0.0.1:45130 (executor mempool_bus)")
    print()

    if len(sys.argv) < 2:
        print("Usage: python test_mempool_listener.py <urgency>")
        print()
        print("Examples:")
        print("  python test_mempool_listener.py 85    # High urgency")
        print("  python test_mempool_listener.py 65    # Medium urgency")
        print("  python test_mempool_listener.py 40    # Low urgency (ignored)")
        print()
        print("Sending default test (urgency=85)...")
        urgency = 85
    else:
        urgency = int(sys.argv[1])

    print(f"Urgency level: {urgency}")
    if urgency >= 80:
        print("  → HIGH PRIORITY (should trigger immediate action + Telegram)")
    elif urgency >= 60:
        print("  → MEDIUM PRIORITY (should trigger monitoring)")
    else:
        print("  → LOW PRIORITY (should be ignored)")

    print()
    print("⚠️  NOTE: This uses simplified binary serialization")
    print("   Real mempool-watcher would use Rust bincode format")
    print("   Executor may not deserialize correctly - check logs!")
    print()

    # Send test signal
    success = send_hot_signal(urgency=urgency)

    if success:
        print()
        print("✅ Signal sent! Check executor logs for:")
        print("   - '🔥 HOT SIGNAL' message with urgency level")
        print("   - Priority handling (HIGH/MEDIUM/LOW)")
        print("   - Telegram notification (if urgency >= 80)")

    return 0 if success else 1


if __name__ == "__main__":
    sys.exit(main())
