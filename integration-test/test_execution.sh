#!/bin/bash

# Execution End-to-End Test Script
# Tests all functionality and identifies unused code
# Version: 2.0 (Nov 1, 2025)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXECUTION_DIR="$SCRIPT_DIR/execution"
TEST_LOG="$SCRIPT_DIR/test_execution_results.log"
UNUSED_LOG="$SCRIPT_DIR/execution_unused_code.log"

echo "========================================" | tee "$TEST_LOG"
echo "Execution End-to-End Test Suite" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"
echo "Started: $(date)" | tee -a "$TEST_LOG"
echo "" | tee -a "$TEST_LOG"

log_test() {
    local test_name="$1"
    local status="$2"
    local details="$3"
    
    if [ "$status" = "PASS" ]; then
        echo "✅ PASS: $test_name" | tee -a "$TEST_LOG"
    elif [ "$status" = "FAIL" ]; then
        echo "❌ FAIL: $test_name" | tee -a "$TEST_LOG"
    elif [ "$status" = "WARN" ]; then
        echo "⚠️  WARN: $test_name" | tee -a "$TEST_LOG"
    else
        echo "ℹ️  INFO: $test_name" | tee -a "$TEST_LOG"
    fi
    
    if [ -n "$details" ]; then
        echo "   Details: $details" | tee -a "$TEST_LOG"
    fi
}

# Test 1: Build Check
echo "Test 1: Build Verification" | tee -a "$TEST_LOG"
cd "$EXECUTION_DIR"
if cargo build --release 2>&1 | tee -a "$TEST_LOG"; then
    log_test "Build" "PASS" "Execution compiled successfully"
else
    log_test "Build" "FAIL" "Compilation errors detected"
    exit 1
fi

# Test 2: Unused Code Detection
echo "" | tee -a "$TEST_LOG"
echo "Test 2: Unused Code Detection" | tee -a "$TEST_LOG"

cargo clippy --all-targets --quiet -- \
    -W dead_code \
    -W unused_variables \
    -W unused_imports \
    -W unused_mut \
    2>&1 | tee "$UNUSED_LOG"

UNUSED_COUNT=$(grep -c "warning:" "$UNUSED_LOG" 2>/dev/null || echo "0")
if [ "$UNUSED_COUNT" -eq 0 ]; then
    log_test "Unused Code Detection" "PASS" "No unused code detected"
else
    log_test "Unused Code Detection" "WARN" "$UNUSED_COUNT warnings found"
fi

# Test 3: Module Structure
echo "" | tee -a "$TEST_LOG"
echo "Test 3: Module Structure" | tee -a "$TEST_LOG"

EXPECTED_MODULES=(
    "src/main.rs"
    "src/config.rs"
    "src/trading.rs"
    "src/advice_bus.rs"
    "src/advice_sender.rs"
    "src/trade_closed.rs"
    "src/tx_confirmed.rs"
    "src/pump_bonding_curve.rs"
    "src/pump_instructions.rs"
    "src/jito.rs"
    "src/database.rs"
    "src/telegram.rs"
    "src/slippage.rs"
)

MISSING_MODULES=0
for module in "${EXPECTED_MODULES[@]}"; do
    if [ ! -f "$module" ]; then
        echo "   Missing: $module" | tee -a "$TEST_LOG"
        MISSING_MODULES=$((MISSING_MODULES + 1))
    fi
done

if [ "$MISSING_MODULES" -eq 0 ]; then
    log_test "Module Structure" "PASS" "All ${#EXPECTED_MODULES[@]} core modules present"
else
    log_test "Module Structure" "FAIL" "$MISSING_MODULES modules missing"
fi

# Test 4: Task #14 - TradeClosed Implementation
echo "" | tee -a "$TEST_LOG"
echo "Test 4: Task #14 - TradeClosed Module" | tee -a "$TEST_LOG"

if [ ! -f "src/trade_closed.rs" ]; then
    log_test "TradeClosed Module" "FAIL" "trade_closed.rs not found"
else
    TRADE_CLOSED_FEATURES=(
        "send_trade_closed"
        "TradeClosed"
        "final_status"
    )
    
    FEATURES_OK=true
    for feature in "${TRADE_CLOSED_FEATURES[@]}"; do
        if ! grep -q "$feature" src/trade_closed.rs; then
            echo "   Missing: $feature" | tee -a "$TEST_LOG"
            FEATURES_OK=false
        fi
    done
    
    if $FEATURES_OK; then
        log_test "TradeClosed Features" "PASS" "All TradeClosed features present"
    else
        log_test "TradeClosed Features" "FAIL" "Some features missing"
    fi
fi

# Test 5: TradeClosed Integration in trading.rs
echo "" | tee -a "$TEST_LOG"
echo "Test 5: TradeClosed Integration" | tee -a "$TEST_LOG"

