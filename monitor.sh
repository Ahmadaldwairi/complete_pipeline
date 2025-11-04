#!/bin/bash

# Simple monitoring script for the trading bot
# Shows key metrics from all three services

echo "================================================"
echo "🤖 TRADING BOT MONITOR"
echo "================================================"
echo ""

# Check if processes are running
echo "📊 Service Status:"
if pgrep -f "data-mining" > /dev/null; then
    echo "  ✅ data-mining: RUNNING"
else
    echo "  ❌ data-mining: STOPPED"
fi

if pgrep -f "decision_engine" > /dev/null; then
    echo "  ✅ Brain: RUNNING"
else
    echo "  ❌ Brain: STOPPED"
fi

if pgrep -f "execution-bot" > /dev/null; then
    echo "  ✅ Executor: RUNNING"
else
    echo "  ❌ Executor: STOPPED"
fi

echo ""
echo "================================================"
echo "📈 Live Logs (Ctrl+C to stop):"
echo "================================================"
echo ""

# Tail all logs with color coding
tail -f \
  <(cd data-mining && ./target/release/data-mining 2>&1 | sed 's/^/[DATA] /') \
  <(cd brain && ./target/release/decision_engine 2>&1 | sed 's/^/[BRAIN] /') \
  <(cd execution && ./target/release/execution-bot 2>&1 | sed 's/^/[EXEC] /') \
  2>/dev/null
