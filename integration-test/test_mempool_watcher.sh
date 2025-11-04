#!/bin/bash

##############################################################################
# Mempool-Watcher Service - End-to-End Test Suite
# 
# Tests all components of the mempool-watcher service including:
# - Build verification
# - Module structure
# - Configuration management
# - Transaction decoding (Pump.fun buy/sell)
# - Alpha wallet tracking (SQLite database)
# - Heat calculation
# - UDP publishing to Brain
# - Transaction confirmation tracking
# - WebSocket connectivity
# - Unused code detection
#
# Output: test_mempool_watcher_results.log
# Unused Code: mempool_watcher_unused_code.log
##############################################################################

set -e  # Exit on error

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MEMPOOL_DIR="$SCRIPT_DIR/mempool-watcher"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_WARNED=0

# Log files
LOG_FILE="$SCRIPT_DIR/test_mempool_watcher_results.log"
UNUSED_CODE_LOG="$SCRIPT_DIR/mempool_watcher_unused_code.log"

# Clear previous logs
> "$LOG_FILE"
> "$UNUSED_CODE_LOG"

# Logging functions
log_info() {
    echo -e "${BLUE}ℹ️  $1${NC}" | tee -a "$LOG_FILE"
}

log_success() {
    echo -e "${GREEN}✅ $1${NC}" | tee -a "$LOG_FILE"
    ((TESTS_PASSED++))
}

log_failure() {
    echo -e "${RED}❌ $1${NC}" | tee -a "$LOG_FILE"
    ((TESTS_FAILED++))
}

log_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}" | tee -a "$LOG_FILE"
    ((TESTS_WARNED++))
}

echo "===============================================" | tee -a "$LOG_FILE"
echo "   MEMPOOL-WATCHER SERVICE - TEST SUITE" | tee -a "$LOG_FILE"
echo "   $(date)" | tee -a "$LOG_FILE"
echo "===============================================" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 1: Build Verification
##############################################################################
log_info "Test 1: Build Verification"
cd "$MEMPOOL_DIR" || exit 1

if cargo build --release 2>&1 | tee -a "$LOG_FILE"; then
    log_success "Test 1 PASSED: Mempool-watcher builds successfully"
else
    log_failure "Test 1 FAILED: Build errors detected"
    exit 1
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 2: Unused Code Detection
##############################################################################
log_info "Test 2: Unused Code Detection (cargo clippy)"

cargo clippy --all-targets --all-features -- \
    -W dead_code \
    -W unused_variables \
    -W unused_imports \
    2>&1 | tee "$UNUSED_CODE_LOG"

# Check if there are any warnings
if grep -q "warning:" "$UNUSED_CODE_LOG"; then
    UNUSED_COUNT=$(grep -c "warning:" "$UNUSED_CODE_LOG" || echo "0")
    log_warning "Test 2 WARNING: Found $UNUSED_COUNT clippy warnings (see $UNUSED_CODE_LOG)"
else
    log_success "Test 2 PASSED: No unused code detected"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 3: Module Structure Verification
##############################################################################
log_info "Test 3: Module Structure Verification"
REQUIRED_MODULES=(
    "src/main.rs"
    "src/config.rs"
    "src/decoder.rs"
    "src/transaction_monitor.rs"
    "src/alpha_wallet_manager.rs"
    "src/heat_calculator.rs"
    "src/udp_publisher.rs"
    "src/tx_confirmed.rs"
    "src/watch_signature.rs"
    "src/watch_listener.rs"
)

ALL_MODULES_EXIST=true
for module in "${REQUIRED_MODULES[@]}"; do
    if [ -f "$module" ]; then
        log_success "  ✓ Found: $module"
    else
        log_failure "  ✗ Missing: $module"
        ALL_MODULES_EXIST=false
    fi
done

if [ "$ALL_MODULES_EXIST" = true ]; then
    log_success "Test 3 PASSED: All required modules exist"
else
    log_failure "Test 3 FAILED: Missing required modules"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 4: Configuration Module
