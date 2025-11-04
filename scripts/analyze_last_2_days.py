#!/usr/bin/env python3
"""
Analyze last 2 days of data and calculate potential profits
Based on 7-signal scoring system with realistic position sizing
"""

import sqlite3
from datetime import datetime, timedelta
from typing import Dict, List, Tuple, Optional
from collections import defaultdict
import statistics

DB_PATH = "data-mining/data/collector.db"
SOL_PRICE_USD = 200  # Approximate SOL price for USD calculations


class ProfitAnalyzer:
    def __init__(self, db_path: str):
        self.conn = sqlite3.connect(db_path)
        self.conn.row_factory = sqlite3.Row

    def get_recent_tokens(self, days: int = 2) -> List[Tuple]:
        """Get all tokens from last N days"""
        cursor = self.conn.cursor()

        cutoff_time = int((datetime.now() - timedelta(days=days)).timestamp())

        query = """
        SELECT mint, launch_block_time, creator_wallet, initial_liquidity_sol
        FROM tokens
        WHERE launch_block_time >= ?
          AND launch_block_time > 0
        ORDER BY launch_block_time DESC
        """

        cursor.execute(query, (cutoff_time,))
        return cursor.fetchall()

    def get_token_volume_first_5min(self, mint: str, launch_time: int) -> float:
        """Get total volume in first 5 minutes"""
        cursor = self.conn.cursor()

        query = """
        SELECT SUM(vol_sol) as total_vol
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND start_time BETWEEN ? AND ?
        """

        cursor.execute(query, (mint, launch_time, launch_time + 300))
        row = cursor.fetchone()
        return row["total_vol"] if row and row["total_vol"] else 0.0

    def get_price_at_time(
        self, mint: str, launch_time: int, seconds_after: int
    ) -> Optional[float]:
        """Get price at specific time after launch"""
        cursor = self.conn.cursor()

        query = """
        SELECT close
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND start_time <= ?
        ORDER BY start_time DESC
        LIMIT 1
        """

        cursor.execute(query, (mint, launch_time + seconds_after))
        row = cursor.fetchone()
        return row["close"] if row and row["close"] else None

    def get_peak_price(
        self, mint: str, launch_time: int, max_seconds: int = 1800
    ) -> Tuple[float, int]:
        """Get peak price within time window"""
        cursor = self.conn.cursor()

        query = """
        SELECT MAX(high) as peak, start_time
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND start_time BETWEEN ? AND ?
        """

        cursor.execute(query, (mint, launch_time, launch_time + max_seconds))
        row = cursor.fetchone()

        if row and row["peak"]:
            peak_time = row["start_time"] - launch_time
            return row["peak"], peak_time

        return 0.0, 0

    def calculate_simple_score(
        self, mint: str, launch_time: int, eval_time: int = 120
    ) -> Dict:
        """Calculate simplified score (signals available in historical data)"""
        cursor = self.conn.cursor()

        score = {
            "signal_1": 0.0,  # Creator (limited data)
            "signal_2": 0.0,  # Buyer speed
            "signal_3": 0.0,  # Liquidity
            "signal_5": 0.0,  # Concentration
            "signal_6": 0.0,  # Volume accel
            "total": 0.0,
        }

        # Signal 2: Buyer speed (simplified)
        cursor.execute(
            """
            SELECT COUNT(DISTINCT trader) as buyers
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
        """,
            (mint, launch_time, launch_time + 60),
        )

        row = cursor.fetchone()
        if row and row["buyers"]:
            buyers = row["buyers"]
            if buyers >= 10:
                score["signal_2"] = 2.0
            elif buyers >= 7:
                score["signal_2"] = 1.5
            elif buyers >= 5:
                score["signal_2"] = 1.0

        # Signal 5: Concentration
        cursor.execute(
            """
            SELECT trader, SUM(amount_sol) as total
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
            GROUP BY trader
            ORDER BY total DESC
            LIMIT 3
        """,
            (mint, launch_time, launch_time + eval_time),
        )

        top3 = cursor.fetchall()

        if len(top3) >= 3:
            cursor.execute(
                """
                SELECT SUM(amount_sol) as total
                FROM trades
                WHERE mint = ?
                  AND side = 'buy'
                  AND block_time BETWEEN ? AND ?
            """,
                (mint, launch_time, launch_time + eval_time),
            )

            total_row = cursor.fetchone()
            if total_row and total_row["total"] and total_row["total"] > 0:
                top3_sum = sum(row["total"] for row in top3)
                concentration = (top3_sum / total_row["total"]) * 100

                if concentration < 70:
                    score["signal_5"] = 1.0
                elif concentration < 80:
                    score["signal_5"] = 0.5

        # Signal 6: Volume acceleration (simplified)
        if eval_time >= 60:
            cursor.execute(
                """
                SELECT SUM(amount_sol) as vol
                FROM trades
                WHERE mint = ?
                  AND side = 'buy'
                  AND block_time BETWEEN ? AND ?
            """,
                (mint, launch_time + eval_time - 30, launch_time + eval_time),
            )

            recent = cursor.fetchone()

            cursor.execute(
                """
                SELECT SUM(amount_sol) as vol
                FROM trades
                WHERE mint = ?
                  AND side = 'buy'
                  AND block_time BETWEEN ? AND ?
            """,
                (mint, launch_time + eval_time - 60, launch_time + eval_time - 30),
            )

            baseline = cursor.fetchone()

            if (
                recent
                and recent["vol"]
                and recent["vol"] > 0.1
                and baseline
                and baseline["vol"]
                and baseline["vol"] > 0.1
            ):
                accel = recent["vol"] / baseline["vol"]
                if accel >= 2.0:
                    score["signal_6"] = 1.5
                elif accel >= 1.5:
                    score["signal_6"] = 1.0

        score["total"] = sum(score.values()) - score["total"]  # Don't double count
        return score

    def simulate_position(
        self, mint: str, launch_time: int, entry_score: float
    ) -> Dict:
        """Simulate a position with our sizing and exit logic"""

        # Position sizing based on score
        if entry_score >= 9.0:
            position_size_sol = 100.0
            stop_loss = 0.80  # -20%
            profit_target_min = 1.30  # +30%
            profit_target_max = 1.50  # +50%
            max_hold_time = 30
        elif entry_score >= 8.0:
            position_size_sol = 75.0
            stop_loss = 0.85  # -15%
            profit_target_min = 1.50  # +50%
            profit_target_max = 2.00  # +100%
            max_hold_time = 120
        elif entry_score >= 7.0:
            position_size_sol = 50.0
            stop_loss = 0.80  # -20%
            profit_target_min = 1.30  # +30%
            profit_target_max = 1.50  # +50%
            max_hold_time = 30
        else:  # 6.0-6.9
            position_size_sol = 25.0
            stop_loss = 0.80  # -20%
            profit_target_min = 1.30  # +30%
            profit_target_max = 1.50  # +50%
            max_hold_time = 30

        # Get entry price (at 120s after launch)
        entry_price = self.get_price_at_time(mint, launch_time, 120)
        if not entry_price or entry_price <= 0:
            return None

        # Get peak price within hold window
        peak_price, peak_time = self.get_peak_price(mint, launch_time, max_hold_time)

        if peak_price <= 0:
            # No price data - assume stop loss hit
            exit_price = entry_price * stop_loss
            exit_reason = "no_data"
        else:
            # Calculate gain at peak
            gain_ratio = peak_price / entry_price

            if gain_ratio <= stop_loss:
                # Stop loss hit
                exit_price = entry_price * stop_loss
                exit_reason = "stop_loss"
            elif gain_ratio >= profit_target_max:
                # Hit max target
                exit_price = entry_price * profit_target_max
                exit_reason = "max_target"
            elif gain_ratio >= profit_target_min:
                # Hit min target (take profit)
                exit_price = entry_price * profit_target_min
                exit_reason = "min_target"
            else:
                # Time-based exit at peak or hold time
                exit_price = peak_price
                exit_reason = "time_exit"

        # Calculate P&L
        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)
        pnl_usd = pnl_sol * SOL_PRICE_USD

        return {
            "mint": mint[:8],
            "entry_score": entry_score,
            "position_size_sol": position_size_sol,
            "position_size_usd": position_size_sol * SOL_PRICE_USD,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_usd,
            "pnl_pct": ((exit_price - entry_price) / entry_price) * 100,
            "peak_price": peak_price,
            "peak_gain_pct": (
                ((peak_price - entry_price) / entry_price) * 100
                if peak_price > 0
                else 0
            ),
        }

    def analyze_last_days(self, days: int = 2):
        """Full analysis of last N days"""

        print("\n" + "=" * 80)
        print(f"📊 ANALYZING LAST {days} DAYS OF DATA")
        print("=" * 80)

        tokens = self.get_recent_tokens(days)
        print(f"\n📋 Total tokens launched: {len(tokens):,}")

        if not tokens:
            print("\n❌ No tokens found in the specified timeframe")
            return

        # Filter for tokens with sufficient volume
        print("\n🔍 Filtering tokens with ≥10 SOL volume in first 5 minutes...")

        qualified_tokens = []
        for i, token in enumerate(tokens):
            if i > 0 and i % 1000 == 0:
                print(f"   Processed {i:,}/{len(tokens):,} tokens...")

            volume = self.get_token_volume_first_5min(
                token["mint"], token["launch_block_time"]
            )
            if volume >= 10:
                qualified_tokens.append((token, volume))

        print(
            f"\n✅ Qualified tokens (≥10 SOL volume): {len(qualified_tokens):,} ({len(qualified_tokens)/len(tokens)*100:.1f}%)"
        )

        if not qualified_tokens:
            print("\n❌ No qualified tokens found")
            return

        # Calculate scores and simulate positions
        print(f"\n📊 Scoring tokens and simulating positions...")

        results = {
            "total_analyzed": 0,
            "score_6_plus": 0,
            "score_7_plus": 0,
            "score_8_plus": 0,
            "score_9_plus": 0,
            "positions": [],
            "wins": 0,
            "losses": 0,
            "total_pnl_sol": 0.0,
            "total_pnl_usd": 0.0,
            "by_score_bracket": defaultdict(
                lambda: {"count": 0, "pnl": 0.0, "wins": 0}
            ),
        }

        for i, (token, volume) in enumerate(qualified_tokens[:500]):  # Analyze top 500
            if i > 0 and i % 50 == 0:
                print(f"   Analyzed {i}/{min(500, len(qualified_tokens))} tokens...")

            # Calculate score at 120s
            score_data = self.calculate_simple_score(
                token["mint"], token["launch_block_time"], 120
            )

            total_score = score_data["total"]
            results["total_analyzed"] += 1

            # Count by score bracket
            if total_score >= 9.0:
                results["score_9_plus"] += 1
            if total_score >= 8.0:
                results["score_8_plus"] += 1
            if total_score >= 7.0:
                results["score_7_plus"] += 1
            if total_score >= 6.0:
                results["score_6_plus"] += 1

            # Simulate position if score >= 6.0
            if total_score >= 6.0:
                position = self.simulate_position(
                    token["mint"], token["launch_block_time"], total_score
                )

                if position:
                    results["positions"].append(position)

                    if position["pnl_sol"] > 0:
                        results["wins"] += 1
                    else:
                        results["losses"] += 1

                    results["total_pnl_sol"] += position["pnl_sol"]
                    results["total_pnl_usd"] += position["pnl_usd"]

                    # Track by score bracket
                    if total_score >= 9.0:
                        bracket = "9.0+"
                    elif total_score >= 8.0:
                        bracket = "8.0-8.9"
                    elif total_score >= 7.0:
                        bracket = "7.0-7.9"
                    else:
                        bracket = "6.0-6.9"

                    results["by_score_bracket"][bracket]["count"] += 1
                    results["by_score_bracket"][bracket]["pnl"] += position["pnl_usd"]
                    if position["pnl_sol"] > 0:
                        results["by_score_bracket"][bracket]["wins"] += 1

        # Print results
        self.print_results(results, days)

        return results

    def print_results(self, results: Dict, days: int):
        """Print comprehensive results"""

        print("\n" + "=" * 80)
        print("💰 PROFIT SIMULATION RESULTS")
        print("=" * 80)

        print(f"\n📊 SCORING DISTRIBUTION:")
        print(f"   Tokens analyzed: {results['total_analyzed']:,}")
        print(
            f"   Score ≥6.0: {results['score_6_plus']:,} ({results['score_6_plus']/results['total_analyzed']*100:.1f}%)"
        )
        print(
            f"   Score ≥7.0: {results['score_7_plus']:,} ({results['score_7_plus']/results['total_analyzed']*100:.1f}%)"
        )
        print(
            f"   Score ≥8.0: {results['score_8_plus']:,} ({results['score_8_plus']/results['total_analyzed']*100:.1f}%)"
        )
        print(
            f"   Score ≥9.0: {results['score_9_plus']:,} ({results['score_9_plus']/results['total_analyzed']*100:.1f}%)"
        )

        if not results["positions"]:
            print("\n❌ No positions simulated (no tokens scored ≥6.0)")
            return

        total_positions = len(results["positions"])
        win_rate = results["wins"] / total_positions * 100

        print(f"\n📈 TRADING PERFORMANCE:")
        print(f"   Total Positions: {total_positions}")
        print(
            f"   Wins: {results['wins']} ({results['wins']/total_positions*100:.1f}%)"
        )
        print(
            f"   Losses: {results['losses']} ({results['losses']/total_positions*100:.1f}%)"
        )
        print(f"   Win Rate: {win_rate:.1f}%")

        print(f"\n💵 PROFIT & LOSS:")
        print(
            f"   Total P&L: {results['total_pnl_sol']:.2f} SOL (${results['total_pnl_usd']:,.2f})"
        )
        print(
            f"   Avg P&L per trade: {results['total_pnl_sol']/total_positions:.2f} SOL (${results['total_pnl_usd']/total_positions:,.2f})"
        )
        print(
            f"   P&L per day: {results['total_pnl_sol']/days:.2f} SOL (${results['total_pnl_usd']/days:,.2f})"
        )

        # Calculate capital requirements
        total_capital_sol = sum(p["position_size_sol"] for p in results["positions"])
        avg_capital_sol = total_capital_sol / total_positions

        print(f"\n💰 CAPITAL REQUIREMENTS:")
        print(
            f"   Total capital deployed: {total_capital_sol:.0f} SOL (${total_capital_sol * SOL_PRICE_USD:,.0f})"
        )
        print(
            f"   Avg position size: {avg_capital_sol:.1f} SOL (${avg_capital_sol * SOL_PRICE_USD:,.0f})"
        )
        print(f"   ROI: {results['total_pnl_sol']/total_capital_sol*100:.1f}%")

        # Performance by score bracket
        print(f"\n🎯 PERFORMANCE BY SCORE BRACKET:")
        for bracket in ["9.0+", "8.0-8.9", "7.0-7.9", "6.0-6.9"]:
            data = results["by_score_bracket"][bracket]
            if data["count"] > 0:
                bracket_win_rate = data["wins"] / data["count"] * 100
                avg_pnl = data["pnl"] / data["count"]
                print(
                    f"   {bracket:8s}: {data['count']:3d} trades | Win Rate: {bracket_win_rate:5.1f}% | Avg P&L: ${avg_pnl:7.2f} | Total: ${data['pnl']:8.2f}"
                )

        # Top 10 winners
        print(f"\n🏆 TOP 10 WINNING TRADES:")
        top_winners = sorted(
            results["positions"], key=lambda x: x["pnl_usd"], reverse=True
        )[:10]
        for i, pos in enumerate(top_winners, 1):
            print(
                f"   {i:2d}. {pos['mint']} | Score: {pos['entry_score']:.1f} | "
                f"Size: {pos['position_size_sol']:.0f} SOL | "
                f"P&L: ${pos['pnl_usd']:7.2f} ({pos['pnl_pct']:+.1f}%) | "
                f"Peak: {pos['peak_gain_pct']:+.1f}% | "
                f"Exit: {pos['exit_reason']}"
            )

        # Bottom 10 losers
        print(f"\n💸 TOP 10 LOSING TRADES:")
        top_losers = sorted(results["positions"], key=lambda x: x["pnl_usd"])[:10]
        for i, pos in enumerate(top_losers, 1):
            print(
                f"   {i:2d}. {pos['mint']} | Score: {pos['entry_score']:.1f} | "
                f"Size: {pos['position_size_sol']:.0f} SOL | "
                f"P&L: ${pos['pnl_usd']:7.2f} ({pos['pnl_pct']:+.1f}%) | "
                f"Exit: {pos['exit_reason']}"
            )

        # Exit reason breakdown
        exit_reasons = defaultdict(int)
        for pos in results["positions"]:
            exit_reasons[pos["exit_reason"]] += 1

        print(f"\n🚪 EXIT REASON BREAKDOWN:")
        for reason, count in sorted(
            exit_reasons.items(), key=lambda x: x[1], reverse=True
        ):
            pct = count / total_positions * 100
            print(f"   {reason:12s}: {count:3d} ({pct:5.1f}%)")

        print("\n" + "=" * 80)


def main():
    analyzer = ProfitAnalyzer(DB_PATH)
    results = analyzer.analyze_last_days(days=2)

    if results:
        print("\n✅ Analysis complete!")
        print(
            f"\n📈 SUMMARY: Over 2 days, the bot could have made approximately ${results['total_pnl_usd']:,.2f}"
        )
        print(
            f"   with {len(results['positions'])} positions and a {results['wins']/len(results['positions'])*100:.1f}% win rate"
        )


if __name__ == "__main__":
    main()
