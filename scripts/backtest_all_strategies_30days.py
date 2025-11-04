#!/usr/bin/env python3
"""
Comprehensive 30-Day Backtest - ALL Trading Strategies
Tests: $1 Scalping, Rank, Momentum, Copy, Late, 1M+ MC Hunting

UPDATED: Using REALISTIC position sizes based on actual trading capital
- Regular trades: $1-5 USD entry (0.005-0.027 SOL)
- 1M+ MC Hunt: max 1 SOL entry ($186)
- SOL price: $186 USD
"""

import sqlite3
from datetime import datetime, timedelta
from typing import Dict, List, Tuple, Optional
from collections import defaultdict
import statistics

DB_PATH = "data-mining/data/collector.db"
SOL_PRICE_USD = 186.0  # Current SOL/USD price


class ComprehensiveBacktest:
    def __init__(self, db_path: str):
        self.conn = sqlite3.connect(db_path)
        self.conn.row_factory = sqlite3.Row

    def get_tokens_last_n_days(self, days: int = 30) -> List:
        """Get all tokens from last N days"""
        cursor = self.conn.cursor()
        cutoff_time = int((datetime.now() - timedelta(days=days)).timestamp())

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
        ORDER BY t.launch_block_time DESC
        """

        cursor.execute(query, (cutoff_time,))
        return cursor.fetchall()

    def get_trades(
        self, mint: str, launch_time: int, start_offset: int, end_offset: int
    ) -> List:
        """Get trades in specific time window"""
        cursor = self.conn.cursor()

        query = """
        SELECT trader, amount_sol, side, block_time
        FROM trades
        WHERE mint = ?
          AND block_time BETWEEN ? AND ?
        ORDER BY block_time ASC
        """

        cursor.execute(
            query, (mint, launch_time + start_offset, launch_time + end_offset)
        )
        return cursor.fetchall()

    def get_price_data(
        self, mint: str, launch_time: int, max_seconds: int = 1800
    ) -> List:
        """Get price windows for the token"""
        cursor = self.conn.cursor()

        query = """
        SELECT start_time, close, high, low, vol_sol
        FROM windows
        WHERE mint = ?
          AND window_sec = 60
          AND start_time BETWEEN ? AND ?
        ORDER BY start_time ASC
        """

        cursor.execute(query, (mint, launch_time, launch_time + max_seconds))
        return cursor.fetchall()

    def get_creator_stats(self, creator_wallet: str) -> Optional[Tuple[float, int]]:
        """Get creator reputation from wallet_stats"""
        cursor = self.conn.cursor()

        query = """
        SELECT net_pnl_sol, create_count
        FROM wallet_stats
        WHERE wallet = ?
        """

        cursor.execute(query, (creator_wallet,))
        row = cursor.fetchone()

        if row:
            return (row["net_pnl_sol"] or 0.0, row["create_count"] or 0)
        return None

    def get_wallet_tier(self, wallet: str) -> str:
        """Determine wallet tier from wallet_stats"""
        cursor = self.conn.cursor()

        query = """
        SELECT profit_score, win_rate, total_trades
        FROM wallet_stats
        WHERE wallet = ?
        """

        cursor.execute(query, (wallet,))
        row = cursor.fetchone()

        if not row:
            return "Unknown"

        profit_score = row["profit_score"] or 0
        win_rate = row["win_rate"] or 0
        total_trades = row["total_trades"] or 0

        # Tier logic (simplified)
        if profit_score >= 85 and win_rate >= 0.70 and total_trades >= 50:
            return "S"  # Elite
        elif profit_score >= 75 and win_rate >= 0.60 and total_trades >= 20:
            return "A"  # Excellent
        elif profit_score >= 65 and win_rate >= 0.50 and total_trades >= 10:
            return "B"  # Good
        elif profit_score >= 50 and total_trades >= 5:
            return "C"  # Acceptable
        else:
            return "D"  # Discovery/Unproven

    # ============================================================================
    # STRATEGY 1: $1 SCALPING (Original Strategy)
    # ============================================================================

    def test_dollar_scalping(
        self, mint: str, launch_time: int, prices: List
    ) -> Optional[Dict]:
        """Test $1 scalping strategy: Quick exits on any pump"""
        if len(prices) < 2:
            return None

        # Entry: 30-60 seconds after launch
        entry_price = None
        entry_time = None

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if 30 <= time_offset <= 60:
                entry_price = p["close"]
                entry_time = time_offset
                break

        if not entry_price or entry_price <= 0:
            return None

        # Exit: First 3-5% gain OR 10-20 second hold
        # REALISTIC POSITION: $1 USD = 0.0054 SOL at $186/SOL
        position_size_sol = 1.0 / SOL_PRICE_USD  # $1 USD position
        exit_price = entry_price
        exit_reason = "time"

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if time_offset <= entry_time:
                continue

            gain_pct = ((p["high"] - entry_price) / entry_price) * 100

            # Take profit at 3-5%
            if gain_pct >= 3.0:
                exit_price = entry_price * 1.03  # Conservative 3%
                exit_reason = "profit_3pct"
                break

            # Stop loss at -2%
            if p["low"] < entry_price * 0.98:
                exit_price = entry_price * 0.98
                exit_reason = "stop_loss"
                break

            # Time-based exit at 20 seconds
            if time_offset >= entry_time + 20:
                exit_price = p["close"]
                exit_reason = "time_20s"
                break

        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)
        pnl_usd = pnl_sol * SOL_PRICE_USD

        # Debug: Print if P&L is exactly 0
        if abs(pnl_usd) < 0.01 and exit_reason == "time":
            # Default exit - no price movement detected
            return None

        return {
            "strategy": "dollar_scalping",
            "position_size_sol": position_size_sol,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_usd,
            "gain_pct": (
                ((exit_price - entry_price) / entry_price) * 100
                if entry_price > 0
                else 0
            ),
        }

    # ============================================================================
    # STRATEGY 2: PATH A - RANK BASED
    # ============================================================================

    def test_rank_based(
        self, mint: str, launch_time: int, prices: List, volume_5min: float
    ) -> Optional[Dict]:
        """Test Rank-Based strategy: Top launches with momentum"""
        if len(prices) < 3:
            return None

        # Check if this would be a "top ranked" launch
        # Criteria: High volume in first 5 minutes
        if volume_5min < 20.0:  # Threshold for "top ranked"
            return None

        # Entry at 60-120 seconds
        entry_price = None
        for p in prices:
            time_offset = p["start_time"] - launch_time
            if 60 <= time_offset <= 120:
                entry_price = p["close"]
                break

        if not entry_price or entry_price <= 0:
            return None

        # Position sizing
        # REALISTIC POSITION: $5 USD = 0.027 SOL at $186/SOL
        position_size_sol = 5.0 / SOL_PRICE_USD  # $5 USD position

        # Exit: +30% target or -20% stop or 30s max hold
        exit_price = entry_price
        exit_reason = "time"

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if time_offset <= 60:
                continue

            gain_pct = ((p["high"] - entry_price) / entry_price) * 100

            if gain_pct >= 30.0:
                exit_price = entry_price * 1.30
                exit_reason = "target_30pct"
                break

            if p["low"] < entry_price * 0.80:
                exit_price = entry_price * 0.80
                exit_reason = "stop_loss_20pct"
                break

            if time_offset >= 60 + 30:  # 30s max hold
                exit_price = p["close"]
                exit_reason = "time_30s"
                break

        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)

        return {
            "strategy": "rank_based",
            "position_size_sol": position_size_sol,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_sol * SOL_PRICE_USD,
            "gain_pct": ((exit_price - entry_price) / entry_price) * 100,
        }

    # ============================================================================
    # STRATEGY 3: PATH B - MOMENTUM
    # ============================================================================

    def test_momentum(
        self, mint: str, launch_time: int, prices: List
    ) -> Optional[Dict]:
        """Test Momentum strategy: High buyer/volume surges"""
        if len(prices) < 4:
            return None

        # Check for momentum signal (high volume surge in 60-120s window)
        trades_60_120 = self.get_trades(mint, launch_time, 60, 120)

        if len(trades_60_120) < 5:  # Need at least 5 trades
            return None

        buyers = set([t["trader"] for t in trades_60_120 if t["side"] == "buy"])
        volume = sum([t["amount_sol"] for t in trades_60_120 if t["side"] == "buy"])

        # Momentum criteria: 3+ buyers, 4+ SOL volume
        if len(buyers) < 3 or volume < 4.0:
            return None

        # Entry at 120 seconds
        entry_price = None
        for p in prices:
            time_offset = p["start_time"] - launch_time
            if 120 <= time_offset <= 130:
                entry_price = p["close"]
                break

        if not entry_price or entry_price <= 0:
            return None

        # REALISTIC POSITION: $3 USD = 0.016 SOL at $186/SOL
        position_size_sol = 3.0 / SOL_PRICE_USD  # $3 USD position

        # Exit: +50% target or -15% stop, 120s max hold
        exit_price = entry_price
        exit_reason = "time"

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if time_offset <= 120:
                continue

            gain_pct = ((p["high"] - entry_price) / entry_price) * 100

            if gain_pct >= 50.0:
                exit_price = entry_price * 1.50
                exit_reason = "target_50pct"
                break

            if p["low"] < entry_price * 0.85:
                exit_price = entry_price * 0.85
                exit_reason = "stop_loss_15pct"
                break

            if time_offset >= 120 + 120:  # 120s max hold
                exit_price = p["close"]
                exit_reason = "time_120s"
                break

        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)

        return {
            "strategy": "momentum",
            "position_size_sol": position_size_sol,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_sol * SOL_PRICE_USD,
            "gain_pct": ((exit_price - entry_price) / entry_price) * 100,
            "buyers": len(buyers),
            "volume": volume,
        }

    # ============================================================================
    # STRATEGY 4: PATH C - COPY TRADING
    # ============================================================================

    def test_copy_trading(
        self, mint: str, launch_time: int, prices: List
    ) -> Optional[Dict]:
        """Test Copy Trading: Follow profitable wallets"""
        if len(prices) < 2:
            return None

        # Get early trades (first 60 seconds)
        early_trades = self.get_trades(mint, launch_time, 0, 60)

        if not early_trades:
            return None

        # Find trades from Tier C or better wallets
        profitable_trade = None
        for trade in early_trades:
            if trade["side"] != "buy":
                continue
            if trade["amount_sol"] < 0.25:  # Min copy size
                continue

            tier = self.get_wallet_tier(trade["trader"])
            if tier in ["S", "A", "B", "C"]:
                profitable_trade = trade
                break

        if not profitable_trade:
            return None

        # Entry: Copy the trade (same time window)
        entry_price = None
        entry_time = profitable_trade["block_time"] - launch_time

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if abs(time_offset - entry_time) < 10:  # Within 10s
                entry_price = p["close"]
                break

        if not entry_price or entry_price <= 0:
            return None

        # REALISTIC POSITION: $2 USD = 0.011 SOL at $186/SOL
        position_size_sol = 2.0 / SOL_PRICE_USD  # $2 USD position

        # Exit: +20% target or -10% stop, 15s max hold
        exit_price = entry_price
        exit_reason = "time"

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if time_offset <= entry_time:
                continue

            gain_pct = ((p["high"] - entry_price) / entry_price) * 100

            if gain_pct >= 20.0:
                exit_price = entry_price * 1.20
                exit_reason = "target_20pct"
                break

            if p["low"] < entry_price * 0.90:
                exit_price = entry_price * 0.90
                exit_reason = "stop_loss_10pct"
                break

            if time_offset >= entry_time + 15:  # 15s max hold
                exit_price = p["close"]
                exit_reason = "time_15s"
                break

        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)

        return {
            "strategy": "copy_trading",
            "position_size_sol": position_size_sol,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_sol * SOL_PRICE_USD,
            "gain_pct": ((exit_price - entry_price) / entry_price) * 100,
            "copied_wallet_tier": self.get_wallet_tier(profitable_trade["trader"]),
        }

    # ============================================================================
    # STRATEGY 5: PATH D - LATE OPPORTUNITY
    # ============================================================================

    def test_late_opportunity(
        self, mint: str, launch_time: int, prices: List
    ) -> Optional[Dict]:
        """Test Late Opportunity: Mature launches with sustained volume"""
        if len(prices) < 20:  # Need 20+ minutes of data
            return None

        # Check for late volume (20+ minutes after launch)
        late_trades = self.get_trades(mint, launch_time, 1200, 1260)  # 20-21 min window

        if not late_trades:
            return None

        buyers = set([t["trader"] for t in late_trades if t["side"] == "buy"])
        volume = sum([t["amount_sol"] for t in late_trades if t["side"] == "buy"])

        # Late opportunity criteria: 10+ buyers, 10+ SOL volume
        if len(buyers) < 10 or volume < 10.0:
            return None

        # Entry at 20 minutes
        entry_price = None
        for p in prices:
            time_offset = p["start_time"] - launch_time
            if 1200 <= time_offset <= 1260:
                entry_price = p["close"]
                break

        if not entry_price or entry_price <= 0:
            return None

        # REALISTIC POSITION: $1 USD = 0.0054 SOL at $186/SOL
        position_size_sol = 1.0 / SOL_PRICE_USD  # $1 USD position

        # Exit: +25% target or -20% stop, 300s max hold
        exit_price = entry_price
        exit_reason = "time"

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if time_offset <= 1200:
                continue

            gain_pct = ((p["high"] - entry_price) / entry_price) * 100

            if gain_pct >= 25.0:
                exit_price = entry_price * 1.25
                exit_reason = "target_25pct"
                break

            if p["low"] < entry_price * 0.80:
                exit_price = entry_price * 0.80
                exit_reason = "stop_loss_20pct"
                break

            if time_offset >= 1200 + 300:  # 300s (5 min) max hold
                exit_price = p["close"]
                exit_reason = "time_300s"
                break

        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)

        return {
            "strategy": "late_opportunity",
            "position_size_sol": position_size_sol,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_sol * SOL_PRICE_USD,
            "gain_pct": ((exit_price - entry_price) / entry_price) * 100,
        }

    # ============================================================================
    # STRATEGY 6: 1M+ MC HUNTING (7-Signal Scoring)
    # ============================================================================

    def calculate_7_signal_score(
        self, mint: str, launch_time: int, creator_wallet: str
    ) -> Dict:
        """Calculate 7-signal score for 1M+ MC hunting"""
        score = {
            "signal_1": 0.0,  # Creator reputation
            "signal_2": 0.0,  # Buyer speed
            "signal_3": 0.0,  # Liquidity ratio (will be 0 - no data)
            "signal_4": 0.0,  # Wallet overlap
            "signal_5": 0.0,  # Buy concentration
            "signal_6": 0.0,  # Volume acceleration
            "signal_7": 0.0,  # MC velocity
            "total": 0.0,
        }

        # Signal 1: Creator reputation
        creator_stats = self.get_creator_stats(creator_wallet)
        if creator_stats:
            net_pnl, create_count = creator_stats
            if net_pnl >= 500 and create_count >= 5:
                score["signal_1"] = 2.0
            elif net_pnl >= 200 and create_count >= 3:
                score["signal_1"] = 1.5
            elif net_pnl >= 50:
                score["signal_1"] = 1.0

        # Signal 2: Buyer speed
        trades_60 = self.get_trades(mint, launch_time, 0, 60)
        buyers_60 = set([t["trader"] for t in trades_60 if t["side"] == "buy"])

        if len(buyers_60) >= 10:
            score["signal_2"] = 2.0
        elif len(buyers_60) >= 7:
            score["signal_2"] = 1.5
        elif len(buyers_60) >= 5:
            score["signal_2"] = 1.0

        # Signal 3: Liquidity ratio (ALWAYS 0 - no data available)
        score["signal_3"] = 0.0

        # Signal 4: Wallet overlap (check if proven winners are buying)
        proven_winners_buying = 0
        for trade in trades_60:
            if trade["side"] != "buy":
                continue
            tier = self.get_wallet_tier(trade["trader"])
            if tier in ["S", "A", "B"]:
                proven_winners_buying += 1

        if proven_winners_buying >= 3:
            score["signal_4"] = 2.0
        elif proven_winners_buying >= 2:
            score["signal_4"] = 1.5
        elif proven_winners_buying >= 1:
            score["signal_4"] = 1.0

        # Signal 5: Buy concentration
        trades_120 = self.get_trades(mint, launch_time, 0, 120)
        buy_trades = [t for t in trades_120 if t["side"] == "buy"]

        if buy_trades:
            trader_volumes = defaultdict(float)
            total_volume = 0.0

            for trade in buy_trades:
                trader_volumes[trade["trader"]] += trade["amount_sol"]
                total_volume += trade["amount_sol"]

            if total_volume > 0:
                top3 = sorted(trader_volumes.values(), reverse=True)[:3]
                concentration = (sum(top3) / total_volume) * 100

                if concentration < 70:
                    score["signal_5"] = 1.0
                elif concentration < 80:
                    score["signal_5"] = 0.5

        # Signal 6: Volume acceleration
        trades_90_120 = self.get_trades(mint, launch_time, 90, 120)
        trades_60_90 = self.get_trades(mint, launch_time, 60, 90)

        vol_recent = sum([t["amount_sol"] for t in trades_90_120 if t["side"] == "buy"])
        vol_baseline = sum(
            [t["amount_sol"] for t in trades_60_90 if t["side"] == "buy"]
        )

        if vol_baseline > 0.5 and vol_recent > 0.5:
            accel = vol_recent / vol_baseline
            if accel >= 2.0:
                score["signal_6"] = 1.5
            elif accel >= 1.5:
                score["signal_6"] = 1.0

        # Signal 7: MC velocity
        prices = self.get_price_data(mint, launch_time, 300)
        if len(prices) >= 3:
            # Calculate price velocity (SOL/min)
            first_price = prices[0]["close"]
            third_price = prices[2]["close"] if len(prices) > 2 else prices[-1]["close"]

            if first_price > 0:
                mc_change = (
                    third_price - first_price
                ) * 1_000_000_000  # Estimated MC change
                time_delta_min = 3  # 3 minutes
                velocity = mc_change / time_delta_min

                if velocity >= 1000:
                    score["signal_7"] = 3.0
                elif velocity >= 500:
                    score["signal_7"] = 2.0
                elif velocity >= 200:
                    score["signal_7"] = 1.0

        score["total"] = sum(v for k, v in score.items() if k != "total")
        return score

    def test_1m_mc_hunting(
        self, mint: str, launch_time: int, creator_wallet: str, prices: List
    ) -> Optional[Dict]:
        """Test 1M+ MC Hunting: 7-signal scoring system"""
        if len(prices) < 3:
            return None

        # Calculate score at 120 seconds
        score_data = self.calculate_7_signal_score(mint, launch_time, creator_wallet)

        # Entry threshold: Score >= 6.0
        if score_data["total"] < 6.0:
            return None

        # Entry at 120 seconds
        entry_price = None
        for p in prices:
            time_offset = p["start_time"] - launch_time
            if 120 <= time_offset <= 130:
                entry_price = p["close"]
                break

        if not entry_price or entry_price <= 0:
            return None

        # REALISTIC POSITION SIZING based on score
        # Max position: 1 SOL ($186 USD)
        if score_data["total"] >= 9.0:
            position_size_sol = 1.0  # $186 (best signals)
            profit_target = 1.50  # +150%
            stop_loss = 0.80  # -20%
        elif score_data["total"] >= 8.0:
            position_size_sol = 0.75  # $140
            profit_target = 1.50
            stop_loss = 0.85
        elif score_data["total"] >= 7.0:
            position_size_sol = 0.50  # $93
            profit_target = 1.30
            stop_loss = 0.80
        else:  # 6.0-6.9
            position_size_sol = 0.30  # $56
            profit_target = 1.30
            stop_loss = 0.80

        # Exit: Target or stop loss or peak MC 1M+
        exit_price = entry_price
        exit_reason = "time"
        peak_mc_sol = 0.0

        for p in prices:
            time_offset = p["start_time"] - launch_time
            if time_offset <= 120:
                continue

            # Calculate MC (rough estimate)
            mc_sol = p["high"] * 1_000_000_000 / 1e9
            if mc_sol > peak_mc_sol:
                peak_mc_sol = mc_sol

            gain_pct = ((p["high"] - entry_price) / entry_price) * 100

            # Hit 1M+ MC - exit at current price
            if peak_mc_sol >= 1_000_000:
                exit_price = p["close"]
                exit_reason = "hit_1m_mc"
                break

            # Hit profit target
            if p["high"] >= entry_price * profit_target:
                exit_price = entry_price * profit_target
                exit_reason = f"target_{int((profit_target-1)*100)}pct"
                break

            # Hit stop loss
            if p["low"] < entry_price * stop_loss:
                exit_price = entry_price * stop_loss
                exit_reason = f"stop_loss_{int((1-stop_loss)*100)}pct"
                break

            # Time-based exit at 300s
            if time_offset >= 420:  # 7 minutes max
                exit_price = p["close"]
                exit_reason = "time_420s"
                break

        pnl_sol = position_size_sol * ((exit_price - entry_price) / entry_price)

        return {
            "strategy": "1m_mc_hunting",
            "position_size_sol": position_size_sol,
            "entry_price": entry_price,
            "exit_price": exit_price,
            "exit_reason": exit_reason,
            "pnl_sol": pnl_sol,
            "pnl_usd": pnl_sol * SOL_PRICE_USD,
            "gain_pct": ((exit_price - entry_price) / entry_price) * 100,
            "score": score_data["total"],
            "peak_mc_sol": peak_mc_sol / 1_000_000,  # In millions
            "hit_1m": peak_mc_sol >= 1_000_000,
        }

    # ============================================================================
    # MAIN BACKTEST RUNNER
    # ============================================================================

    def run_comprehensive_backtest(self, days: int = 30):
        """Run all strategies and compare performance"""

        print("\n" + "=" * 100)
        print(f"🚀 COMPREHENSIVE 30-DAY BACKTEST - ALL STRATEGIES")
        print("=" * 100)

        tokens = self.get_tokens_last_n_days(days)
        print(f"\n📋 Total tokens launched: {len(tokens):,}")

        # Filter for qualified tokens (>=10 SOL volume)
        qualified = [t for t in tokens if t["total_volume_5min"] >= 10.0]
        print(
            f"✅ Qualified tokens (≥10 SOL volume): {len(qualified):,} ({len(qualified)/len(tokens)*100:.1f}%)"
        )

        # Initialize results
        results = {
            "dollar_scalping": {"trades": [], "total_pnl": 0.0, "wins": 0, "losses": 0},
            "rank_based": {"trades": [], "total_pnl": 0.0, "wins": 0, "losses": 0},
            "momentum": {"trades": [], "total_pnl": 0.0, "wins": 0, "losses": 0},
            "copy_trading": {"trades": [], "total_pnl": 0.0, "wins": 0, "losses": 0},
            "late_opportunity": {
                "trades": [],
                "total_pnl": 0.0,
                "wins": 0,
                "losses": 0,
            },
            "1m_mc_hunting": {
                "trades": [],
                "total_pnl": 0.0,
                "wins": 0,
                "losses": 0,
                "1m_hits": 0,
            },
        }

        print(
            f"\n🔄 Testing {min(1000, len(qualified))} tokens across all strategies..."
        )
        print("   (This will take a few minutes...)\n")

        # Test each strategy on each token
        for i, token in enumerate(qualified[:1000]):  # Limit to 1000 for speed
            if i > 0 and i % 100 == 0:
                print(f"   Progress: {i}/1000 tokens analyzed...")

            mint = token["mint"]
            launch_time = token["launch_block_time"]
            creator = token["creator_wallet"]
            volume = token["total_volume_5min"]

            # Get price data once for all strategies
            prices = self.get_price_data(mint, launch_time, 1800)

            if not prices:
                continue

            # Test each strategy
            strategies_to_test = [
                (
                    "dollar_scalping",
                    lambda: self.test_dollar_scalping(mint, launch_time, prices),
                ),
                (
                    "rank_based",
                    lambda: self.test_rank_based(mint, launch_time, prices, volume),
                ),
                ("momentum", lambda: self.test_momentum(mint, launch_time, prices)),
                (
                    "copy_trading",
                    lambda: self.test_copy_trading(mint, launch_time, prices),
                ),
                (
                    "late_opportunity",
                    lambda: self.test_late_opportunity(mint, launch_time, prices),
                ),
                (
                    "1m_mc_hunting",
                    lambda: self.test_1m_mc_hunting(mint, launch_time, creator, prices),
                ),
            ]

            for strategy_name, test_func in strategies_to_test:
                try:
                    result = test_func()
                    if result:
                        results[strategy_name]["trades"].append(result)
                        results[strategy_name]["total_pnl"] += result["pnl_usd"]

                        if result["pnl_sol"] > 0:
                            results[strategy_name]["wins"] += 1
                        else:
                            results[strategy_name]["losses"] += 1

                        # Track 1M hits for hunting strategy
                        if strategy_name == "1m_mc_hunting" and result.get("hit_1m"):
                            results[strategy_name]["1m_hits"] += 1
                except Exception as e:
                    # Silently skip errors
                    pass

        # Print results
        self.print_comprehensive_results(results, days)

        return results

    def print_comprehensive_results(self, results: Dict, days: int):
        """Print comprehensive comparison of all strategies"""

        print("\n" + "=" * 100)
        print("📊 STRATEGY COMPARISON - 30 DAYS")
        print("=" * 100)

        total_pnl = 0.0

        strategies = [
            ("dollar_scalping", "$1 Scalping", "💵"),
            ("rank_based", "Path A: Rank", "🏆"),
            ("momentum", "Path B: Momentum", "🚀"),
            ("copy_trading", "Path C: Copy", "👥"),
            ("late_opportunity", "Path D: Late", "🕐"),
            ("1m_mc_hunting", "1M+ MC Hunting", "🎯"),
        ]

        for strategy_key, strategy_name, emoji in strategies:
            data = results[strategy_key]
            trades_count = len(data["trades"])

            if trades_count == 0:
                print(f"\n{emoji} {strategy_name}")
                print(f"   No trades executed")
                continue

            wins = data["wins"]
            losses = data["losses"]
            win_rate = (wins / trades_count * 100) if trades_count > 0 else 0
            total_pnl += data["total_pnl"]

            avg_pnl = data["total_pnl"] / trades_count
            avg_position = statistics.mean(
                [t["position_size_sol"] for t in data["trades"]]
            )

            print(f"\n{emoji} {strategy_name}")
            print(f"   Trades: {trades_count:,}")
            print(f"   Wins: {wins:,} | Losses: {losses:,} | Win Rate: {win_rate:.1f}%")
            print(f"   Total P&L: ${data['total_pnl']:,.2f}")
            print(f"   Avg P&L/trade: ${avg_pnl:,.2f}")
            print(f"   Avg position: {avg_position:.1f} SOL")

            # Strategy-specific metrics
            if strategy_key == "1m_mc_hunting":
                print(f"   Tokens hit 1M+: {data['1m_hits']}")
                if trades_count > 0:
                    avg_score = statistics.mean([t["score"] for t in data["trades"]])
                    print(f"   Avg entry score: {avg_score:.2f}/15.0")

            if strategy_key == "momentum":
                trades_with_buyers = [t for t in data["trades"] if "buyers" in t]
                if trades_with_buyers:
                    avg_buyers = statistics.mean(
                        [t["buyers"] for t in trades_with_buyers]
                    )
                    print(f"   Avg buyers: {avg_buyers:.1f}")

            if strategy_key == "copy_trading":
                tier_counts = defaultdict(int)
                for t in data["trades"]:
                    if "copied_wallet_tier" in t:
                        tier_counts[t["copied_wallet_tier"]] += 1
                print(f"   Wallet tiers copied: {dict(tier_counts)}")

        # Combined totals
        print("\n" + "=" * 100)
        print("💰 COMBINED RESULTS")
        print("=" * 100)

        total_trades = sum(len(data["trades"]) for data in results.values())
        total_wins = sum(data["wins"] for data in results.values())
        total_losses = sum(data["losses"] for data in results.values())
        overall_win_rate = (total_wins / total_trades * 100) if total_trades > 0 else 0

        print(f"\n📈 Total Trades: {total_trades:,}")
        print(
            f"   Wins: {total_wins:,} | Losses: {total_losses:,} | Win Rate: {overall_win_rate:.1f}%"
        )
        print(f"\n💵 Total P&L: ${total_pnl:,.2f}")
        print(f"   Per day: ${total_pnl/days:,.2f}")
        print(f"   Per trade: ${total_pnl/total_trades:,.2f}")

        # Best performing strategy
        best_strategy = max(results.items(), key=lambda x: x[1]["total_pnl"])
        best_name = [name for key, name, _ in strategies if key == best_strategy[0]][0]

        print(f"\n🏆 Best Performing Strategy: {best_name}")
        print(f"   P&L: ${best_strategy[1]['total_pnl']:,.2f}")

        print("\n" + "=" * 100)


def main():
    backtest = ComprehensiveBacktest(DB_PATH)
    results = backtest.run_comprehensive_backtest(days=30)


if __name__ == "__main__":
    main()
