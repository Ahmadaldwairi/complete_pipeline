# Quick Reference Guide

**Last Updated**: November 1, 2025
**Purpose**: Fast reference for documentation and testing

---

## 📚 Documentation Files

### Directory Structures

- `files/BRAIN_STRUCTURE.md` - Brain service (7,564 lines, 25 files)
- `files/DATA_MINING_STRUCTURE.md` - Data-mining service (5,000+ lines)
- `files/EXECUTION_STRUCTURE.md` - Execution service (8,000+ lines)
- `files/MEMPOOL_WATCHER_STRUCTURE.md` - Mempool-watcher service (2,207 lines, 10 files)

**What's Inside**: Complete directory trees, file descriptions, line counts, module purposes, recent changes

---

## 🧪 Test Scripts

### Individual Service Tests

```bash
./test_brain.sh              # 13 tests for Brain
./test_data_mining.sh        # 15 tests for Data-mining
./test_execution.sh          # 17 tests for Execution
./test_mempool_watcher.sh    # 20 tests for Mempool-watcher
```

### Run All Tests

```bash
./run_all_tests.sh           # Runs all 65 tests + generates cleanup report
```

### Test Outputs

- `test_brain_results.log` - Brain test results
- `brain_unused_code.log` - Brain unused code warnings
- `test_data_mining_results.log` - Data-mining test results
- `data_mining_unused_code.log` - Data-mining unused code warnings
- `test_execution_results.log` - Execution test results
- `execution_unused_code.log` - Execution unused code warnings
- `test_mempool_watcher_results.log` - Mempool-watcher test results
- `mempool_watcher_unused_code.log` - Mempool-watcher unused code warnings
- `master_test_results.log` - All services summary
- `CLEANUP_RECOMMENDATIONS.md` - What to delete (auto-generated)

---

## 🧹 Cleanup Process

### Step 1: Back Up

```bash
tar -czf backup_$(date +%Y%m%d).tar.gz brain/ data-mining/ execution/ mempool-watcher/
```

### Step 2: Run Tests

```bash
./run_all_tests.sh
```

### Step 3: Review Report

```bash
cat CLEANUP_RECOMMENDATIONS.md
```

### Step 4: Execute Cleanup

Follow recommendations in CLEANUP_RECOMMENDATIONS.md:

- Phase 1: Safe deletions (main_failed.rs, duplicate metrics.rs)
- Phase 2: Fix unused variables (add \_ prefix)
- Phase 3: Optional (remove Pyth/mempool if unused)
- Phase 4: Verify with tests

### Step 5: Verify

```bash
./run_all_tests.sh
```

---

## 🎯 Quick Checks

### Build All Services

```bash
cd brain && cargo build --release
cd ../data-mining && cargo build --release
cd ../execution && cargo build --release
cd ../mempool-watcher && cargo build --release
```

### Check for Unused Code

```bash
cd brain && cargo clippy --quiet -- -W dead_code
cd ../data-mining && cargo clippy --quiet -- -W dead_code
cd ../mempool-watcher && cargo clippy --quiet -- -W dead_code
cd ../execution && cargo clippy --quiet -- -W dead_code
```

### Count TODO/FIXME

```bash
grep -r "TODO" brain/src/ data-mining/src/ execution/src/ mempool-watcher/src/ | wc -l
grep -r "FIXME" brain/src/ data-mining/src/ execution/src/ mempool-watcher/src/ | wc -l
```

---

## 📊 Test Coverage

| Service         | Tests  | Status |
| --------------- | ------ | ------ |
| Brain           | 13     | ✅     |
| Data-Mining     | 15     | ✅     |
| Execution       | 17     | ✅     |
| Mempool-Watcher | 20     | ✅     |
| **Total**       | **65** | ✅     |

---

## 🔍 Known Issues

### Data-Mining

- **line 772**: Unused variable `price` (prefix with `_`)
- **line 830**: Unused variable `buyers_60s` (prefix with `_`)

### Execution

- **main_failed.rs**: Delete after verifying main.rs works
- **metrics.rs** (root): Check if duplicate, delete if so

### Mempool-Watcher

- To be determined after first test run

### Brain

- ✅ No major issues

---

## 📁 File Locations

```
/home/sol/Desktop/solana-dev/Bots/scalper-bot/
├── files/                          # Documentation
│   ├── BRAIN_STRUCTURE.md
│   ├── DATA_MINING_STRUCTURE.md
│   ├── EXECUTION_STRUCTURE.md
│   └── MEMPOOL_WATCHER_STRUCTURE.md
├── test_brain.sh                   # Brain tests
├── test_data_mining.sh             # Data-mining tests
├── test_execution.sh               # Execution tests
├── test_mempool_watcher.sh         # Mempool-watcher tests
├── run_all_tests.sh                # Master test runner
├── CLEANUP_RECOMMENDATIONS.md      # Cleanup guide (generated)
├── DOCUMENTATION_AND_TESTING_SUMMARY.md  # Full summary
└── QUICK_REFERENCE.md              # This file
```

---

## 🚀 Common Tasks

### View documentation for a service

```bash
cat files/BRAIN_STRUCTURE.md
cat files/DATA_MINING_STRUCTURE.md
cat files/EXECUTION_STRUCTURE.md
cat files/MEMPOOL_WATCHER_STRUCTURE.md
```

### Test a single service

```bash
./test_brain.sh
./test_data_mining.sh
./test_execution.sh
./test_mempool_watcher.sh
```

### Test everything

```bash
./run_all_tests.sh
```

### Find what to delete

```bash
cat CLEANUP_RECOMMENDATIONS.md
```

### Check test results

```bash
cat master_test_results.log
```

---

## ⚡ One-Liners

```bash
# Full audit in one command
./run_all_tests.sh && cat CLEANUP_RECOMMENDATIONS.md

# Count unused warnings
grep -c "warning:" *_unused_code.log

# List all test scripts
ls -lh test_*.sh

# Check if services compile
for dir in brain data-mining execution; do cd $dir && cargo build --release && cd ..; done
```

---

## 📝 Notes

- **Padding fields**: Never delete `_padding` in message structs (UDP alignment)
- **Tests are non-destructive**: Safe to run anytime
- **Cleanup is optional**: System works fine with current code
- **Back up first**: Always back up before deleting files

---

**Need help?** Check:

1. DOCUMENTATION_AND_TESTING_SUMMARY.md (detailed overview)
2. CLEANUP_RECOMMENDATIONS.md (what to delete)
3. Test logs (\*\_results.log files)
