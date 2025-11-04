#!/usr/bin/env python3
"""
Backtest the 7-signal scoring system on historical data
Analyzes tokens that reached 1M+ market cap to validate detection rates
"""

import sqlite3
from datetime import datetime
from typing import Dict, List, Tuple, Optional
from collections import defaultdict
import statistics

DB_PATH = "data-mining/data/collector.db"


class ScoringBacktest:
    def __init__(self, db_path: str):
        self.conn = sqlite3.connect(db_path)
        self.conn.row_factory = sqlite3.Row

    def get_tokens_with_high_volume(
        self, min_vol_sol: float = 50, limit: int = 100
    ) -> List[str]:
        """Get tokens that had significant trading volume (proxy for successful launches)"""
        cursor = self.conn.cursor()

        # Get tokens with high volume in first 5 minutes (300 seconds)
        query = """
        SELECT w.mint, SUM(w.vol_sol) as total_vol, t.launch_block_time
        FROM windows w
        JOIN tokens t ON w.mint = t.mint
        WHERE w.window_sec = 60
          AND (w.start_time - t.launch_block_time) <= 300
          AND t.launch_block_time > 0
        GROUP BY w.mint
        HAVING total_vol >= ?
        ORDER BY total_vol DESC
        LIMIT ?
        """

        cursor.execute(query, (min_vol_sol, limit))
        results = [
            (row["mint"], row["total_vol"], row["launch_block_time"])
            for row in cursor.fetchall()
        ]

        print(
            f"\n📊 Found {len(results)} tokens with ≥{min_vol_sol} SOL volume in first 5 minutes"
        )
        if results:
            print(f"   Top token: {results[0][1]:.1f} SOL volume")
            print(f"   Avg volume: {statistics.mean([r[1] for r in results]):.1f} SOL")

        return results

    def get_peak_market_cap(self, mint: str, launch_time: int) -> Tuple[float, int]:
        """Calculate peak market cap within first 30 minutes"""
        cursor = self.conn.cursor()

        # Get highest price in 30 minutes after launch
        query = """
        SELECT MAX(high) as peak_price, start_time
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND (start_time - ?) <= 1800
        """

        cursor.execute(query, (mint, launch_time))
        row = cursor.fetchone()

        if row and row["peak_price"]:
            # Assume 1B token supply for pump.fun tokens
            peak_mc = row["peak_price"] * 1_000_000_000.0
            peak_time = row["start_time"] - launch_time
            return peak_mc, peak_time

        return 0.0, 0

    def calculate_signal_1_creator_reputation(self, mint: str) -> Tuple[float, str]:
        """Signal 1: Creator wallet reputation"""
        cursor = self.conn.cursor()

        # Get creator wallet
        cursor.execute("SELECT creator_wallet FROM tokens WHERE mint = ?", (mint,))
        row = cursor.fetchone()
        if not row:
            return 0.0, "no_creator_data"

        creator = row["creator_wallet"]

        # Get creator stats
        cursor.execute(
            """
            SELECT net_pnl_sol, create_count
            FROM wallet_stats
            WHERE wallet = ?
        """,
            (creator,),
        )

        row = cursor.fetchone()
        if not row:
            return 0.0, "new_creator"

        pnl = row["net_pnl_sol"] or 0.0
        count = row["create_count"] or 0

        if pnl >= 500 and count >= 5:
            return 2.0, f"proven_{pnl:.0f}SOL_{count}tokens"
        elif pnl >= 200 and count >= 3:
            return 1.5, f"good_{pnl:.0f}SOL_{count}tokens"
        elif pnl >= 50:
            return 1.0, f"profitable_{pnl:.0f}SOL"

        return 0.0, f"unprofitable_{pnl:.0f}SOL"

    def calculate_signal_2_buyer_speed(
        self, mint: str, launch_time: int, eval_time: int
    ) -> Tuple[float, str]:
        """Signal 2: Speed of first 10 buyers"""
        cursor = self.conn.cursor()

        # Get first 10 buy trades
        query = """
        SELECT COUNT(DISTINCT trader) as buyer_count, 
               MAX(block_time) - MIN(block_time) as time_span
        FROM trades
        WHERE mint = ?
          AND side = 'buy'
          AND block_time BETWEEN ? AND ?
        LIMIT 10
        """

        cursor.execute(query, (mint, launch_time, launch_time + eval_time))
        row = cursor.fetchone()

        if not row or not row["buyer_count"]:
            return 0.0, "no_buyers"

        count = row["buyer_count"]
        span = row["time_span"] or 0

        if count >= 10 and span <= 30:
            return 2.0, f"10buyers_{span}s"
        elif count >= 10 and span <= 60:
            return 1.5, f"10buyers_{span}s"
        elif count >= 7:
            return 1.0, f"{count}buyers"

        return 0.0, f"{count}buyers"

    def calculate_signal_3_liquidity_ratio(
        self, mint: str, eval_time: int, launch_time: int
    ) -> Tuple[float, str]:
        """Signal 3: Liquidity to market cap ratio"""
        cursor = self.conn.cursor()

        # Get initial liquidity
        cursor.execute(
            "SELECT initial_liquidity_sol FROM tokens WHERE mint = ?", (mint,)
        )
        row = cursor.fetchone()

        if not row or not row["initial_liquidity_sol"]:
            return 0.0, "no_liquidity_data"

        liquidity = row["initial_liquidity_sol"]

        # Get price at eval_time
        cursor.execute(
            """
            SELECT close
            FROM windows
            WHERE mint = ?
              AND window_sec = 60
              AND start_time <= ?
            ORDER BY start_time DESC
            LIMIT 1
        """,
            (mint, launch_time + eval_time),
        )

        row = cursor.fetchone()
        if not row or not row["close"]:
            return 0.0, "no_price_data"

        price = row["close"]
        estimated_mc = price * 1_000_000_000.0

        if estimated_mc == 0:
            return 0.0, "zero_mc"

        ratio = liquidity / estimated_mc

        if ratio < 0.03:
            return 1.5, f"ratio_{ratio*100:.1f}%"
        elif ratio < 0.05:
            return 1.0, f"ratio_{ratio*100:.1f}%"

        return 0.0, f"ratio_{ratio*100:.1f}%_thin"

    def calculate_signal_4_wallet_overlap(
        self, mint: str, launch_time: int, eval_time: int
    ) -> Tuple[float, str]:
        """Signal 4: Proven winner wallet overlap"""
        cursor = self.conn.cursor()

        # Get profitable wallets
        cursor.execute(
            """
            SELECT wallet
            FROM wallet_stats
            WHERE net_pnl_sol >= 100
              AND win_rate >= 0.5
              AND total_trades >= 5
            ORDER BY profit_score DESC
            LIMIT 100
        """
        )

        profitable_wallets = {row["wallet"] for row in cursor.fetchall()}

        if not profitable_wallets:
            return 0.0, "no_proven_wallets"

        # Get buyers within eval window
        cursor.execute(
            """
            SELECT DISTINCT trader
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
        """,
            (mint, launch_time, launch_time + eval_time),
        )

        buyers = {row["trader"] for row in cursor.fetchall()}

        overlap = len(buyers.intersection(profitable_wallets))

        if overlap >= 3:
            return 2.0, f"{overlap}_winners"
        elif overlap == 2:
            return 1.5, f"{overlap}_winners"
        elif overlap == 1:
            return 1.0, f"{overlap}_winner"

        return 0.0, "no_overlap"

    def calculate_signal_5_buy_concentration(
        self, mint: str, launch_time: int, eval_time: int
    ) -> Tuple[float, str]:
        """Signal 5: Buy concentration (lower is better)"""
        cursor = self.conn.cursor()

        # Get buy trades
        cursor.execute(
            """
            SELECT trader, SUM(amount_sol) as total_sol
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
            GROUP BY trader
            ORDER BY total_sol DESC
        """,
            (mint, launch_time, launch_time + eval_time),
        )

        trades = cursor.fetchall()

        if len(trades) <= 2:
            return 0.0, f"{len(trades)}buyers_flagged"

        total_volume = sum(row["total_sol"] for row in trades)
        if total_volume == 0:
            return 0.0, "no_volume"

        top3_volume = sum(row["total_sol"] for row in trades[:3])
        concentration = (top3_volume / total_volume) * 100

        if concentration < 70:
            return 1.0, f"{concentration:.1f}%_healthy"
        elif concentration < 80:
            return 0.5, f"{concentration:.1f}%_moderate"

        return 0.0, f"{concentration:.1f}%_high_risk"

    def calculate_signal_6_volume_acceleration(
        self, mint: str, launch_time: int, eval_time: int
    ) -> Tuple[float, str]:
        """Signal 6: Volume acceleration"""
        cursor = self.conn.cursor()

        if eval_time < 60:
            return 0.0, "insufficient_time"

        # Get recent volume (last 30s)
        cursor.execute(
            """
            SELECT SUM(amount_sol) as volume
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
        """,
            (mint, launch_time + eval_time - 30, launch_time + eval_time),
        )

        row = cursor.fetchone()
        recent_vol = row["volume"] or 0.0

        # Get baseline volume (30-60s ago)
        cursor.execute(
            """
            SELECT SUM(amount_sol) as volume
            FROM trades
            WHERE mint = ?
              AND side = 'buy'
              AND block_time BETWEEN ? AND ?
        """,
            (mint, launch_time + eval_time - 60, launch_time + eval_time - 30),
        )

        row = cursor.fetchone()
        baseline_vol = row["volume"] or 0.0

        if baseline_vol < 0.1:
            return 0.0, "no_baseline"

        acceleration = recent_vol / baseline_vol

        if acceleration >= 2.0:
            return 1.5, f"{acceleration:.2f}X_explosive"
        elif acceleration >= 1.5:
            return 1.0, f"{acceleration:.2f}X_strong"

        return 0.0, f"{acceleration:.2f}X_low"

    def calculate_signal_7_mc_velocity(
        self, mint: str, launch_time: int, eval_time: int
    ) -> Tuple[float, str]:
        """Signal 7: Market cap velocity"""
        cursor = self.conn.cursor()

        if eval_time < 30:
            return 0.0, "insufficient_time"

        # Get current price
        cursor.execute(
            """
            SELECT close
            FROM windows
            WHERE mint = ?
              AND window_sec = 60
              AND start_time <= ?
            ORDER BY start_time DESC
            LIMIT 1
        """,
            (mint, launch_time + eval_time),
        )

        row = cursor.fetchone()
        if not row or not row["close"]:
            return 0.0, "no_current_price"

        current_price = row["close"]
        current_mc = current_price * 1_000_000_000.0

        # Get price 30s ago
        cursor.execute(
            """
            SELECT close
            FROM windows
            WHERE mint = ?
              AND window_sec = 60
              AND start_time <= ?
            ORDER BY start_time DESC
            LIMIT 1
        """,
            (mint, launch_time + eval_time - 30),
        )

        row = cursor.fetchone()
        if not row or not row["close"]:
            return 0.0, "no_baseline_price"

        baseline_price = row["close"]
        baseline_mc = baseline_price * 1_000_000_000.0

        # Calculate velocity (SOL/min)
        mc_change = current_mc - baseline_mc
        velocity = (mc_change / 30.0) * 60.0  # Convert to per-minute

        if velocity >= 1000:
            return 3.0, f"{velocity:.0f}SOL/min_explosive"
        elif velocity >= 500:
            return 2.0, f"{velocity:.0f}SOL/min_strong"
        elif velocity >= 200:
            return 1.0, f"{velocity:.0f}SOL/min_moderate"

        return 0.0, f"{velocity:.0f}SOL/min_low"

    def score_token_at_time(self, mint: str, launch_time: int, eval_time: int) -> Dict:
        """Calculate full 7-signal score at specific time"""

        s1_score, s1_detail = self.calculate_signal_1_creator_reputation(mint)
        s2_score, s2_detail = self.calculate_signal_2_buyer_speed(
            mint, launch_time, eval_time
        )
        s3_score, s3_detail = self.calculate_signal_3_liquidity_ratio(
            mint, eval_time, launch_time
        )
        s4_score, s4_detail = self.calculate_signal_4_wallet_overlap(
            mint, launch_time, eval_time
        )
        s5_score, s5_detail = self.calculate_signal_5_buy_concentration(
            mint, launch_time, eval_time
        )
        s6_score, s6_detail = self.calculate_signal_6_volume_acceleration(
            mint, launch_time, eval_time
        )
        s7_score, s7_detail = self.calculate_signal_7_mc_velocity(
            mint, launch_time, eval_time
        )

        total_score = (
            s1_score + s2_score + s3_score + s4_score + s5_score + s6_score + s7_score
        )

        return {
            "total_score": total_score,
            "signal_1": {"score": s1_score, "detail": s1_detail},
            "signal_2": {"score": s2_score, "detail": s2_detail},
            "signal_3": {"score": s3_score, "detail": s3_detail},
            "signal_4": {"score": s4_score, "detail": s4_detail},
            "signal_5": {"score": s5_score, "detail": s5_detail},
            "signal_6": {"score": s6_score, "detail": s6_detail},
            "signal_7": {"score": s7_score, "detail": s7_detail},
        }

    def run_backtest(self, min_volume: float = 50, sample_size: int = 100):
        """Run full backtest on historical tokens"""

        print("\n" + "=" * 80)
        print("🔬 BACKTESTING 7-SIGNAL SCORING SYSTEM")
        print("=" * 80)

        # Get tokens to analyze
        tokens = self.get_tokens_with_high_volume(min_volume, sample_size)

        if not tokens:
            print("\n❌ No tokens found with sufficient volume")
            return

        # Evaluation timestamps (seconds after launch)
        eval_times = [30, 60, 120, 300]

        results = {
            "tokens_analyzed": len(tokens),
            "reached_1m": 0,
            "by_eval_time": {
                t: {
                    "detected": 0,
                    "scores": [],
                    "signal_contributions": defaultdict(list),
                }
                for t in eval_times
            },
            "peak_mc_distribution": [],
            "token_details": [],
        }

        print(f"\n📋 Analyzing {len(tokens)} tokens at timestamps: {eval_times}s\n")

        for i, (mint, volume, launch_time) in enumerate(tokens):
            if i > 0 and i % 10 == 0:
                print(f"   Progress: {i}/{len(tokens)} tokens analyzed...")

            # Get peak market cap
            peak_mc, peak_time = self.get_peak_market_cap(mint, launch_time)
            results["peak_mc_distribution"].append(peak_mc)

            reached_1m = peak_mc >= 1_000_000
            if reached_1m:
                results["reached_1m"] += 1

            token_result = {
                "mint": mint[:8],
                "volume": volume,
                "peak_mc": peak_mc,
                "peak_time": peak_time,
                "reached_1m": reached_1m,
                "scores": {},
            }

            # Calculate scores at each evaluation time
            for eval_time in eval_times:
                score_data = self.score_token_at_time(mint, launch_time, eval_time)
                total_score = score_data["total_score"]

                token_result["scores"][eval_time] = total_score

                # Track if detected (score >= 6.0)
                if total_score >= 6.0:
                    results["by_eval_time"][eval_time]["detected"] += 1

                results["by_eval_time"][eval_time]["scores"].append(total_score)

                # Track signal contributions
                for sig_num in range(1, 8):
                    sig_key = f"signal_{sig_num}"
                    results["by_eval_time"][eval_time]["signal_contributions"][
                        sig_key
                    ].append(score_data[sig_key]["score"])

            results["token_details"].append(token_result)

        # Print results
        self.print_backtest_results(results)

        return results

    def print_backtest_results(self, results: Dict):
        """Print comprehensive backtest results"""

        print("\n" + "=" * 80)
        print("📊 BACKTEST RESULTS")
        print("=" * 80)

        total = results["tokens_analyzed"]
        reached_1m = results["reached_1m"]

        print(f"\n🎯 OVERALL METRICS:")
        print(f"   Tokens Analyzed: {total}")
        print(f"   Reached 1M+ MC: {reached_1m} ({reached_1m/total*100:.1f}%)")

        if results["peak_mc_distribution"]:
            avg_peak = statistics.mean(results["peak_mc_distribution"])
            median_peak = statistics.median(results["peak_mc_distribution"])
            print(f"   Avg Peak MC: {avg_peak/1000:.0f}K SOL")
            print(f"   Median Peak MC: {median_peak/1000:.0f}K SOL")

        # Results by evaluation time
        for eval_time in sorted(results["by_eval_time"].keys()):
            data = results["by_eval_time"][eval_time]
            scores = data["scores"]

            if not scores:
                continue

            print(f"\n⏱️  AT {eval_time}s AFTER LAUNCH:")
            print(
                f"   Tokens with score ≥6.0: {data['detected']}/{total} ({data['detected']/total*100:.1f}%)"
            )
            print(f"   Avg Score: {statistics.mean(scores):.2f}/15.0")
            print(f"   Median Score: {statistics.median(scores):.2f}/15.0")
            print(f"   Max Score: {max(scores):.2f}/15.0")

            # Show score distribution
            score_ranges = {
                "9.0+": sum(1 for s in scores if s >= 9.0),
                "8.0-8.9": sum(1 for s in scores if 8.0 <= s < 9.0),
                "7.0-7.9": sum(1 for s in scores if 7.0 <= s < 8.0),
                "6.0-6.9": sum(1 for s in scores if 6.0 <= s < 7.0),
                "<6.0": sum(1 for s in scores if s < 6.0),
            }

            print(f"\n   Score Distribution:")
            for range_name, count in score_ranges.items():
                pct = count / total * 100
                bar = "█" * int(pct / 2)
                print(f"     {range_name:8s}: {count:3d} ({pct:5.1f}%) {bar}")

            # Signal contributions
            print(f"\n   Average Signal Contributions:")
            for sig_num in range(1, 8):
                sig_key = f"signal_{sig_num}"
                sig_scores = data["signal_contributions"][sig_key]
                if sig_scores:
                    avg_contribution = statistics.mean(sig_scores)
                    print(f"     Signal {sig_num}: {avg_contribution:.2f}")

        # Top scoring tokens
        print(f"\n🏆 TOP 10 TOKENS BY SCORE AT 120s:")
        top_tokens = sorted(
            results["token_details"],
            key=lambda x: x["scores"].get(120, 0),
            reverse=True,
        )[:10]

        for i, token in enumerate(top_tokens, 1):
            score_120 = token["scores"].get(120, 0)
            print(
                f"   {i:2d}. {token['mint']} | Score: {score_120:.1f}/15.0 | "
                f"Peak MC: {token['peak_mc']/1000:.0f}K | "
                f"Volume: {token['volume']:.0f} SOL | "
                f"{'✅ 1M+' if token['reached_1m'] else '❌ <1M'}"
            )

        print("\n" + "=" * 80)


def main():
    backtest = ScoringBacktest(DB_PATH)
    results = backtest.run_backtest(min_volume=50, sample_size=100)

    print("\n✅ Backtest complete!")
    print(f"   Analysis saved for {results['tokens_analyzed']} tokens")


if __name__ == "__main__":
    main()