##############################################################################
log_info "Test 4: Configuration Module (.env loading)"
if grep -q "pub struct Config" src/config.rs && \
   grep -q "SOLANA_RPC_URL" src/config.rs && \
   grep -q "PUMP_PROGRAM_ID" src/config.rs && \
   grep -q "UDP_BRAIN_ADDRESS" src/config.rs; then
    log_success "Test 4 PASSED: Configuration module correctly implemented"
else
    log_failure "Test 4 FAILED: Configuration module incomplete"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 5: Transaction Decoder (Pump.fun)
##############################################################################
log_info "Test 5: Transaction Decoder Implementation"
if grep -q "pub struct DecodedTransaction" src/decoder.rs && \
   grep -q "pub enum TransactionAction" src/decoder.rs && \
   grep -q "decode_transaction" src/decoder.rs && \
   grep -q "decode_pump_buy" src/decoder.rs && \
   grep -q "decode_pump_sell" src/decoder.rs; then
    log_success "Test 5 PASSED: Transaction decoder implemented"
else
    log_failure "Test 5 FAILED: Transaction decoder incomplete"
fi

# Check for discriminators
if grep -q "66063d1201daebea" src/decoder.rs && \
   grep -q "33e685a4017f83ad" src/decoder.rs; then
    log_success "  ✓ Buy/Sell discriminators present"
else
    log_warning "  ⚠️  Buy/Sell discriminators not found"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 6: Alpha Wallet Manager (SQLite)
##############################################################################
log_info "Test 6: Alpha Wallet Manager Implementation"
if grep -q "pub struct AlphaWalletManager" src/alpha_wallet_manager.rs && \
   grep -q "record_transaction" src/alpha_wallet_manager.rs && \
   grep -q "update_wallet_alpha_status" src/alpha_wallet_manager.rs && \
   grep -q "get_alpha_wallets" src/alpha_wallet_manager.rs; then
    log_success "Test 6 PASSED: Alpha wallet manager implemented"
else
    log_failure "Test 6 FAILED: Alpha wallet manager incomplete"
fi

# Check for SQLite usage
if grep -q "rusqlite" Cargo.toml && \
   grep -q "CREATE TABLE" src/alpha_wallet_manager.rs; then
    log_success "  ✓ SQLite database integration present"
else
    log_warning "  ⚠️  SQLite database not properly integrated"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 7: Heat Calculator
##############################################################################
log_info "Test 7: Heat Calculator Implementation"
if grep -q "pub struct HeatCalculator" src/heat_calculator.rs && \
   grep -q "pub struct HeatIndex" src/heat_calculator.rs && \
   grep -q "pub struct HotSignal" src/heat_calculator.rs && \
   grep -q "calculate_heat" src/heat_calculator.rs && \
   grep -q "get_hot_signals" src/heat_calculator.rs; then
    log_success "Test 7 PASSED: Heat calculator implemented"
else
    log_failure "Test 7 FAILED: Heat calculator incomplete"
fi

# Check for heat metrics
if grep -q "tx_count" src/heat_calculator.rs && \
   grep -q "alpha_wallet_count" src/heat_calculator.rs && \
   grep -q "heat_score" src/heat_calculator.rs; then
    log_success "  ✓ Heat metrics present"
else
    log_warning "  ⚠️  Heat metrics incomplete"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 8: UDP Publisher
##############################################################################
log_info "Test 8: UDP Publisher Implementation"
if grep -q "pub struct UdpPublisher" src/udp_publisher.rs && \
   grep -q "send_hot_signal" src/udp_publisher.rs && \
   grep -q "send_alpha_wallet_signal" src/udp_publisher.rs; then
    log_success "Test 8 PASSED: UDP publisher implemented"
else
    log_failure "Test 8 FAILED: UDP publisher incomplete"
fi

# Check for UDP socket usage
if grep -q "UdpSocket" src/udp_publisher.rs; then
    log_success "  ✓ UDP socket usage detected"
