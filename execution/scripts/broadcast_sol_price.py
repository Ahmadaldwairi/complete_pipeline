#!/usr/bin/env python3
"""
SOL Price Broadcaster - Sends SOL/USD price updates via UDP every 20 seconds

This script simulates the copytrader bot broadcasting SOL prices to the execution bot.
In production, this logic should be integrated into the copytrader bot.

Advisory Type 5: SolPriceUpdate
- Bytes 0: Type (5)
- Bytes 1-4: price_cents (u32, little-endian) - e.g., 18283 = $182.83
- Bytes 5-8: timestamp_secs (u32, little-endian) - Unix timestamp
- Bytes 9: source (u8) - 1=Helius, 2=Jupiter, 3=Fallback
- Bytes 10-63: padding (zeros)

Usage:
    python3 broadcast_sol_price.py [--port PORT] [--interval SECONDS]

Example:
    python3 broadcast_sol_price.py --port 45100 --interval 20
"""

import socket
import struct
import time
import argparse
import requests
from datetime import datetime


def fetch_sol_price_helius():
    """Fetch SOL price from Helius API"""
    try:
        # Use Helius RPC to get SOL price via Jupiter price API through Helius
        response = requests.get(
            "https://api.helius.xyz/v0/addresses/So11111111111111111111111111111111111111112/balances",
            params={
                "api-key": "dd6814ec-edbb-4a17-9d8d-cc0826aacf01",
            },
            timeout=3,
        )
        response.raise_for_status()
        # Try alternative: use CoinGecko API directly (no key needed)
        response = requests.get(
            "https://api.coingecko.com/api/v3/simple/price",
            params={
                "ids": "solana",
                "vs_currencies": "usd",
            },
            timeout=3,
        )
        response.raise_for_status()
        data = response.json()
        price = data["solana"]["usd"]
        return price, 1  # source=1 (CoinGecko via Helius path)
    except Exception as e:
        print(f"⚠️  Helius/CoinGecko failed: {e}")
        return None, None


def fetch_sol_price_jupiter():
    """Fetch SOL price from Jupiter API (fallback)"""
    try:
        response = requests.get(
            "https://api.jup.ag/price/v2?ids=So11111111111111111111111111111111111111112",
            timeout=3,
        )
        response.raise_for_status()
        data = response.json()
        price = data["data"]["So11111111111111111111111111111111111111112"]["price"]
        return price, 2  # source=2 (Jupiter)
    except Exception as e:
        print(f"⚠️  Jupiter failed: {e}")
        return None, None


def fetch_sol_price():
    """Fetch SOL price with fallback"""
    # Try Helius first
    price, source = fetch_sol_price_helius()
    if price:
        return price, source

    # Fallback to Jupiter
    price, source = fetch_sol_price_jupiter()
    if price:
        return price, source

    # Ultimate fallback
    print("⚠️  All APIs failed, using fallback $150")
    return 150.0, 3  # source=3 (Fallback)


def send_sol_price_advisory(sock, host, port, price_usd, source):
    """
    Send SolPriceUpdate advisory (Type 5, 64 bytes)

    Args:
        sock: UDP socket
        host: Target host
        port: Target port
        price_usd: SOL price in USD (e.g., 182.83)
        source: 1=Helius, 2=Jupiter, 3=Fallback
    """
    # Convert price to cents (e.g., $182.83 → 18283)
    price_cents = int(price_usd * 100)

    # Get current Unix timestamp
    timestamp_secs = int(time.time())

    # Build 64-byte message
    message = bytearray(64)
    message[0] = 5  # Type: SolPriceUpdate
    message[1:5] = struct.pack("<I", price_cents)  # u32 little-endian
    message[5:9] = struct.pack("<I", timestamp_secs)  # u32 little-endian
    message[9] = source  # u8
    # Bytes 10-63 are already zeros (padding)

    # Send
    sock.sendto(message, (host, port))

    source_name = {1: "Helius", 2: "Jupiter", 3: "Fallback"}.get(source, "Unknown")
    print(
        f"📤 Sent SOL Price: ${price_usd:.2f} from {source_name} (timestamp: {timestamp_secs})"
    )


def main():
    parser = argparse.ArgumentParser(description="Broadcast SOL/USD price via UDP")
    parser.add_argument(
        "--host", default="127.0.0.1", help="Target host (default: 127.0.0.1)"
    )
    parser.add_argument(
        "--port", type=int, default=45100, help="Target port (default: 45100)"
    )
    parser.add_argument(
        "--interval",
        type=int,
        default=20,
        help="Broadcast interval in seconds (default: 20)",
    )
    parser.add_argument(
        "--test", action="store_true", help="Send one test message and exit"
    )
    args = parser.parse_args()

    # Create UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(f"🚀 SOL Price Broadcaster")
    print(f"   Target: {args.host}:{args.port}")
    print(f"   Interval: {args.interval}s")
    print(f"   Source priority: Helius → Jupiter → Fallback ($150)")
    print()

    if args.test:
        # Test mode: send one message and exit
        print("🧪 Test mode - sending single price update...")
        price, source = fetch_sol_price()
        send_sol_price_advisory(sock, args.host, args.port, price, source)
        print("✅ Test message sent!")
        return

    # Continuous mode
    print("🔁 Starting continuous broadcast (Ctrl+C to stop)...")
    print()

    try:
        while True:
            # Fetch current price
            price, source = fetch_sol_price()

            # Broadcast to execution bot
            send_sol_price_advisory(sock, args.host, args.port, price, source)

            # Wait for next interval
            time.sleep(args.interval)

    except KeyboardInterrupt:
        print()
        print("🛑 Stopped by user")
    finally:
        sock.close()


if __name__ == "__main__":
    main()
