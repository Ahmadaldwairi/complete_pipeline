#!/bin/bash
# Quick test to verify Brain metrics endpoint

echo "🧪 Quick Metrics Test"
echo "===================="
echo ""

# Start Brain in background
echo "🚀 Starting Brain service..."
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot/brain
cargo run --release > /tmp/brain_test.log 2>&1 &
BRAIN_PID=$!

echo "   Brain PID: $BRAIN_PID"
echo ""

# Wait for startup
echo "⏳ Waiting 15 seconds for service to start..."
sleep 15

echo ""
echo "📊 Testing endpoints..."
echo "---------------------"

# Test health endpoint
echo "1. Health Check:"
curl -s http://localhost:9090/health | jq . 2>/dev/null || curl -s http://localhost:9090/health
echo ""
echo ""

# Test metrics endpoint
echo "2. Metrics Endpoint (first 30 lines):"
curl -s http://localhost:9090/metrics | head -30
echo ""
echo ""

# Count metrics
METRIC_COUNT=$(curl -s http://localhost:9090/metrics | grep -v '^#' | grep -v '^$' | wc -l)
echo "3. Total metrics found: $METRIC_COUNT"
echo ""

# Check key metrics
echo "4. Key Metrics Present:"
for metric in "brain_decisions_total" "brain_sol_price_usd" "brain_mint_cache_hits" "brain_guardrail_rate_limit"; do
    if curl -s http://localhost:9090/metrics | grep -q "^$metric"; then
        VALUE=$(curl -s http://localhost:9090/metrics | grep "^$metric" | awk '{print $2}')
        echo "   ✅ $metric = $VALUE"
    else
        echo "   ❌ $metric NOT FOUND"
    fi
done

echo ""
echo "---------------------"
echo "🛑 Stopping Brain service..."
kill $BRAIN_PID 2>/dev/null
wait $BRAIN_PID 2>/dev/null

echo "✅ Test complete!"
echo ""
echo "Full logs saved to: /tmp/brain_test.log"