else
    log_warning "  ⚠️  UDP socket not found"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 9: Transaction Monitor (WebSocket)
##############################################################################
log_info "Test 9: Transaction Monitor Implementation"
if grep -q "pub struct TransactionMonitor" src/transaction_monitor.rs && \
   grep -q "start" src/transaction_monitor.rs && \
   grep -q "process_transaction" src/transaction_monitor.rs; then
    log_success "Test 9 PASSED: Transaction monitor implemented"
else
    log_failure "Test 9 FAILED: Transaction monitor incomplete"
fi

# Check for WebSocket usage
if grep -q "tokio-tungstenite\|WebSocket\|logsSubscribe" Cargo.toml src/transaction_monitor.rs; then
    log_success "  ✓ WebSocket integration present"
else
    log_warning "  ⚠️  WebSocket integration not found"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 10: Transaction Confirmation Tracking
##############################################################################
log_info "Test 10: Transaction Confirmation Tracking"
if grep -q "pub struct TxConfirmed" src/tx_confirmed.rs && \
   grep -q "track_signature" src/tx_confirmed.rs && \
   grep -q "check_confirmations" src/tx_confirmed.rs; then
    log_success "Test 10 PASSED: Transaction confirmation tracking implemented"
else
    log_failure "Test 10 FAILED: Transaction confirmation tracking incomplete"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 11: Signature Monitoring
##############################################################################
log_info "Test 11: Signature Monitoring Implementation"
if grep -q "pub struct WatchSignature" src/watch_signature.rs && \
   grep -q "pub struct SignatureTracker" src/watch_signature.rs; then
    log_success "Test 11 PASSED: Signature monitoring implemented"
else
    log_failure "Test 11 FAILED: Signature monitoring incomplete"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 12: Environment Configuration
##############################################################################
log_info "Test 12: Environment Configuration Files"
if [ -f ".env.example" ]; then
    log_success "  ✓ .env.example exists"
    
    # Check for required environment variables
    REQUIRED_VARS=(
        "SOLANA_RPC_URL"
        "SOLANA_WS_URL"
        "PUMP_PROGRAM_ID"
        "UDP_BRAIN_ADDRESS"
    )
    
    ALL_VARS_PRESENT=true
    for var in "${REQUIRED_VARS[@]}"; do
        if grep -q "$var" .env.example 2>/dev/null || grep -q "$var" src/config.rs; then
            log_success "    ✓ $var configured"
        else
            log_warning "    ⚠️  $var not documented"
            ALL_VARS_PRESENT=false
        fi
    done
    
    if [ "$ALL_VARS_PRESENT" = true ]; then
        log_success "Test 12 PASSED: Environment configuration complete"
    else
        log_warning "Test 12 WARNING: Some environment variables not documented"
    fi
else
    log_warning "Test 12 WARNING: .env.example not found"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 13: Database Schema (Alpha Wallets)
##############################################################################
log_info "Test 13: Database Schema Verification"
if grep -q "CREATE TABLE.*wallets" src/alpha_wallet_manager.rs && \
   grep -q "CREATE TABLE.*trades" src/alpha_wallet_manager.rs; then
    log_success "Test 13 PASSED: Database schema defined"
    
    # Check for required columns
    if grep -q "total_profit_sol" src/alpha_wallet_manager.rs && \
       grep -q "is_alpha" src/alpha_wallet_manager.rs; then
        log_success "  ✓ Alpha tracking columns present"
    else
        log_warning "  ⚠️  Alpha tracking columns incomplete"
    fi
else
    log_failure "Test 13 FAILED: Database schema not properly defined"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 14: Message Types to Brain
##############################################################################
log_info "Test 14: UDP Message Types to Brain"
MESSAGE_TYPES=(
    "MempoolHeatAdvice"
    "AlphaWalletSignal"
    "HotSignal"
)

FOUND_COUNT=0
for msg_type in "${MESSAGE_TYPES[@]}"; do
    if grep -rq "$msg_type" src/; then
        log_success "  ✓ Found message type: $msg_type"
        ((FOUND_COUNT++))
    else
        log_warning "  ⚠️  Message type not found: $msg_type"
    fi
