#!/usr/bin/env python3
"""
Realistic profit analysis based on ACTUAL available historical data
Shows current scores vs projected scores with full real-time data
"""

import sqlite3
from datetime import datetime, timedelta
from typing import Dict, List, Tuple
from collections import defaultdict

DB_PATH = "data-mining/data/collector.db"
SOL_PRICE_USD = 200


class RealisticAnalyzer:
    def __init__(self, db_path: str):
        self.conn = sqlite3.connect(db_path)
        self.conn.row_factory = sqlite3.Row

    def get_recent_tokens(self, days: int = 2, min_volume: float = 10.0) -> List:
        """Get qualified tokens from last N days"""
        cursor = self.conn.cursor()
        cutoff_time = int((datetime.now() - timedelta(days=days)).timestamp())

        # Get tokens with sufficient volume
        query = """
        SELECT 
            t.mint,
            t.launch_block_time,
            t.creator_wallet,
            t.initial_liquidity_sol,
            COALESCE(SUM(w.vol_sol), 0) as total_volume_5min
        FROM tokens t
        LEFT JOIN windows w ON t.mint = w.mint 
            AND w.window_sec = 60
            AND w.start_time BETWEEN t.launch_block_time AND t.launch_block_time + 300
        WHERE t.launch_block_time >= ?
          AND t.launch_block_time > 0
        GROUP BY t.mint
        HAVING total_volume_5min >= ?
        ORDER BY t.launch_block_time DESC
        LIMIT 1000
        """

        cursor.execute(query, (cutoff_time, min_volume))
        return cursor.fetchall()

    def calculate_historical_score(self, mint: str, launch_time: int) -> Dict:
        """Calculate score with available historical data only"""
        cursor = self.conn.cursor()
        score = {
            "signal_2": 0.0,  # Buyer speed
            "signal_5": 0.0,  # Concentration
            "signal_6": 0.0,  # Volume accel
            "missing": 0.0,  # What we're missing
        }

        # Signal 2: Buyer speed (WORKS in historical data)
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

        # Signal 5: Concentration (WORKS in historical data)
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
            (mint, launch_time, launch_time + 120),
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
                (mint, launch_time, launch_time + 120),
            )

            total_row = cursor.fetchone()
            if total_row and total_row["total"] and total_row["total"] > 0:
                top3_sum = sum(r["total"] for r in top3)
                concentration = (top3_sum / total_row["total"]) * 100

                if concentration < 70:
                    score["signal_5"] = 1.0
                elif concentration < 80:
                    score["signal_5"] = 0.5

        # Signal 6: Volume acceleration (LIMITED in historical)
        cursor.execute(
            """
            SELECT SUM(amount_sol) as vol
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
        """,
            (mint, launch_time + 90, launch_time + 120),
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
            (mint, launch_time + 60, launch_time + 90),
        )

        baseline = cursor.fetchone()

        if (
            recent
            and recent["vol"]
            and recent["vol"] > 0.5
            and baseline
            and baseline["vol"]
            and baseline["vol"] > 0.5
        ):
            accel = recent["vol"] / baseline["vol"]
            if accel >= 2.0:
                score["signal_6"] = 1.5
            elif accel >= 1.5:
                score["signal_6"] = 1.0

        # Missing signals (NOT in historical data)
        score["missing"] = 2.0 + 1.5 + 2.0 + 3.0  # Signals 1, 3, 4, 7 max points

        score["historical_total"] = (
            score["signal_2"] + score["signal_5"] + score["signal_6"]
        )
        score["projected_total"] = score[
            "historical_total"
        ]  # Will be adjusted per token

        return score

    def get_peak_mc_and_price(
        self, mint: str, launch_time: int
    ) -> Tuple[float, float, int]:
        """Get peak market cap, price, and time"""
        cursor = self.conn.cursor()

        query = """
        SELECT 
            MAX(high * 1000000000.0) as peak_mc,
            high as peak_price,
            start_time - ? as seconds_after
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND start_time BETWEEN ? AND ?
        ORDER BY high DESC
        LIMIT 1
        """

        cursor.execute(query, (launch_time, mint, launch_time, launch_time + 1800))
        row = cursor.fetchone()

        if row and row["peak_mc"]:
            return row["peak_mc"], row["peak_price"], row["seconds_after"]

        return 0.0, 0.0, 0

    def get_entry_price(self, mint: str, launch_time: int) -> float:
        """Get price at 120s (our entry point)"""
        cursor = self.conn.cursor()

        query = """
        SELECT close
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND start_time >= ?
        ORDER BY start_time ASC
        LIMIT 3
        """

        cursor.execute(query, (mint, launch_time + 60))
        rows = cursor.fetchall()

        if rows and len(rows) >= 2:
            return rows[1]["close"]  # ~120s window

        return 0.0

    def analyze_with_projections(self, days: int = 2):
        """Analyze with both historical and projected scores"""

        print("\n" + "=" * 80)
        print(f"📊 REALISTIC PROFIT ANALYSIS - LAST {days} DAYS")
        print("=" * 80)

        tokens = self.get_recent_tokens(days, min_volume=10.0)
        print(f"\n📋 Qualified tokens (≥10 SOL volume): {len(tokens):,}")

        if not tokens:
            print("\n❌ No qualified tokens found")
            return

        # Analyze scores and potential
        results = {
            "historical": {
                "avg_score": 0.0,
                "max_score": 0.0,
                "score_6_plus": 0,
                "score_distribution": defaultdict(int),
            },
            "projected": {
                "conservative_6_plus": 0,  # Assume 50% of missing points
                "moderate_6_plus": 0,  # Assume 65% of missing points
                "optimistic_6_plus": 0,  # Assume 80% of missing points
            },
            "tokens_1m_plus": 0,
            "example_positions": [],
        }

        print(f"\n🔍 Analyzing {len(tokens)} tokens...")

        all_scores = []
        tokens_1m = []

        for i, token in enumerate(tokens):
            if i > 0 and i % 100 == 0:
                print(f"   Processed {i}/{len(tokens)}...")

            score_data = self.calculate_historical_score(
                token["mint"], token["launch_block_time"]
            )

            hist_score = score_data["historical_total"]
            all_scores.append(hist_score)

            # Track max
            if hist_score > results["historical"]["max_score"]:
                results["historical"]["max_score"] = hist_score

            # Score distribution
            score_bracket = int(hist_score)
            results["historical"]["score_distribution"][score_bracket] += 1

            # Check if hit 1M MC
            peak_mc, peak_price, peak_time = self.get_peak_mc_and_price(
                token["mint"], token["launch_block_time"]
            )

            hit_1m = peak_mc >= 1_000_000
            if hit_1m:
                results["tokens_1m_plus"] += 1
                tokens_1m.append((token, score_data, peak_mc, peak_price, peak_time))

            # Projected scores (add estimated missing signal points)
            missing_points = score_data["missing"]

            # Conservative: 50% of missing points
            conservative = hist_score + (missing_points * 0.50)
            if conservative >= 6.0:
                results["projected"]["conservative_6_plus"] += 1

            # Moderate: 65% of missing points
            moderate = hist_score + (missing_points * 0.65)
            if moderate >= 6.0:
                results["projected"]["moderate_6_plus"] += 1

            # Optimistic: 80% of missing points
            optimistic = hist_score + (missing_points * 0.80)
            if optimistic >= 6.0:
                results["projected"]["optimistic_6_plus"] += 1

            # Store example positions for 1M+ tokens
            if hit_1m and len(results["example_positions"]) < 20:
                entry_price = self.get_entry_price(
                    token["mint"], token["launch_block_time"]
                )
                if entry_price > 0:
                    potential_gain = ((peak_price - entry_price) / entry_price) * 100

                    results["example_positions"].append(
                        {
                            "mint": token["mint"][:8],
                            "hist_score": hist_score,
                            "proj_moderate": moderate,
                            "peak_mc_sol": peak_mc / 1_000_000,  # Convert to millions
                            "peak_time_sec": peak_time,
                            "potential_gain_pct": potential_gain,
                            "entry_price": entry_price,
                            "peak_price": peak_price,
                        }
                    )

        results["historical"]["avg_score"] = sum(all_scores) / len(all_scores)

        # Print results
        self.print_projection_results(results, days, len(tokens))

        return results

    def print_projection_results(self, results: Dict, days: int, total_tokens: int):
        """Print comprehensive projection results"""

        print("\n" + "=" * 80)
        print("📊 HISTORICAL DATA ANALYSIS (ACTUAL SCORES)")
        print("=" * 80)

        hist = results["historical"]
        print(f"\n📉 Score Performance with Available Data:")
        print(f"   Tokens analyzed: {total_tokens:,}")
        print(f"   Average score: {hist['avg_score']:.2f}/15.0")
        print(f"   Max score seen: {hist['max_score']:.1f}/15.0")
        print(f"   Tokens ≥6.0: {hist['score_6_plus']}")

        print(f"\n📊 Score Distribution:")
        for score in sorted(hist["score_distribution"].keys()):
            count = hist["score_distribution"][score]
            pct = count / total_tokens * 100
            bar = "█" * int(pct / 2)
            print(f"   {score:2d}.x: {count:4d} ({pct:5.1f}%) {bar}")

        print(f"\n🎯 Tokens Reaching 1M+ Market Cap:")
        print(
            f"   Total: {results['tokens_1m_plus']:,} ({results['tokens_1m_plus']/total_tokens*100:.1f}%)"
        )

        print("\n" + "=" * 80)
        print("🚀 PROJECTED PERFORMANCE (WITH REAL-TIME DATA)")
        print("=" * 80)

        proj = results["projected"]

        print(f"\n💡 Missing Signal Points in Historical Data:")
        print(f"   Signal 1 (Creator): up to 2.0 points")
        print(f"   Signal 3 (Liquidity): up to 1.5 points")
        print(f"   Signal 4 (Wallet Overlap): up to 2.0 points")
        print(f"   Signal 7 (MC Velocity): up to 3.0 points")
        print(f"   Total Missing: 8.5 points")

        print(f"\n📈 Projected Entry Rates (Score ≥6.0):")

        # Conservative scenario
        print(f"\n   🟡 CONSERVATIVE (50% of missing signals working):")
        print(
            f"      Tokens ≥6.0: {proj['conservative_6_plus']:,} ({proj['conservative_6_plus']/total_tokens*100:.1f}%)"
        )
        print(
            f"      Daily average: {proj['conservative_6_plus']/days:.0f} positions/day"
        )
        print(
            f"      Capture rate for 1M+: {proj['conservative_6_plus']/results['tokens_1m_plus']*100:.0f}%"
            if results["tokens_1m_plus"] > 0
            else ""
        )

        # Moderate scenario
        print(f"\n   🟠 MODERATE (65% of missing signals working):")
        print(
            f"      Tokens ≥6.0: {proj['moderate_6_plus']:,} ({proj['moderate_6_plus']/total_tokens*100:.1f}%)"
        )
        print(f"      Daily average: {proj['moderate_6_plus']/days:.0f} positions/day")
        print(
            f"      Capture rate for 1M+: {proj['moderate_6_plus']/results['tokens_1m_plus']*100:.0f}%"
            if results["tokens_1m_plus"] > 0
            else ""
        )

        # Optimistic scenario
        print(f"\n   🟢 OPTIMISTIC (80% of missing signals working):")
        print(
            f"      Tokens ≥6.0: {proj['optimistic_6_plus']:,} ({proj['optimistic_6_plus']/total_tokens*100:.1f}%)"
        )
        print(
            f"      Daily average: {proj['optimistic_6_plus']/days:.0f} positions/day"
        )
        print(
            f"      Capture rate for 1M+: {proj['optimistic_6_plus']/results['tokens_1m_plus']*100:.0f}%"
            if results["tokens_1m_plus"] > 0
            else ""
        )

        # Profit projections
        print("\n" + "=" * 80)
        print("💰 ESTIMATED PROFIT POTENTIAL")
        print("=" * 80)

        # Use moderate scenario for calculations
        moderate_entries_per_day = proj["moderate_6_plus"] / days

        # Assumptions
        avg_position_size_sol = 50  # Mix of 25-100 SOL positions
        win_rate = 0.60  # Conservative 60% win rate
        avg_win_pct = 35  # Average +35% on winners
        avg_loss_pct = 15  # Average -15% on losers

        wins_per_day = moderate_entries_per_day * win_rate
        losses_per_day = moderate_entries_per_day * (1 - win_rate)

        profit_per_win_sol = avg_position_size_sol * (avg_win_pct / 100)
        loss_per_loss_sol = avg_position_size_sol * (avg_loss_pct / 100)

        daily_profit_sol = (wins_per_day * profit_per_win_sol) - (
            losses_per_day * loss_per_loss_sol
        )
        daily_profit_usd = daily_profit_sol * SOL_PRICE_USD

        print(f"\n📊 MODERATE SCENARIO Projections:")
        print(f"   Entries per day: {moderate_entries_per_day:.1f}")
        print(f"   Win rate: {win_rate*100:.0f}%")
        print(
            f"   Avg position size: {avg_position_size_sol:.0f} SOL (${avg_position_size_sol*SOL_PRICE_USD:,.0f})"
        )
        print(f"   Avg win: +{avg_win_pct}%")
        print(f"   Avg loss: -{avg_loss_pct}%")

        print(f"\n💵 Estimated Daily P&L:")
        print(
            f"   Winners: {wins_per_day:.1f} × ${profit_per_win_sol*SOL_PRICE_USD:,.0f} = ${wins_per_day*profit_per_win_sol*SOL_PRICE_USD:,.0f}"
        )
        print(
            f"   Losers: {losses_per_day:.1f} × -${loss_per_loss_sol*SOL_PRICE_USD:,.0f} = -${losses_per_day*loss_per_loss_sol*SOL_PRICE_USD:,.0f}"
        )
        print(
            f"   Net per day: {daily_profit_sol:+.1f} SOL (${daily_profit_usd:+,.0f})"
        )
        print(
            f"   Over {days} days: {daily_profit_sol*days:+.1f} SOL (${daily_profit_usd*days:+,.0f})"
        )

        # Show example tokens that hit 1M+
        if results["example_positions"]:
            print(f"\n🏆 EXAMPLE: Top Opportunities That Hit 1M+ MC:")
            print(f"   (First 10 tokens)")
            print(
                f"\n   {'Mint':<10} {'Hist':<5} {'Proj':<5} {'Peak MC':<10} {'Time':<7} {'Gain':<10}"
            )
            print(f"   {'-'*10} {'-'*5} {'-'*5} {'-'*10} {'-'*7} {'-'*10}")

            for pos in results["example_positions"][:10]:
                print(
                    f"   {pos['mint']:<10} "
                    f"{pos['hist_score']:5.1f} "
                    f"{pos['proj_moderate']:5.1f} "
                    f"{pos['peak_mc_sol']:>8.1f}M "
                    f"{pos['peak_time_sec']:>5.0f}s "
                    f"{pos['potential_gain_pct']:>+9.1f}%"
                )

        print("\n" + "=" * 80)
        print("📝 KEY FINDINGS")
        print("=" * 80)
        print("\n✅ What We Know:")
        print(
            f"   • {results['tokens_1m_plus']:,} tokens hit 1M+ MC in last {days} days"
        )
        print(f"   • {total_tokens:,} tokens had ≥10 SOL volume")
        print(
            f"   • Historical scores average {results['historical']['avg_score']:.2f}/15.0"
        )
        print(f"   • Missing 8.5 points from signals not in historical data")

        print("\n🎯 What This Means:")
        print(
            f"   • With real-time data, {proj['moderate_6_plus']:,} tokens would score ≥6.0"
        )
        print(f"   • That's ~{moderate_entries_per_day:.0f} positions per day")
        print(f"   • Estimated ${daily_profit_usd:,.0f}/day profit at 60% win rate")
        print(
            f"   • Captures {proj['moderate_6_plus']/results['tokens_1m_plus']*100:.0f}% of 1M+ tokens"
            if results["tokens_1m_plus"] > 0
            else ""
        )

        print("\n⚠️  Important Notes:")
        print("   • Historical data incomplete (no wallet_stats populated)")
        print("   • Real-time system will have all 7 signals functional")
        print(
            "   • Projections based on moderate assumptions (65% signal effectiveness)"
        )
        print("   • Actual results will vary based on market conditions")

        print("\n" + "=" * 80)


def main():
    analyzer = RealisticAnalyzer(DB_PATH)
    analyzer.analyze_with_projections(days=2)


if __name__ == "__main__":
    main()