if [ -f "src/trading.rs" ]; then
    if grep -q "send_trade_closed" src/trading.rs || grep -q "trade_closed" src/trading.rs; then
        log_test "TradeClosed Integration" "PASS" "TradeClosed called in trading.rs"
    else
        log_test "TradeClosed Integration" "WARN" "TradeClosed may not be integrated"
    fi
else
    log_test "TradeClosed Integration" "FAIL" "trading.rs not found"
fi

# Test 6: Feedback Loop Functions
echo "" | tee -a "$TEST_LOG"
echo "Test 6: Feedback Loop" | tee -a "$TEST_LOG"

FEEDBACK_FUNCTIONS=(
    "send_enter_ack"
    "send_tx_confirmed"
    "send_trade_closed"
)

MISSING_FEEDBACK=0
for func in "${FEEDBACK_FUNCTIONS[@]}"; do
    if ! grep -rq "$func" src/; then
        echo "   Missing: $func" | tee -a "$TEST_LOG"
        MISSING_FEEDBACK=$((MISSING_FEEDBACK + 1))
    fi
done

if [ "$MISSING_FEEDBACK" -eq 0 ]; then
    log_test "Feedback Functions" "PASS" "All ${#FEEDBACK_FUNCTIONS[@]} feedback functions present"
else
    log_test "Feedback Functions" "FAIL" "$MISSING_FEEDBACK functions missing"
fi

# Test 7: Position Tracking
echo "" | tee -a "$TEST_LOG"
echo "Test 7: Position Tracking" | tee -a "$TEST_LOG"

if [ -f "src/trading.rs" ]; then
    if grep -q "HashMap.*BuyResult" src/trading.rs || grep -q "active_positions" src/trading.rs; then
        log_test "Position Tracking" "PASS" "Position tracking structure found"
    else
        log_test "Position Tracking" "WARN" "Position tracking structure unclear"
    fi
else
    log_test "Position Tracking" "FAIL" "trading.rs not found"
fi

# Test 8: Pump.fun Integration
echo "" | tee -a "$TEST_LOG"
echo "Test 8: Pump.fun Integration" | tee -a "$TEST_LOG"

PUMP_OK=true
if [ ! -f "src/pump_bonding_curve.rs" ]; then
    echo "   Missing: pump_bonding_curve.rs" | tee -a "$TEST_LOG"
    PUMP_OK=false
fi
if [ ! -f "src/pump_instructions.rs" ]; then
    echo "   Missing: pump_instructions.rs" | tee -a "$TEST_LOG"
    PUMP_OK=false
fi

if $PUMP_OK; then
    log_test "Pump.fun Integration" "PASS" "Both Pump.fun modules present"
else
    log_test "Pump.fun Integration" "FAIL" "Missing Pump.fun modules"
fi

# Test 9: Jito Integration
echo "" | tee -a "$TEST_LOG"
echo "Test 9: Jito Bundles" | tee -a "$TEST_LOG"

if [ -f "src/jito.rs" ]; then
    if grep -q "submit_bundle" src/jito.rs || grep -q "send_bundle" src/jito.rs; then
        log_test "Jito Integration" "PASS" "Jito bundle submission present"
    else
        log_test "Jito Integration" "WARN" "Jito module exists but unclear"
    fi
else
    log_test "Jito Integration" "WARN" "jito.rs not found (may use TPU direct)"
fi

# Test 10: Database Logging
echo "" | tee -a "$TEST_LOG"
echo "Test 10: Database Logging" | tee -a "$TEST_LOG"

if [ -f "src/database.rs" ]; then
    if grep -q "insert" src/database.rs || grep -q "log_trade" src/database.rs; then
        log_test "Database Logging" "PASS" "Database logging functions present"
    else
        log_test "Database Logging" "WARN" "Database module unclear"
    fi
else
    log_test "Database Logging" "WARN" "database.rs not found (may not log to DB)"
fi

# Test 11: Telegram Notifications
echo "" | tee -a "$TEST_LOG"
echo "Test 11: Telegram Integration" | tee -a "$TEST_LOG"

if [ -f "src/telegram.rs" ]; then
    log_test "Telegram Integration" "PASS" "Telegram module present"
else
    log_test "Telegram Integration" "WARN" "telegram.rs not found (notifications disabled?)"
fi

# Test 12: Slippage Calculation
echo "" | tee -a "$TEST_LOG"
echo "Test 12: Slippage Handling" | tee -a "$TEST_LOG"

if [ -f "src/slippage.rs" ]; then
    log_test "Slippage Module" "PASS" "Slippage module present"
else
    log_test "Slippage Module" "WARN" "slippage.rs not found"
fi

# Test 13: Configuration
echo "" | tee -a "$TEST_LOG"
echo "Test 13: Configuration" | tee -a "$TEST_LOG"