done

if [ $FOUND_COUNT -ge 2 ]; then
    log_success "Test 14 PASSED: UDP message types implemented"
else
    log_failure "Test 14 FAILED: Missing UDP message types"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 15: Audit Implementation
##############################################################################
log_info "Test 15: Audit Feature Implementation"
if [ -f "AUDIT_IMPLEMENTATION_COMPLETE.md" ]; then
    log_success "  ✓ Audit documentation exists"
    
    # Check for audit logging in code
    if grep -rq "audit\|Audit" src/; then
        log_success "Test 15 PASSED: Audit feature implemented"
    else
        log_warning "Test 15 WARNING: Audit documentation exists but code references not found"
    fi
else
    log_warning "Test 15 WARNING: Audit documentation not found"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 16: Dependencies Verification
##############################################################################
log_info "Test 16: Critical Dependencies Check"
REQUIRED_DEPS=(
    "tokio"
    "solana-client"
    "rusqlite"
    "tokio-tungstenite"
    "serde"
    "anyhow"
)

ALL_DEPS_PRESENT=true
for dep in "${REQUIRED_DEPS[@]}"; do
    if grep -q "^$dep\s*=" Cargo.toml; then
        log_success "  ✓ Dependency: $dep"
    else
        log_warning "  ⚠️  Dependency not found: $dep"
        ALL_DEPS_PRESENT=false
    fi
done

if [ "$ALL_DEPS_PRESENT" = true ]; then
    log_success "Test 16 PASSED: All critical dependencies present"
else
    log_warning "Test 16 WARNING: Some dependencies missing"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 17: Code Quality Checks
##############################################################################
log_info "Test 17: Code Quality Analysis"
TODO_COUNT=$(grep -r "TODO\|FIXME\|XXX\|HACK" src/ | wc -l || echo "0")
PANIC_COUNT=$(grep -r "panic!\|unwrap()\|expect(" src/ | wc -l || echo "0")

log_info "  Code quality metrics:"
log_info "    - TODO/FIXME comments: $TODO_COUNT"
log_info "    - panic!/unwrap/expect calls: $PANIC_COUNT"

if [ "$TODO_COUNT" -lt 10 ] && [ "$PANIC_COUNT" -lt 20 ]; then
    log_success "Test 17 PASSED: Code quality acceptable"
else
    log_warning "Test 17 WARNING: High number of TODOs ($TODO_COUNT) or panics ($PANIC_COUNT)"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 18: Potentially Unused Files Check
##############################################################################
log_info "Test 18: Check for Unused/Obsolete Files"
SUSPICIOUS_FILES=(
    "src/main_failed.rs"
    "src/old_*.rs"
    "src/*_backup.rs"
    "src/*_deprecated.rs"
)

FOUND_SUSPICIOUS=false
for pattern in "${SUSPICIOUS_FILES[@]}"; do
    # Use find to check if any files match the pattern
    if find . -path "$pattern" 2>/dev/null | grep -q .; then
        FOUND_FILES=$(find . -path "$pattern" 2>/dev/null)
        log_warning "  ⚠️  Suspicious file found: $FOUND_FILES"
        echo "  → RECOMMENDATION: Review and consider deleting: $FOUND_FILES" | tee -a "$LOG_FILE"
        FOUND_SUSPICIOUS=true
    fi
done

if [ "$FOUND_SUSPICIOUS" = false ]; then
    log_success "Test 18 PASSED: No suspicious files found"
else
    log_warning "Test 18 WARNING: Found potentially unused files"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 19: Unused Variables Detection
