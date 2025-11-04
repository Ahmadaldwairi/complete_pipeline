#!/bin/bash
# Start All Services - Complete Trading System
# Usage: ./START_ALL_SERVICES.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=================================="
echo "🚀 Starting Trading System"
echo "=================================="
echo ""

# Check if all binaries exist
echo "🔍 Checking binaries..."
if [ ! -f "$SCRIPT_DIR/data-mining/target/release/data-mining" ]; then
    echo "❌ data-mining binary not found! Run: cd data-mining && cargo build --release"
    exit 1
fi

if [ ! -f "$SCRIPT_DIR/brain/target/release/decision_engine" ]; then
    echo "❌ brain binary not found! Run: cd brain && cargo build --release"
    exit 1
fi

if [ ! -f "$SCRIPT_DIR/execution/target/release/execution_bot" ]; then
    echo "❌ executor binary not found! Run: cd execution && cargo build --release"
    exit 1
fi

echo "✅ All binaries found"
echo ""

# Check environment files
echo "🔍 Checking configuration..."
if [ ! -f "$SCRIPT_DIR/data-mining/.env" ]; then
    echo "⚠️  data-mining/.env not found"
fi

if [ ! -f "$SCRIPT_DIR/brain/.env" ]; then
    echo "❌ brain/.env not found!"
    exit 1
fi

if [ ! -f "$SCRIPT_DIR/execution/.env" ]; then
    echo "❌ execution/.env not found!"
    exit 1
fi

echo "✅ Configuration files found"
echo ""

# Create log directory
mkdir -p "$SCRIPT_DIR/logs"

echo "=================================="
echo "Starting services in order..."
echo "=================================="
echo ""

# 1. Start Data-Mining
echo "📊 [1/3] Starting Data-Mining Service..."
cd "$SCRIPT_DIR/data-mining"
./target/release/data-mining > ../logs/data-mining.log 2>&1 &
DATA_MINING_PID=$!
echo "✅ Data-Mining started (PID: $DATA_MINING_PID)"
sleep 2

# 2. Start Brain
echo "🧠 [2/3] Starting Brain Service..."
cd "$SCRIPT_DIR/brain"
./target/release/decision_engine > ../logs/brain.log 2>&1 &
BRAIN_PID=$!
echo "✅ Brain started (PID: $BRAIN_PID)"
sleep 2

# 3. Start Executor
echo "⚡ [3/3] Starting Executor Service..."
cd "$SCRIPT_DIR/execution"
./target/release/execution_bot > ../logs/executor.log 2>&1 &
EXECUTOR_PID=$!
echo "✅ Executor started (PID: $EXECUTOR_PID)"
sleep 1

echo ""
echo "=================================="
echo "✅ All services started!"
echo "=================================="
echo ""
echo "Process IDs:"
echo "  Data-Mining: $DATA_MINING_PID"
echo "  Brain:       $BRAIN_PID"
echo "  Executor:    $EXECUTOR_PID"
echo ""
echo "Logs directory: $SCRIPT_DIR/logs/"
echo ""
echo "To monitor logs:"
echo "  tail -f logs/data-mining.log"
echo "  tail -f logs/brain.log"
echo "  tail -f logs/executor.log"
echo ""
echo "To stop all services:"
echo "  kill $DATA_MINING_PID $BRAIN_PID $EXECUTOR_PID"
echo ""
echo "Or save PIDs for later:"
echo "  echo \"$DATA_MINING_PID $BRAIN_PID $EXECUTOR_PID\" > pids.txt"
echo ""

# Save PIDs
echo "$DATA_MINING_PID $BRAIN_PID $EXECUTOR_PID" > "$SCRIPT_DIR/pids.txt"
echo "✅ PIDs saved to pids.txt"
echo ""
echo "Press Ctrl+C to stop all services, or run: kill \$(cat pids.txt)"
echo ""

# Wait for any process to exit
wait -n

# If one exits, kill all
echo ""
echo "⚠️  One service exited. Stopping all services..."
kill $DATA_MINING_PID $BRAIN_PID $EXECUTOR_PID 2>/dev/null || true
echo "✅ All services stopped"
