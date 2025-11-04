#!/usr/bin/env python3
"""
Fix historical price data in trades and windows tables.
The old calculation was wrong: price = (sol/1e9) / tokens (missing decimals)
Correct formula: price = (sol/1e9) / (tokens/1e6) = sol / tokens * 1e3
"""

import sqlite3
import time

DB_PATH = "data-mining/data/collector.db"


def fix_trade_prices():
    """Recalculate and update all trade prices"""
    print("🔧 Fixing trade prices...")
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()

    # Get total trades
    cursor.execute("SELECT COUNT(*) FROM trades")
    total_trades = cursor.fetchone()[0]
    print(f"Total trades to fix: {total_trades:,}")

    # Update prices in batches
    batch_size = 10000
    updated = 0
    start_time = time.time()

    cursor.execute(
        """
        UPDATE trades
        SET price = CAST(amount_sol AS REAL) / (CAST(amount_tokens AS REAL) / 1000000.0)
        WHERE amount_tokens > 0
    """
    )

    conn.commit()
    elapsed = time.time() - start_time
    print(f"✅ Updated {cursor.rowcount:,} trade prices in {elapsed:.2f}s")

    conn.close()


def fix_window_prices():
    """Recalculate all window aggregates with correct prices"""
    print("\n🔧 Recalculating window aggregates...")
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()

    # Get all windows
    cursor.execute("SELECT COUNT(DISTINCT mint) as tokens FROM windows")
    token_count = cursor.fetchone()[0]
    print(f"Tokens with windows: {token_count:,}")

    # Get unique (mint, window_sec) combinations
    cursor.execute(
        """
        SELECT DISTINCT mint, window_sec, start_time, end_time
        FROM windows
        ORDER BY start_time DESC
        LIMIT 10000
    """
    )

    windows = cursor.fetchall()
    print(f"Recalculating {len(windows):,} recent windows...")

    updated = 0
    start_time = time.time()

    for mint, window_sec, start_time_val, end_time_val in windows:
        # Get all trades in this window
        cursor.execute(
            """
            SELECT price, amount_sol
            FROM trades
            WHERE mint = ? AND block_time >= ? AND block_time < ?
            ORDER BY block_time ASC
        """,
            (mint, start_time_val, end_time_val),
        )

        trades = cursor.fetchall()
        if not trades:
            continue

        prices = [t[0] for t in trades]
        amounts = [t[1] for t in trades]

        # Calculate OHLC
        open_price = prices[0]
        close_price = prices[-1]
        high_price = max(prices)
        low_price = min(prices)

        # Calculate VWAP
        total_value = sum(p * a for p, a in zip(prices, amounts))
        total_volume = sum(amounts)
        vwap = total_value / total_volume if total_volume > 0 else 0

        # Calculate volatility
        if len(prices) > 1:
            mean = sum(prices) / len(prices)
            variance = sum((p - mean) ** 2 for p in prices) / len(prices)
            volatility = variance**0.5
        else:
            volatility = 0.0

        # Update window
        cursor.execute(
            """
            UPDATE windows
            SET open = ?, high = ?, low = ?, close = ?, vwap = ?, price_volatility = ?
            WHERE mint = ? AND window_sec = ? AND start_time = ?
        """,
            (
                open_price,
                high_price,
                low_price,
                close_price,
                vwap,
                volatility,
                mint,
                window_sec,
                start_time_val,
            ),
        )

        updated += 1
        if updated % 1000 == 0:
            conn.commit()
            elapsed = time.time() - start_time
            rate = updated / elapsed
            print(
                f"Progress: {updated:,}/{len(windows):,} windows ({rate:.1f} windows/s)"
            )

    conn.commit()
    elapsed = time.time() - start_time
    print(f"✅ Updated {updated:,} windows in {elapsed:.2f}s")

    conn.close()


def verify_fixes():
    """Verify that prices are now correct"""
    print("\n🔍 Verifying fixes...")
    conn = sqlite3.connect(DB_PATH)
    cursor = conn.cursor()

    # Check trade prices
    cursor.execute(
        """
        SELECT 
            COUNT(*) as total,
            COUNT(CASE WHEN price > 0 THEN 1 END) as with_price,
            MIN(price) as min_price,
            MAX(price) as max_price,
            AVG(price) as avg_price
        FROM trades
    """
    )

    total, with_price, min_p, max_p, avg_p = cursor.fetchone()
    print(f"\nTrades:")
    print(f"  Total: {total:,}")
    print(f"  With price > 0: {with_price:,} ({100*with_price/total:.1f}%)")
    print(f"  Price range: {min_p:.12f} to {max_p:.12f}")
    print(f"  Average price: {avg_p:.12f}")

    # Check window prices
    cursor.execute(
        """
        SELECT 
            COUNT(*) as total,
            COUNT(CASE WHEN close > 0 THEN 1 END) as with_price,
            MIN(close) as min_close,
            MAX(close) as max_close,
            AVG(close) as avg_close
        FROM windows
        WHERE vol_sol > 0
    """
    )

    total, with_price, min_c, max_c, avg_c = cursor.fetchone()
    print(f"\nWindows (with volume):")
    print(f"  Total: {total:,}")
    print(f"  With close > 0: {with_price:,} ({100*with_price/total:.1f}%)")
    print(f"  Close range: {min_c:.12f} to {max_c:.12f}")
    print(f"  Average close: {avg_c:.12f}")

    # Sample a recent window
    cursor.execute(
        """
        SELECT mint, window_sec, open, high, low, close, vwap, vol_sol
        FROM windows
        WHERE vol_sol > 0
        ORDER BY start_time DESC
        LIMIT 1
    """
    )

    row = cursor.fetchone()
    if row:
        mint, ws, o, h, l, c, v, vol = row
        print(f"\nSample window ({mint[:12]}..., {ws}s window):")
        print(f"  Open: {o:.12f}")
        print(f"  High: {h:.12f}")
        print(f"  Low:  {l:.12f}")
        print(f"  Close: {c:.12f}")
        print(f"  VWAP: {v:.12f}")
        print(f"  Volume: {vol:.2f} SOL")

        if c > 1e-10:
            print("  ✅ Prices look reasonable!")
        else:
            print("  ❌ Prices still too small")

    conn.close()


if __name__ == "__main__":
    print("=" * 70)
    print("HISTORICAL PRICE FIX SCRIPT")
    print("=" * 70)

    fix_trade_prices()
    fix_window_prices()
    verify_fixes()

    print("\n✅ All fixes complete!")
    print("\nNext steps:")
    print("1. Restart data-mining bot to use new price calculation")
    print("2. Re-run backtest: python3 backtest_all_strategies_30days.py")