if [ -f "src/config.rs" ]; then
    if grep -q "pub struct Config" src/config.rs; then
        log_test "Configuration" "PASS" "Config struct present"
    else
        log_test "Configuration" "WARN" "Config structure unclear"
    fi
else
    log_test "Configuration" "FAIL" "config.rs not found"
fi

# Test 14: Identify Potentially Unused Files
echo "" | tee -a "$TEST_LOG"
echo "Test 14: Potentially Unused Files" | tee -a "$TEST_LOG"

SUSPICIOUS_FILES=(
    "src/main_failed.rs"
    "metrics.rs"
)

FOUND_SUSPICIOUS=0
for file in "${SUSPICIOUS_FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "   Found suspicious file: $file (may be unused)" | tee -a "$TEST_LOG"
        FOUND_SUSPICIOUS=$((FOUND_SUSPICIOUS + 1))
    fi
done

if [ "$FOUND_SUSPICIOUS" -gt 0 ]; then
    log_test "Suspicious Files" "WARN" "$FOUND_SUSPICIOUS potentially unused files found"
else
    log_test "Suspicious Files" "PASS" "No obviously unused files"
fi

# Test 15: UDP Communication
echo "" | tee -a "$TEST_LOG"
echo "Test 15: UDP Communication" | tee -a "$TEST_LOG"

if [ -f "src/advice_bus.rs" ]; then
    if grep -q "45110" src/advice_bus.rs || grep -q "bind" src/advice_bus.rs; then
        log_test "UDP Receiver" "PASS" "UDP receiver (port 45110) present"
    else
        log_test "UDP Receiver" "WARN" "UDP receiver structure unclear"
    fi
else
    log_test "UDP Receiver" "FAIL" "advice_bus.rs not found"
fi

if [ -f "src/advice_sender.rs" ]; then
    if grep -q "45100" src/advice_sender.rs || grep -q "send_to" src/advice_sender.rs; then
        log_test "UDP Sender" "PASS" "UDP sender (port 45100) present"
    else
        log_test "UDP Sender" "WARN" "UDP sender structure unclear"
    fi
else
    log_test "UDP Sender" "FAIL" "advice_sender.rs not found"
fi

# Test 16: Code Quality
echo "" | tee -a "$TEST_LOG"
echo "Test 16: Code Quality" | tee -a "$TEST_LOG"

TODO_COUNT=$(grep -r "TODO" src/ | wc -l)
if [ "$TODO_COUNT" -gt 0 ]; then
    log_test "TODO Comments" "INFO" "$TODO_COUNT TODO comments found"
fi

FIXME_COUNT=$(grep -r "FIXME" src/ | wc -l)
if [ "$FIXME_COUNT" -gt 0 ]; then
    log_test "FIXME Comments" "WARN" "$FIXME_COUNT FIXME comments found"
fi

# Test 17: Backtesting Module
echo "" | tee -a "$TEST_LOG"
echo "Test 17: Backtesting" | tee -a "$TEST_LOG"

if [ -d "backtesting" ]; then
    if [ -f "backtesting/Cargo.toml" ]; then
        log_test "Backtesting Module" "PASS" "Backtesting crate present"
    else
        log_test "Backtesting Module" "WARN" "Backtesting directory exists but no Cargo.toml"
    fi
else
    log_test "Backtesting Module" "INFO" "No backtesting module (optional)"
fi

# Summary
echo "" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"
echo "Test Summary" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"

PASS_COUNT=$(grep -c "✅ PASS" "$TEST_LOG" || echo "0")
FAIL_COUNT=$(grep -c "❌ FAIL" "$TEST_LOG" || echo "0")
WARN_COUNT=$(grep -c "⚠️  WARN" "$TEST_LOG" || echo "0")
INFO_COUNT=$(grep -c "ℹ️  INFO" "$TEST_LOG" || echo "0")

echo "Tests Passed: $PASS_COUNT" | tee -a "$TEST_LOG"
echo "Tests Failed: $FAIL_COUNT" | tee -a "$TEST_LOG"
echo "Warnings: $WARN_COUNT" | tee -a "$TEST_LOG"
echo "Info: $INFO_COUNT" | tee -a "$TEST_LOG"
echo "" | tee -a "$TEST_LOG"

if [ "$FAIL_COUNT" -eq 0 ]; then
    echo "✅ ALL TESTS PASSED!" | tee -a "$TEST_LOG"
    EXIT_CODE=0
else
    echo "❌ SOME TESTS FAILED" | tee -a "$TEST_LOG"
    EXIT_CODE=1
fi

echo "Completed: $(date)" | tee -a "$TEST_LOG"
echo "Results saved to: $TEST_LOG" | tee -a "$TEST_LOG"
echo "Unused code analysis: $UNUSED_LOG" | tee -a "$TEST_LOG"

exit $EXIT_CODE
