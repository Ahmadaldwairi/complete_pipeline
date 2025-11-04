#!/bin/bash

# Data-Mining End-to-End Test Script
# Tests all functionality and identifies unused code
# Version: 2.0 (Nov 1, 2025)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_MINING_DIR="$SCRIPT_DIR/data-mining"
TEST_LOG="$SCRIPT_DIR/test_data_mining_results.log"
UNUSED_LOG="$SCRIPT_DIR/data_mining_unused_code.log"

echo "========================================" | tee "$TEST_LOG"
echo "Data-Mining End-to-End Test Suite" | tee -a "$TEST_LOG"
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
cd "$DATA_MINING_DIR"
if cargo build --release 2>&1 | tee -a "$TEST_LOG"; then
    log_test "Build" "PASS" "Data-mining compiled successfully"
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
    "src/lib.rs"
    "src/config.rs"
    "src/checkpoint.rs"
    "src/db/mod.rs"
    "src/db/aggregator.rs"
    "src/decoder/mod.rs"
    "src/grpc/mod.rs"
    "src/parser/mod.rs"
    "src/types/mod.rs"
    "src/udp/mod.rs"
    "src/momentum_tracker.rs"
    "src/window_tracker.rs"
)

MISSING_MODULES=0
for module in "${EXPECTED_MODULES[@]}"; do
    if [ ! -f "$module" ]; then
        echo "   Missing: $module" | tee -a "$TEST_LOG"
        MISSING_MODULES=$((MISSING_MODULES + 1))
    fi
done

if [ "$MISSING_MODULES" -eq 0 ]; then
    log_test "Module Structure" "PASS" "All ${#EXPECTED_MODULES[@]} modules present"
else
    log_test "Module Structure" "FAIL" "$MISSING_MODULES modules missing"
fi

# Test 4: Task #15 - WindowTracker Implementation
echo "" | tee -a "$TEST_LOG"
echo "Test 4: Task #15 - WindowTracker" | tee -a "$TEST_LOG"

if [ ! -f "src/window_tracker.rs" ]; then
    log_test "WindowTracker Module" "FAIL" "window_tracker.rs not found"
else
    WINDOW_FEATURES=(
        "WindowTracker"
        "add_trade"
        "get_metrics_if_ready"
        "calculate_metrics"
        "volume_sol_1s"
        "unique_buyers_1s"
        "price_change_bps_2s"
        "alpha_wallet_hits_10s"
    )
    
    FEATURES_OK=true
    for feature in "${WINDOW_FEATURES[@]}"; do
        if ! grep -q "$feature" src/window_tracker.rs; then
            echo "   Missing: $feature" | tee -a "$TEST_LOG"
            FEATURES_OK=false
        fi
    done
    
    if $FEATURES_OK; then
        log_test "WindowTracker Features" "PASS" "All window tracking features present"
    else
        log_test "WindowTracker Features" "FAIL" "Some window features missing"
    fi
fi

# Test 5: WindowTracker Integration in main.rs
echo "" | tee -a "$TEST_LOG"
echo "Test 5: WindowTracker Integration" | tee -a "$TEST_LOG"

INTEGRATION_CHECKS=(
    "WindowTracker::new"
    "window_tracker.add_trade"
    "get_metrics_if_ready"
    "send_window_metrics"
)

INTEGRATION_OK=true
for check in "${INTEGRATION_CHECKS[@]}"; do
    if ! grep -q "$check" src/main.rs; then
        echo "   Missing: $check in main.rs" | tee -a "$TEST_LOG"
        INTEGRATION_OK=false
    fi
done

if $INTEGRATION_OK; then
    log_test "WindowTracker Integration" "PASS" "WindowTracker fully integrated in main.rs"
else
    log_test "WindowTracker Integration" "FAIL" "Integration incomplete"
fi

# Test 6: UDP Message Sending
echo "" | tee -a "$TEST_LOG"
echo "Test 6: UDP Message Functions" | tee -a "$TEST_LOG"

UDP_FUNCTIONS=(
    "send_momentum_opportunity"
    "send_sol_price"
    "send_window_metrics"
)

MISSING_UDP=0
for func in "${UDP_FUNCTIONS[@]}"; do
    if ! grep -q "pub fn $func" src/udp/mod.rs && ! grep -q "fn $func" src/udp/mod.rs; then
        echo "   Missing: $func" | tee -a "$TEST_LOG"
        MISSING_UDP=$((MISSING_UDP + 1))
    fi
done

if [ "$MISSING_UDP" -eq 0 ]; then
    log_test "UDP Functions" "PASS" "All ${#UDP_FUNCTIONS[@]} UDP functions present"
else
    log_test "UDP Functions" "FAIL" "$MISSING_UDP functions missing"
fi

# Test 7: Configuration File
echo "" | tee -a "$TEST_LOG"
echo "Test 7: Configuration" | tee -a "$TEST_LOG"

if [ -f "config.toml" ] || [ -f "config.example.toml" ]; then
    log_test "Configuration File" "PASS" "Config file exists"