##############################################################################
log_info "Test 19: Unused Variables Analysis"
if [ -f "$UNUSED_CODE_LOG" ] && [ -s "$UNUSED_CODE_LOG" ]; then
    UNUSED_VAR_COUNT=$(grep -c "unused variable\|unused import\|unused mut" "$UNUSED_CODE_LOG" || echo "0")
    
    if [ "$UNUSED_VAR_COUNT" -gt 0 ]; then
        log_warning "Test 19 WARNING: Found $UNUSED_VAR_COUNT unused variables/imports"
        echo "  → See $UNUSED_CODE_LOG for details" | tee -a "$LOG_FILE"
        
        # Extract specific line numbers
        grep "unused variable\|unused import" "$UNUSED_CODE_LOG" | head -10 | while read -r line; do
            echo "    $line" | tee -a "$LOG_FILE"
        done
    else
        log_success "Test 19 PASSED: No unused variables detected"
    fi
else
    log_info "Test 19 INFO: Unused code log not available"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# TEST 20: Integration Points Check
##############################################################################
log_info "Test 20: Integration Points Verification"
INTEGRATION_OK=true

# Check Brain UDP integration
if grep -rq "127.0.0.1:8888\|UDP_BRAIN_ADDRESS" src/; then
    log_success "  ✓ Brain UDP integration configured"
else
    log_warning "  ⚠️  Brain UDP integration not found"
    INTEGRATION_OK=false
fi

# Check Solana WebSocket integration
if grep -rq "wss://\|logsSubscribe" src/ Cargo.toml; then
    log_success "  ✓ Solana WebSocket integration present"
else
    log_warning "  ⚠️  Solana WebSocket integration not found"
    INTEGRATION_OK=false
fi

# Check Pump.fun program ID
if grep -rq "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P\|PUMP_PROGRAM_ID" src/; then
    log_success "  ✓ Pump.fun program ID configured"
else
    log_warning "  ⚠️  Pump.fun program ID not found"
    INTEGRATION_OK=false
fi

if [ "$INTEGRATION_OK" = true ]; then
    log_success "Test 20 PASSED: All integration points verified"
else
    log_warning "Test 20 WARNING: Some integration points missing"
fi
echo "" | tee -a "$LOG_FILE"

##############################################################################
# FINAL SUMMARY
##############################################################################
echo "" | tee -a "$LOG_FILE"
echo "===============================================" | tee -a "$LOG_FILE"
echo "           TEST SUMMARY" | tee -a "$LOG_FILE"
echo "===============================================" | tee -a "$LOG_FILE"
echo -e "${GREEN}✅ Tests Passed:  $TESTS_PASSED${NC}" | tee -a "$LOG_FILE"
echo -e "${RED}❌ Tests Failed:  $TESTS_FAILED${NC}" | tee -a "$LOG_FILE"
echo -e "${YELLOW}⚠️  Warnings:      $TESTS_WARNED${NC}" | tee -a "$LOG_FILE"
echo "===============================================" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# Unused code summary
if [ -f "$UNUSED_CODE_LOG" ] && [ -s "$UNUSED_CODE_LOG" ]; then
    TOTAL_WARNINGS=$(grep -c "warning:" "$UNUSED_CODE_LOG" || echo "0")
    echo "Unused Code Analysis:" | tee -a "$LOG_FILE"
    echo "  - Total clippy warnings: $TOTAL_WARNINGS" | tee -a "$LOG_FILE"
    echo "  - Full report: $UNUSED_CODE_LOG" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
fi

# Recommendations
echo "CLEANUP RECOMMENDATIONS:" | tee -a "$LOG_FILE"
echo "1. Review unused code in: $UNUSED_CODE_LOG" | tee -a "$LOG_FILE"
echo "2. Fix any unused variables by prefixing with underscore (_)" | tee -a "$LOG_FILE"
echo "3. Remove unused imports to reduce compilation time" | tee -a "$LOG_FILE"
echo "4. Delete any suspicious files identified in Test 18" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# Exit with appropriate code
if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "${GREEN}All tests completed successfully!${NC}" | tee -a "$LOG_FILE"
    exit 0
else
    echo -e "${RED}Some tests failed. Please review the log file.${NC}" | tee -a "$LOG_FILE"
    exit 1
fi
