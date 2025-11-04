#!/bin/bash
# Stop All Trading Services
# Usage: ./STOP_ALL_SERVICES.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🛑 Stopping all trading services..."
echo ""

if [ -f "$SCRIPT_DIR/pids.txt" ]; then
    PIDS=$(cat "$SCRIPT_DIR/pids.txt")
    echo "Found PIDs: $PIDS"
    
    for PID in $PIDS; do
        if kill -0 $PID 2>/dev/null; then
            echo "  Stopping PID $PID..."
            kill $PID
        else
            echo "  PID $PID not running"
        fi
    done
    
    echo ""
    echo "✅ All services stopped"
    rm "$SCRIPT_DIR/pids.txt"
else
    echo "⚠️  No pids.txt found. Trying to find processes by name..."
    
    # Kill by process name
    pkill -f "data-mining" 2>/dev/null && echo "  Stopped data-mining" || echo "  data-mining not running"
    pkill -f "decision_engine" 2>/dev/null && echo "  Stopped brain" || echo "  brain not running"
    pkill -f "execution_bot" 2>/dev/null && echo "  Stopped executor" || echo "  executor not running"
    
    echo ""
    echo "✅ Done"
fi
