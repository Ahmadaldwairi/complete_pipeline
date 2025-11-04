#!/bin/bash

# Master Test Runner
# Runs all end-to-end tests and generates cleanup report
# Version: 1.0 (Nov 1, 2025)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MASTER_LOG="$SCRIPT_DIR/master_test_results.log"

echo "==========================================" | tee "$MASTER_LOG"
echo "Master Test Suite - All Services" | tee -a "$MASTER_LOG"
echo "==========================================" | tee -a "$MASTER_LOG"
echo "Started: $(date)" | tee -a "$MASTER_LOG"
echo "" | tee -a "$MASTER_LOG"

# Run Brain tests
echo "========== BRAIN TESTS ==========" | tee -a "$MASTER_LOG"
if ./test_brain.sh; then
    echo "✅ Brain tests PASSED" | tee -a "$MASTER_LOG"
    BRAIN_STATUS="PASS"
else
    echo "❌ Brain tests FAILED" | tee -a "$MASTER_LOG"
    BRAIN_STATUS="FAIL"
fi
echo "" | tee -a "$MASTER_LOG"

# Run Data-Mining tests
echo "========== DATA-MINING TESTS ==========" | tee -a "$MASTER_LOG"
if ./test_data_mining.sh; then
    echo "✅ Data-mining tests PASSED" | tee -a "$MASTER_LOG"
    DATA_MINING_STATUS="PASS"
else
    echo "❌ Data-mining tests FAILED" | tee -a "$MASTER_LOG"
    DATA_MINING_STATUS="FAIL"
fi
echo "" | tee -a "$MASTER_LOG"

# Run Execution tests
echo "========== EXECUTION TESTS ==========" | tee -a "$MASTER_LOG"
if ./test_execution.sh; then
    echo "✅ Execution tests PASSED" | tee -a "$MASTER_LOG"
    EXECUTION_STATUS="PASS"
else
    echo "❌ Execution tests FAILED" | tee -a "$MASTER_LOG"
    EXECUTION_STATUS="FAIL"
fi
echo "" | tee -a "$MASTER_LOG"

# Run Mempool-Watcher tests
echo "========== MEMPOOL-WATCHER TESTS ==========" | tee -a "$MASTER_LOG"
if ./test_mempool_watcher.sh; then
    echo "✅ Mempool-watcher tests PASSED" | tee -a "$MASTER_LOG"
    MEMPOOL_STATUS="PASS"
else
    echo "❌ Mempool-watcher tests FAILED" | tee -a "$MASTER_LOG"
    MEMPOOL_STATUS="FAIL"
fi
echo "" | tee -a "$MASTER_LOG"

# Generate cleanup report
echo "========== GENERATING CLEANUP REPORT ==========" | tee -a "$MASTER_LOG"

cat > CLEANUP_RECOMMENDATIONS.md << 'EOF'
# Code Cleanup Recommendations

**Generated**: $(date)
**Purpose**: Identify unused code, variables, and files that can be safely removed

---

## Summary

This report consolidates findings from all three service test scripts:
- test_brain.sh → brain_unused_code.log
- test_data_mining.sh → data_mining_unused_code.log
- test_execution.sh → execution_unused_code.log

---

## Brain Service

### Unused Code Warnings

EOF

if [ -f "brain_unused_code.log" ]; then
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    cat brain_unused_code.log >> CLEANUP_RECOMMENDATIONS.md
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    echo "" >> CLEANUP_RECOMMENDATIONS.md
    
    BRAIN_WARNINGS=$(grep -c "warning:" brain_unused_code.log 2>/dev/null || echo "0")
    echo "**Total warnings**: $BRAIN_WARNINGS" >> CLEANUP_RECOMMENDATIONS.md
else
    echo "No unused code log found for Brain." >> CLEANUP_RECOMMENDATIONS.md
fi

cat >> CLEANUP_RECOMMENDATIONS.md << 'EOF'

### Recommendations

1. **Padding fields in messages.rs**: These are intentional for fixed-size UDP packets - DO NOT REMOVE
2. **Unused imports**: Review and remove if genuinely unused
3. **Unused variables**: Add `_` prefix if intentionally unused for future use
4. **Dead code**: Review each warning individually

---

## Data-Mining Service

### Unused Code Warnings

EOF

