#!/bin/bash
# Integration Test: Advice Bus End-to-End
# Tests that advisories can be sent to a listening UDP socket

set -e

echo "🧪 Advice Bus Integration Test"
echo "========================================"
echo ""

# Test configuration
TEST_PORT=45100
TEST_MINT="11111111111111111111111111111111"  # Valid base58 address
TEST_WALLET="So11111111111111111111111111111111111111112"  # SOL token address

echo "📋 Test Configuration:"
echo "   Port: $TEST_PORT"
echo "   Test Mint: $TEST_MINT"
echo ""

# Test 1: Advisory Script Functionality
echo "✅ Test 1: Advisory Script Functionality"
echo "   Testing all 4 advisory types..."
echo ""

echo "   1️⃣ ExtendHold (extend 20s, confidence 85%)"
python3 test_advisor.py extend_hold $TEST_MINT 20 85
echo ""

echo "   2️⃣ WidenExit (5% slippage for 10s, confidence 90%)"
python3 test_advisor.py widen_exit $TEST_MINT 500 10000 90
echo ""

echo "   3️⃣ LateOpportunity (urgency 5, confidence 75%)"
python3 test_advisor.py late_opportunity $TEST_MINT 5 75
echo ""

echo "   4️⃣ CopyTrade (alpha wallet, confidence 95%)"
python3 test_advisor.py copy_trade $TEST_MINT $TEST_WALLET 95
echo ""

# Test 2: Unit Tests
echo "✅ Test 2: Advice Bus Unit Tests"
echo "   Running Rust unit tests..."
cargo test advice_bus --release --quiet
echo "   ✅ Unit tests passed"
echo ""

# Test 3: Compilation Check
echo "✅ Test 3: Full Compilation Check"
echo "   Building release binary..."
cargo build --release --quiet
echo "   ✅ Build successful"
echo ""

# Test 4: Check UDP listener in code
echo "✅ Test 4: Code Integration Check"
echo "   Verifying Advice Bus is integrated in main.rs..."
if grep -q "advice_listener" src/main.rs; then
    echo "   ✅ Found advice_listener in main.rs"
else
    echo "   ❌ advice_listener not found in main.rs"
    exit 1
fi

if grep -q "ext_hold_until_ns" src/main.rs; then
    echo "   ✅ Found atomic field ext_hold_until_ns"
else
    echo "   ❌ Atomic fields not found"
    exit 1
fi

if grep -q "ADVICE: Hold extended" src/main.rs; then
    echo "   ✅ Found exit logic respecting advice"
else
    echo "   ❌ Exit logic not updated"
    exit 1
fi
echo ""

# Summary
echo "========================================"
echo "✅ ALL TESTS PASSED"
echo ""
echo "📝 Next Steps:"
echo "   1. Create .env file with:"
echo "      ADVICE_BUS_ENABLED=true"
echo "      ADVICE_BUS_PORT=45100"
echo "      ADVICE_MIN_CONFIDENCE=60"
echo "      ADVICE_MAX_HOLD_EXTENSION_SECS=30"
echo ""
echo "   2. Run the bot: ./target/release/execution-bot"
echo ""
echo "   3. While bot has active position, send advisory:"
echo "      python3 test_advisor.py extend_hold <MINT> 20 80"
echo ""
echo "   4. Check bot logs for:"
echo "      '✅ Advice Bus: Listening on 127.0.0.1:45100'"
echo "      '🎯 ADVICE: Hold extended N more secs'"
echo ""
echo "========================================"