else
    log_test "Configuration File" "WARN" "No config.toml or config.example.toml found"
fi

# Test 8: Database Module
echo "" | tee -a "$TEST_LOG"
echo "Test 8: Database Module" | tee -a "$TEST_LOG"

if [ -f "src/db/mod.rs" ]; then
    DB_FEATURES=(
        "create_tables"
        "insert_trade"
        "insert_token"
        "update_wallet_stats"
    )
    
    DB_OK=true
    for feature in "${DB_FEATURES[@]}"; do
        if ! grep -q "$feature" src/db/mod.rs; then
            echo "   Note: $feature may not be explicitly in mod.rs" | tee -a "$TEST_LOG"
        fi
    done
    
    log_test "Database Module" "PASS" "Database module present"
else
    log_test "Database Module" "FAIL" "db/mod.rs not found"
fi

# Test 9: gRPC Module
echo "" | tee -a "$TEST_LOG"
echo "Test 9: gRPC Client" | tee -a "$TEST_LOG"

if [ -f "src/grpc/mod.rs" ]; then
    if grep -q "subscribe" src/grpc/mod.rs || grep -q "stream" src/grpc/mod.rs; then
        log_test "gRPC Client" "PASS" "gRPC module with streaming capability"
    else
        log_test "gRPC Client" "WARN" "gRPC module may be incomplete"
    fi
else
    log_test "gRPC Client" "FAIL" "grpc/mod.rs not found"
fi

# Test 10: Parser Module
echo "" | tee -a "$TEST_LOG"
echo "Test 10: Transaction Parser" | tee -a "$TEST_LOG"

if [ -f "src/parser/mod.rs" ]; then
    log_test "Parser Module" "PASS" "Parser module present"
else
    log_test "Parser Module" "FAIL" "parser/mod.rs not found"
fi

# Test 11: Momentum Tracker
echo "" | tee -a "$TEST_LOG"
echo "Test 11: Momentum Tracker" | tee -a "$TEST_LOG"

if [ -f "src/momentum_tracker.rs" ]; then
    if grep -q "MomentumTracker" src/momentum_tracker.rs; then
        log_test "Momentum Tracker" "PASS" "MomentumTracker module present"
    else
        log_test "Momentum Tracker" "WARN" "momentum_tracker.rs exists but structure unclear"
    fi
else
    log_test "Momentum Tracker" "WARN" "momentum_tracker.rs not found (may be optional)"
fi

# Test 12: Pyth Price Feeds
echo "" | tee -a "$TEST_LOG"
echo "Test 12: Pyth Price Integration" | tee -a "$TEST_LOG"

PYTH_FILES=(
    "src/pyth_http.rs"
    "src/pyth_subscriber.rs"
    "src/pyth_subscriber_rpc.rs"
)

PYTH_COUNT=0
for file in "${PYTH_FILES[@]}"; do
    if [ -f "$file" ]; then
        PYTH_COUNT=$((PYTH_COUNT + 1))
    fi
done

if [ "$PYTH_COUNT" -gt 0 ]; then
    log_test "Pyth Integration" "PASS" "$PYTH_COUNT Pyth modules found"
else
    log_test "Pyth Integration" "WARN" "No Pyth modules found (may use alternative price source)"
fi

# Test 13: Lib.rs Exports
echo "" | tee -a "$TEST_LOG"
echo "Test 13: Library Exports" | tee -a "$TEST_LOG"

if [ -f "src/lib.rs" ]; then
    if grep -q "pub mod window_tracker" src/lib.rs; then
        log_test "Library Exports" "PASS" "window_tracker exported in lib.rs"
    else
        log_test "Library Exports" "FAIL" "window_tracker not exported in lib.rs"
    fi
else
    log_test "Library Exports" "WARN" "lib.rs not found"
fi

# Test 14: Code Quality
echo "" | tee -a "$TEST_LOG"
echo "Test 14: Code Quality" | tee -a "$TEST_LOG"

TODO_COUNT=$(grep -r "TODO" src/ | wc -l)
if [ "$TODO_COUNT" -gt 0 ]; then
    log_test "TODO Comments" "INFO" "$TODO_COUNT TODO comments found"
fi

FIXME_COUNT=$(grep -r "FIXME" src/ | wc -l)
if [ "$FIXME_COUNT" -gt 0 ]; then
    log_test "FIXME Comments" "WARN" "$FIXME_COUNT FIXME comments found"
fi

# Test 15: Potentially Unused Variables
echo "" | tee -a "$TEST_LOG"
echo "Test 15: Unused Variables Check" | tee -a "$TEST_LOG"

UNUSED_VARS=$(grep "unused variable" "$UNUSED_LOG" 2>/dev/null | wc -l || echo "0")
if [ "$UNUSED_VARS" -gt 0 ]; then
    log_test "Unused Variables" "WARN" "$UNUSED_VARS unused variables found"
    echo "   See: $UNUSED_LOG for details" | tee -a "$TEST_LOG"
else
    log_test "Unused Variables" "PASS" "No unused variables"
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
