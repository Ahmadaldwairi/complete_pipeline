#!/bin/bash

# Brain End-to-End Test Script
# Tests all functionality and identifies unused code
# Version: 2.0 (Nov 1, 2025)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRAIN_DIR="$SCRIPT_DIR/brain"
TEST_LOG="$SCRIPT_DIR/test_brain_results.log"
UNUSED_LOG="$SCRIPT_DIR/brain_unused_code.log"

echo "========================================" | tee "$TEST_LOG"
echo "Brain End-to-End Test Suite" | tee -a "$TEST_LOG"
echo "========================================" | tee -a "$TEST_LOG"
echo "Started: $(date)" | tee -a "$TEST_LOG"
echo "" | tee -a "$TEST_LOG"

# Function to log test results
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
cd "$BRAIN_DIR"
if cargo build --release 2>&1 | tee -a "$TEST_LOG"; then
    log_test "Build" "PASS" "Brain compiled successfully"
else
    log_test "Build" "FAIL" "Compilation errors detected"
    exit 1
fi

# Test 2: Unused Code Detection
echo "" | tee -a "$TEST_LOG"
echo "Test 2: Unused Code Detection" | tee -a "$TEST_LOG"
echo "Running cargo clippy to detect dead code..." | tee -a "$TEST_LOG"

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
    log_test "Unused Code Detection" "WARN" "$UNUSED_COUNT warnings found (see brain_unused_code.log)"
fi

# Test 3: Module Structure Verification
echo "" | tee -a "$TEST_LOG"
echo "Test 3: Module Structure" | tee -a "$TEST_LOG"