if [ -f "data_mining_unused_code.log" ]; then
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    cat data_mining_unused_code.log >> CLEANUP_RECOMMENDATIONS.md
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    echo "" >> CLEANUP_RECOMMENDATIONS.md
    
    DM_WARNINGS=$(grep -c "warning:" data_mining_unused_code.log 2>/dev/null || echo "0")
    echo "**Total warnings**: $DM_WARNINGS" >> CLEANUP_RECOMMENDATIONS.md
else
    echo "No unused code log found for Data-Mining." >> CLEANUP_RECOMMENDATIONS.md
fi

cat >> CLEANUP_RECOMMENDATIONS.md << 'EOF'

### Recommendations

1. **Unused variables in main.rs**: Check lines 772, 830 (price, buyers_60s)
   - If these are placeholders for future features, prefix with `_`
   - If genuinely unused, remove
2. **Pyth modules**: If not using Pyth price feeds, these can be removed
3. **Parser modules**: Verify all parser functions are being called

---

## Execution Service

### Unused Code Warnings

EOF

if [ -f "execution_unused_code.log" ]; then
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    cat execution_unused_code.log >> CLEANUP_RECOMMENDATIONS.md
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    echo "" >> CLEANUP_RECOMMENDATIONS.md
    
    EXEC_WARNINGS=$(grep -c "warning:" execution_unused_code.log 2>/dev/null || echo "0")
    echo "**Total warnings**: $EXEC_WARNINGS" >> CLEANUP_RECOMMENDATIONS.md
else
    echo "No unused code log found for Execution." >> CLEANUP_RECOMMENDATIONS.md
fi

cat >> CLEANUP_RECOMMENDATIONS.md << 'EOF'

### Recommendations

1. **main_failed.rs**: This appears to be an old/failed implementation
   - **RECOMMEND DELETION** if main.rs is working
   - Back up first if unsure
2. **metrics.rs (root level)**: Duplicate of src/metrics.rs?
   - Check if this is used
   - If not, **RECOMMEND DELETION**
3. **Mempool modules**: If mempool watching is not critical, consider removing
4. **Test scripts**: Many test scripts in execution/ - consolidate or archive old ones

---

## Mempool-Watcher Service

### Unused Code Warnings

EOF

if [ -f "mempool_watcher_unused_code.log" ]; then
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    cat mempool_watcher_unused_code.log >> CLEANUP_RECOMMENDATIONS.md
    echo "\`\`\`" >> CLEANUP_RECOMMENDATIONS.md
    echo "" >> CLEANUP_RECOMMENDATIONS.md
    
    MW_WARNINGS=$(grep -c "warning:" mempool_watcher_unused_code.log 2>/dev/null || echo "0")
    echo "**Total warnings**: $MW_WARNINGS" >> CLEANUP_RECOMMENDATIONS.md
else
    echo "No unused code log found for Mempool-Watcher." >> CLEANUP_RECOMMENDATIONS.md
fi

cat >> CLEANUP_RECOMMENDATIONS.md << 'EOF'

### Recommendations

1. **Unused imports**: Review and remove if genuinely unused
2. **WebSocket modules**: Ensure all WebSocket handling is actively used
3. **Database queries**: Verify all SQLite queries are necessary
4. **Audit logging**: Check if audit feature is fully implemented

---

## Files Identified for Potential Deletion

### High Confidence (Safe to Delete)

1. **execution/src/main_failed.rs** - Old implementation (back up first)
2. **execution/metrics.rs** (if duplicate of src/metrics.rs)

### Medium Confidence (Review First)

1. **Pyth price feed modules** (if not using Pyth):
   - data-mining/src/pyth_http.rs
   - data-mining/src/pyth_subscriber.rs
   - data-mining/src/pyth_subscriber_rpc.rs

2. **Mempool modules** (if not using mempool watching):
   - execution/src/mempool.rs
   - execution/src/mempool_bus.rs

3. **Old test scripts** (check if still relevant):
   - execution/test_*.py files that are duplicates

### Low Confidence (Keep for Now)

1. **Parser/raydium.rs** - May be for future Raydium integration
2. **Backtesting module** - Useful for strategy testing
3. **Test data files** (*.json, *.csv in execution/)

---

## Variables to Review

### Brain

Run: `grep -n "unused variable" brain_unused_code.log`

Action items:
- Add `_` prefix if intentionally unused
- Remove if genuinely not needed

### Data-Mining

Specific variables flagged:
- `price` at line 772 in main.rs
- `buyers_60s` at line 830 in main.rs

Recommendation: Prefix with `_` if these are for future window analysis features

