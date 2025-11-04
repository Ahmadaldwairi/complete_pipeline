#!/usr/bin/env python3
"""Quick diagnostic to check why backtest shows $0 P&L"""

import sqlite3

DB_PATH = "data-mining/data/collector.db"

conn = sqlite3.connect(DB_PATH)
conn.row_factory = sqlite3.Row

# Get a recent token with good volume
cursor = conn.cursor()
cursor.execute(
    """
    SELECT t.mint, t.launch_block_time, SUM(w.vol_sol) as vol
    FROM tokens t
    LEFT JOIN windows w ON t.mint = w.mint
    WHERE t.launch_block_time >= strftime('%s', 'now', '-7 days')
    GROUP BY t.mint
    HAVING vol >= 50
    ORDER BY t.launch_block_time DESC
    LIMIT 1
"""
)

token = cursor.fetchone()
if not token:
    print("No tokens found")
    exit(1)

mint = token["mint"]
launch_time = token["launch_block_time"]

print(f"Token: {mint[:12]}...")
print(f"Launch: {launch_time}")
print(f"Volume: {token['vol']:.2f} SOL\n")

# Get price windows
cursor.execute(
    """
    SELECT start_time, close, high, low, vol_sol
    FROM windows
    WHERE mint = ?
      AND start_time BETWEEN ? AND ?
    ORDER BY start_time ASC
    LIMIT 10
""",
    (mint, launch_time, launch_time + 300),
)

prices = cursor.fetchall()

print(f"Price windows found: {len(prices)}\n")

if len(prices) == 0:
    print("❌ NO PRICE DATA - This is why P&L is $0!")
    print("   The backtest can't calculate gains without price windows")
else:
    print("Time  | Close      | High       | Low        | Volume")
    print("-" * 60)
    for p in prices:
        offset = p["start_time"] - launch_time
        print(
            f"{offset:4d}s | {p['close']:.8f} | {p['high']:.8f} | {p['low']:.8f} | {p['vol_sol']:6.2f}"
        )

    # Check if prices vary
    closes = [p["close"] for p in prices if p["close"]]
    if closes:
        min_price = min(closes)
        max_price = max(closes)
        variation = ((max_price - min_price) / min_price * 100) if min_price > 0 else 0

        print(f"\nPrice variation: {variation:.2f}%")

        if variation < 0.1:
            print("⚠️  Prices barely change - this explains low P&L")
        else:
            print("✅ Prices vary significantly")

conn.close()