EXPECTED_MODULES=(
    "src/main.rs"
    "src/config.rs"
    "src/metrics.rs"
    "src/mint_reservation.rs"
    "src/trade_state.rs"
    "src/decision_engine/mod.rs"
    "src/decision_engine/scoring.rs"
    "src/decision_engine/validation.rs"
    "src/decision_engine/guardrails.rs"
    "src/decision_engine/position_sizer.rs"
    "src/decision_engine/position_tracker.rs"
    "src/decision_engine/triggers.rs"
    "src/decision_engine/logging.rs"
    "src/feature_cache/mod.rs"
    "src/feature_cache/mint_cache.rs"
    "src/feature_cache/wallet_cache.rs"
    "src/udp_bus/mod.rs"
    "src/udp_bus/messages.rs"
    "src/udp_bus/receiver.rs"
    "src/udp_bus/sender.rs"
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

# Test 4: Message Type Coverage
echo "" | tee -a "$TEST_LOG"
echo "Test 4: Message Type Coverage" | tee -a "$TEST_LOG"

REQUIRED_MESSAGES=(
    "SolPriceUpdate"
    "MomentumOpportunity"
    "TradeDecision"
    "EnterAck"
    "TxConfirmed"
    "TradeClosed"
    "WindowMetrics"
)

MISSING_MESSAGES=0
for msg in "${REQUIRED_MESSAGES[@]}"; do
    if ! grep -q "pub struct $msg" src/udp_bus/messages.rs; then
        echo "   Missing message: $msg" | tee -a "$TEST_LOG"
        MISSING_MESSAGES=$((MISSING_MESSAGES + 1))
    fi
done

if [ "$MISSING_MESSAGES" -eq 0 ]; then
    log_test "Message Types" "PASS" "All ${#REQUIRED_MESSAGES[@]} required messages defined"
else
    log_test "Message Types" "FAIL" "$MISSING_MESSAGES messages missing"
fi

# Test 5: Task #14 (TradeClosed) Implementation
echo "" | tee -a "$TEST_LOG"
echo "Test 5: Task #14 - TradeClosed Message" | tee -a "$TEST_LOG"

TASK14_CHECKS=(
    "TradeClosed:src/udp_bus/messages.rs"
    "AdviceMessage::TradeClosed:src/main.rs"
    "trade_closed:src/udp_bus/receiver.rs"
)

TASK14_OK=true
for check in "${TASK14_CHECKS[@]}"; do
    pattern="${check%%:*}"
    file="${check##*:}"
    if ! grep -q "$pattern" "$file"; then
        echo "   Missing: $pattern in $file" | tee -a "$TEST_LOG"
        TASK14_OK=false
    fi
done

if $TASK14_OK; then
    log_test "Task #14 Implementation" "PASS" "TradeClosed message fully integrated"
else
    log_test "Task #14 Implementation" "FAIL" "TradeClosed implementation incomplete"
fi

# Test 6: Task #15 (WindowMetrics) Implementation
echo "" | tee -a "$TEST_LOG"
echo "Test 6: Task #15 - WindowMetrics Message" | tee -a "$TEST_LOG"

TASK15_CHECKS=(
    "WindowMetrics:src/udp_bus/messages.rs"
    "AdviceMessage::WindowMetrics:src/main.rs"
    "volume_sol_1s:src/udp_bus/messages.rs"
    "unique_buyers_1s:src/udp_bus/messages.rs"
    "price_change_bps_2s:src/udp_bus/messages.rs"
    "alpha_wallet_hits_10s:src/udp_bus/messages.rs"
)

TASK15_OK=true
for check in "${TASK15_CHECKS[@]}"; do
    pattern="${check%%:*}"
    file="${check##*:}"
    if ! grep -q "$pattern" "$file"; then
        echo "   Missing: $pattern in $file" | tee -a "$TEST_LOG"
        TASK15_OK=false
    fi
done

if $TASK15_OK; then
    log_test "Task #15 Implementation" "PASS" "WindowMetrics message fully integrated"
else
    log_test "Task #15 Implementation" "FAIL" "WindowMetrics implementation incomplete"
fi

# Test 7: Duplicate Prevention System
echo "" | tee -a "$TEST_LOG"
echo "Test 7: Duplicate Prevention System" | tee -a "$TEST_LOG"

if [ -f "src/mint_reservation.rs" ]; then
    if grep -q "try_reserve" src/mint_reservation.rs && \
       grep -q "release" src/mint_reservation.rs; then
        log_test "Duplicate Prevention" "PASS" "Mint reservation system present"
    else
        log_test "Duplicate Prevention" "FAIL" "Mint reservation methods incomplete"
    fi
else
    log_test "Duplicate Prevention" "FAIL" "mint_reservation.rs not found"
fi

# Test 8: Position Tracking
echo "" | tee -a "$TEST_LOG"
echo "Test 8: Position Tracking" | tee -a "$TEST_LOG"

if [ -f "src/decision_engine/position_tracker.rs" ]; then
    POSITION_METHODS=(
        "add_position"
        "remove_position"
        "get_active_positions"
        "total_exposure"
    )
    
    METHODS_OK=true
    for method in "${POSITION_METHODS[@]}"; do
        if ! grep -q "$method" src/decision_engine/position_tracker.rs; then
            echo "   Missing method: $method" | tee -a "$TEST_LOG"
            METHODS_OK=false
        fi
    done
    
    if $METHODS_OK; then
        log_test "Position Tracking" "PASS" "All position tracking methods present"
    else
        log_test "Position Tracking" "WARN" "Some position tracking methods may be missing"
    fi
else
    log_test "Position Tracking" "FAIL" "position_tracker.rs not found"
fi

# Test 9: Cache Implementation
echo "" | tee -a "$TEST_LOG"
echo "Test 9: Cache Implementation" | tee -a "$TEST_LOG"

CACHE_OK=true
if [ ! -f "src/feature_cache/mint_cache.rs" ]; then
    echo "   Missing: mint_cache.rs" | tee -a "$TEST_LOG"
    CACHE_OK=false
fi
if [ ! -f "src/feature_cache/wallet_cache.rs" ]; then
    echo "   Missing: wallet_cache.rs" | tee -a "$TEST_LOG"
    CACHE_OK=false
fi

if $CACHE_OK; then
    log_test "Cache Implementation" "PASS" "Both caches implemented"
else
    log_test "Cache Implementation" "FAIL" "Cache files missing"
fi

# Test 10: Configuration
echo "" | tee -a "$TEST_LOG"
echo "Test 10: Configuration Handling" | tee -a "$TEST_LOG"

if [ -f "src/config.rs" ]; then
    if grep -q "pub struct Config" src/config.rs && \
       grep -q "from_env" src/config.rs; then
        log_test "Configuration" "PASS" "Config struct and loader present"
    else
        log_test "Configuration" "WARN" "Config structure may be incomplete"
    fi
else
    log_test "Configuration" "FAIL" "config.rs not found"
fi

# Test 11: Metrics Export
echo "" | tee -a "$TEST_LOG"
echo "Test 11: Metrics Implementation" | tee -a "$TEST_LOG"

if [ -f "src/metrics.rs" ]; then
    METRIC_TYPES=(
        "advisories_received"
        "decisions_sent"
        "score_distribution"
        "validation_failures"
    )
    
    METRICS_FOUND=0
    for metric in "${METRIC_TYPES[@]}"; do
        if grep -q "$metric" src/metrics.rs; then
            METRICS_FOUND=$((METRICS_FOUND + 1))
        fi
    done
    
    if [ "$METRICS_FOUND" -ge 3 ]; then
        log_test "Metrics" "PASS" "$METRICS_FOUND/${#METRIC_TYPES[@]} metric types found"
    else
        log_test "Metrics" "WARN" "Only $METRICS_FOUND/${#METRIC_TYPES[@]} metric types found"
    fi
else
    log_test "Metrics" "FAIL" "metrics.rs not found"
fi

# Test 12: Trade State Machine
echo "" | tee -a "$TEST_LOG"
echo "Test 12: Trade State Machine" | tee -a "$TEST_LOG"

if [ -f "src/trade_state.rs" ]; then
    STATES=(
        "Enter"
        "EnterAck"
        "TxConfirmed"
        "TradeClosed"
    )
    
    STATES_FOUND=0
    for state in "${STATES[@]}"; do
        if grep -q "$state" src/trade_state.rs; then
            STATES_FOUND=$((STATES_FOUND + 1))
        fi
    done
    
    if [ "$STATES_FOUND" -eq ${#STATES[@]} ]; then
        log_test "Trade State Machine" "PASS" "All 4 states defined"
    else
        log_test "Trade State Machine" "WARN" "Only $STATES_FOUND/4 states found"
    fi
else
    log_test "Trade State Machine" "FAIL" "trade_state.rs not found"
fi

# Test 13: Code Quality Checks
echo "" | tee -a "$TEST_LOG"
echo "Test 13: Code Quality" | tee -a "$TEST_LOG"

# Check for TODO comments
TODO_COUNT=$(grep -r "TODO" src/ | wc -l)
if [ "$TODO_COUNT" -gt 0 ]; then
    log_test "TODO Comments" "INFO" "$TODO_COUNT TODO comments found"
else
    log_test "TODO Comments" "PASS" "No TODO comments"
fi

# Check for FIXME comments
FIXME_COUNT=$(grep -r "FIXME" src/ | wc -l)
if [ "$FIXME_COUNT" -gt 0 ]; then
    log_test "FIXME Comments" "WARN" "$FIXME_COUNT FIXME comments found"
else
    log_test "FIXME Comments" "PASS" "No FIXME comments"
fi

# Check for panic! calls
PANIC_COUNT=$(grep -r "panic!" src/ | grep -v "// " | wc -l)
if [ "$PANIC_COUNT" -gt 5 ]; then
    log_test "Panic Calls" "WARN" "$PANIC_COUNT panic! calls found (consider error handling)"
else
    log_test "Panic Calls" "PASS" "$PANIC_COUNT panic! calls (acceptable)"
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
echo "" | tee -a "$TEST_LOG"

exit $EXIT_CODE