### Execution

Run: `grep -n "unused variable" execution_unused_code.log`

---

## Cleanup Checklist

### Phase 1: Safe Deletions (Do First)

- [ ] Back up main_failed.rs
- [ ] Delete execution/src/main_failed.rs (if main.rs works)
- [ ] Check and delete execution/metrics.rs (if duplicate)
- [ ] Archive old test scripts to tests_archive/ folder

### Phase 2: Review and Decide

- [ ] Review each unused variable warning
- [ ] Add `_` prefix to intentionally unused variables
- [ ] Remove genuinely unused variables
- [ ] Review unused import warnings
- [ ] Remove unused imports

### Phase 3: Optional Cleanup

- [ ] Remove Pyth modules if not using Pyth price feed
- [ ] Remove mempool modules if not using mempool watching
- [ ] Consider removing raydium parser if not trading Raydium

### Phase 4: Verification

- [ ] Run all tests again: `./run_all_tests.sh`
- [ ] Ensure all services compile
- [ ] Verify no functionality broken
- [ ] Update documentation to reflect removed features

---

## How to Apply Recommendations

### 1. Back Up Everything First

```bash
cd /home/sol/Desktop/solana-dev/Bots/scalper-bot
tar -czf backup_before_cleanup_$(date +%Y%m%d).tar.gz brain/ data-mining/ execution/
```

### 2. Delete Confirmed Unused Files

```bash
# Example: Delete main_failed.rs
mv execution/src/main_failed.rs execution/src/main_failed.rs.bak
cargo build --release -p execution

# If builds successfully:
rm execution/src/main_failed.rs.bak
```

### 3. Fix Unused Variables

```bash
# Example: Fix unused variable
# Change: let price = ...
# To:     let _price = ...
```

### 4. Remove Unused Imports

Use `cargo fix` to automatically fix simple issues:

```bash
cd brain && cargo fix --allow-dirty
cd data-mining && cargo fix --allow-dirty
cd execution && cargo fix --allow-dirty
```

### 5. Verify Everything Still Works

```bash
./run_all_tests.sh
```

---

## Notes

- **Padding fields**: DO NOT remove `_padding` fields in message structs - these are for UDP packet alignment
- **TODO comments**: Not a problem, but track for future implementation
- **FIXME comments**: Should be addressed eventually
- **Dead code warnings**: May indicate genuinely unused code OR code that will be used in future features

---

## Contact

For questions about what's safe to delete, review:
- files/BRAIN_STRUCTURE.md
- files/DATA_MINING_STRUCTURE.md
- files/EXECUTION_STRUCTURE.md

These documents explain what each file does.

---

**Next Steps**: Review this report, make decisions on each recommendation, and execute cleanup in phases.
EOF

echo "✅ Cleanup report generated: CLEANUP_RECOMMENDATIONS.md" | tee -a "$MASTER_LOG"

# Final Summary
echo "" | tee -a "$MASTER_LOG"
echo "==========================================" | tee -a "$MASTER_LOG"
echo "FINAL SUMMARY" | tee -a "$MASTER_LOG"
echo "==========================================" | tee -a "$MASTER_LOG"
echo "Brain:           $BRAIN_STATUS" | tee -a "$MASTER_LOG"
echo "Data-Mining:     $DATA_MINING_STATUS" | tee -a "$MASTER_LOG"
echo "Execution:       $EXECUTION_STATUS" | tee -a "$MASTER_LOG"
echo "Mempool-Watcher: $MEMPOOL_STATUS" | tee -a "$MASTER_LOG"
echo "" | tee -a "$MASTER_LOG"

if [ "$BRAIN_STATUS" = "PASS" ] && [ "$DATA_MINING_STATUS" = "PASS" ] && [ "$EXECUTION_STATUS" = "PASS" ] && [ "$MEMPOOL_STATUS" = "PASS" ]; then
    echo "✅ ALL SERVICES PASSED!" | tee -a "$MASTER_LOG"
    echo "" | tee -a "$MASTER_LOG"
    echo "📋 Cleanup report: CLEANUP_RECOMMENDATIONS.md" | tee -a "$MASTER_LOG"
    EXIT_CODE=0
else
    echo "❌ SOME SERVICES FAILED" | tee -a "$MASTER_LOG"
    EXIT_CODE=1
fi

echo "Completed: $(date)" | tee -a "$MASTER_LOG"

exit $EXIT_CODE
