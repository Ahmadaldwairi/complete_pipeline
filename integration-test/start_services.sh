#!/bin/bash
# Master service launcher for Solana Scalper Bot
# Starts all services in separate terminal windows

PROJECT_ROOT="/home/sol/Desktop/solana-dev/Bots/scalper-bot"

echo "╔════════════════════════════════════════════════════════════╗"
echo "║         SOLANA SCALPER BOT - SERVICE LAUNCHER              ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "This script will launch all services in separate terminals:"
echo "  1. Brain Service (decision engine)"
echo "  2. Executor Service (trade execution)"
echo "  3. Mempool Watcher (optional)"
echo ""

read -p "Start all services? (y/n): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "❌ Cancelled"
    exit 1
fi

# Check if gnome-terminal is available
if ! command -v gnome-terminal &> /dev/null; then
    echo "⚠️  gnome-terminal not found. Starting services in background..."
    
    # Start Brain
    cd "$PROJECT_ROOT/brain"
    cargo run --release > brain.log 2>&1 &
    BRAIN_PID=$!
    echo "✅ Brain started (PID: $BRAIN_PID, log: brain/brain.log)"
    
    # Start Executor
    cd "$PROJECT_ROOT/execution"
    cargo run --release > executor.log 2>&1 &
    EXECUTOR_PID=$!
    echo "✅ Executor started (PID: $EXECUTOR_PID, log: execution/executor.log)"
    
    # Start Mempool (optional)
    read -p "Start Mempool Watcher? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        cd "$PROJECT_ROOT/mempool-watcher"
        cargo run --release > mempool.log 2>&1 &
        MEMPOOL_PID=$!
        echo "✅ Mempool started (PID: $MEMPOOL_PID, log: mempool-watcher/mempool.log)"
    fi
    
    echo ""
    echo "📝 Logs:"
    echo "   Brain: $PROJECT_ROOT/brain/brain.log"
    echo "   Executor: $PROJECT_ROOT/execution/executor.log"
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "   Mempool: $PROJECT_ROOT/mempool-watcher/mempool.log"
    fi
    
    echo ""
    echo "🛑 To stop all services:"
    echo "   kill $BRAIN_PID $EXECUTOR_PID"
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "   kill $MEMPOOL_PID"
    fi
    
else
    # Start Brain in new terminal
    gnome-terminal --title="Brain Service" -- bash -c "
        cd $PROJECT_ROOT/brain && 
        echo '🧠 Starting Brain Service...' && 
        cargo run --release; 
        echo ''; 
        echo '❌ Brain service stopped. Press ENTER to close.'; 
        read
    " &
    echo "✅ Brain terminal launched"
    sleep 2
    
    # Start Executor in new terminal
    gnome-terminal --title="Executor Service" -- bash -c "
        cd $PROJECT_ROOT/execution && 
        echo '⚡ Starting Executor Service...' && 
        cargo run --release; 
        echo ''; 
        echo '❌ Executor service stopped. Press ENTER to close.'; 
        read
    " &
    echo "✅ Executor terminal launched"
    sleep 2
    
    # Ask about Mempool
    read -p "Start Mempool Watcher? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        gnome-terminal --title="Mempool Watcher" -- bash -c "
            cd $PROJECT_ROOT/mempool-watcher && 
            echo '🔥 Starting Mempool Watcher...' && 
            cargo run --release; 
            echo ''; 
            echo '❌ Mempool watcher stopped. Press ENTER to close.'; 
            read
        " &
        echo "✅ Mempool terminal launched"
    fi
    
    echo ""
    echo "🎯 All services launched in separate terminals!"
fi

echo ""
echo "⏳ Waiting 5 seconds for services to start..."
sleep 5

echo ""
echo "🔍 Checking service status..."
cd "$PROJECT_ROOT/integration-test"
python3 test_ports.py

echo ""
echo "✅ Setup complete!"
echo ""
echo "📝 Next steps:"
echo "   1. Check that all services show ✅ LISTENING"
echo "   2. Run E2E test: cd integration-test && python3 test_e2e.py"
echo "   3. Monitor logs in each service terminal"
echo ""
