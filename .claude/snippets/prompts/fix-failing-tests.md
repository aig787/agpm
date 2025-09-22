# Fix Failing Tests - Systematic Test Repair

## 🎯 Definition of Complete

**This task is ONLY complete when:**
- `cargo test --lib` shows all unit tests passing
- `cargo test --tests` shows all integration tests passing
- `cargo test --doc` shows all documentation tests passing
- `cargo nextest run` shows all tests passing (if available)
- Zero tests show as failed across ALL test categories
- Every single "test result:" line shows "0 failed"

**If you see ANY failed tests in ANY category, you MUST continue working.**

## Overview

You are tasked with fixing ALL failing tests in the CCPM (Claude Code Package Manager) project. This is an iterative process where you will continuously discover, fix, and validate until every single test passes.

**⏱️ IMPORTANT: The full test suite may take up to 5 minutes to run.** Plan accordingly and be patient when running `cargo test --all`. Individual test categories typically run faster, but doc tests in particular can be slow due to compilation overhead.

## Test Categories and Priority Order

### Priority 1: Functional Tests (Fix First)
These are the core tests that verify the application works correctly:

1. **Unit Tests** (`cargo test --lib`)
   - Located in `src/**/*.rs` in `#[cfg(test)] mod tests` blocks
   - Test individual functions and methods
   - Usually fastest to run and fix

2. **Integration Tests** (`cargo test --tests`)
   - Located in `tests/*.rs` files
   - Test complete workflows and CLI commands
   - May involve file I/O and external processes

### Priority 2: Documentation Tests (Fix Second)
These ensure code examples in documentation are correct:

3. **Doc Tests** (`cargo test --doc`)
   - Located in `///` doc comments in `src/**/*.rs`
   - Test code examples in documentation
   - Can be slow due to compilation overhead
   - Often require proper imports and setup code

### Using nextest (if available)
The project may use `cargo nextest` for faster parallel test execution:

```bash
# Run all tests with nextest (much faster than cargo test)
cargo nextest run

# Run specific test categories
cargo nextest run --lib
cargo nextest run --tests

# Note: nextest doesn't support doc tests, use cargo test --doc for those
cargo test --doc
```

## ⚠️ Common Mistakes to Avoid

❌ **DO NOT** assume your initial test run found all failures
❌ **DO NOT** run only specific test files - run the full suite
❌ **DO NOT** skip doc tests - they often reveal API issues
❌ **DO NOT** change test logic without understanding its purpose
❌ **DO NOT** comment out failing tests - fix them properly
❌ **DO NOT** forget to check that your fixes don't break other tests
❌ **DO NOT** ignore compilation errors before running tests

## 🎯 Strategic Approach

### The Right Mindset

Think of this as **detective work**: You're uncovering all failures systematically, fixing them properly, and ensuring no collateral damage. Each test failure is a clue about what's broken.

### Key Principles

1. **Run the FULL test suite** - Individual test files may pass while others fail
2. **Fix compilation errors first** - Tests can't run if the code doesn't compile
3. **Save all output** - Keep test failure logs for reference
4. **Track your progress** - Know how many tests failed initially and monitor improvement
5. **Verify fixes comprehensively** - One fix might break something else
6. **Test incrementally** - After each fix, run relevant tests before the full suite

## 📋 Complete Process

### Step 1: Preparation and Initial Assessment

```bash
# First, ensure compilation succeeds without running tests
cargo build --all-targets 2>&1 | tee /tmp/build_output.txt

# If compilation fails, fix those errors FIRST before proceeding

# Count total tests to understand scope (attempt to count, but may not work for all)
echo "Attempting to count tests..."
echo "Unit tests: $(cargo test --lib -- --list 2>&1 | grep -c '^test ' || echo 0)"
echo "Integration tests: $(cargo test --tests -- --list 2>&1 | grep -c '^test ' || echo 0)"
# Doc tests are harder to count without running them
```

**STOP if compilation fails** - fix compilation errors first before identifying test failures.

### Step 2: Discover ALL Failures

```bash
# Clean any stale test artifacts
cargo clean

# Check formatting and linting first (may be required to pass)
cargo fmt --check
cargo clippy -- -D warnings

# Run all tests with output to see failures (may take up to 5 minutes)
cargo test --all 2>&1 | tee /tmp/test_failures_initial.txt

# Count failures for tracking progress
grep "test result:" /tmp/test_failures_initial.txt
```

### Step 3: Categorize Failures

Organize test failures by type to fix systematically:

```bash
# Extract just the failure names for unit tests
cargo test --lib 2>&1 | grep "^test " | grep FAILED > /tmp/unit_test_failures.txt

# Extract integration test failures
cargo test --tests 2>&1 | grep "^test " | grep FAILED > /tmp/integration_test_failures.txt

# Extract doc test failures
cargo test --doc 2>&1 | grep "^test " | grep FAILED > /tmp/doc_test_failures.txt

# Count by category
echo "Unit test failures: $(wc -l < /tmp/unit_test_failures.txt)"
echo "Integration test failures: $(wc -l < /tmp/integration_test_failures.txt)"
echo "Doc test failures: $(wc -l < /tmp/doc_test_failures.txt)"
```

### Step 4: Fix Failures Systematically

For EACH failure:

1. **Understand the test's purpose** - Read the test name and its code
2. **Examine the failure message** - What exactly is failing?
3. **Check recent changes** - Was something recently modified?
4. **Apply targeted fix** - Change only what's necessary
5. **Verify the specific fix** - Run that test individually
6. **Check for side effects** - Run related tests

Example workflow for a single failure:

```bash
# For a specific failing test
cargo test --lib test_specific_function 2>&1 | tee /tmp/single_test.txt

# After fixing, verify it passes
cargo test --lib test_specific_function

# Then check the entire module/file wasn't broken
cargo test --lib module_name
```

### Incremental Verification

After fixing a batch of related tests:

```bash
# Run the specific test category
cargo test --lib  # After fixing unit tests
cargo test --tests  # After fixing integration tests
cargo test --doc  # After fixing doc tests

# Save current state
cargo test --all 2>&1 | tee /tmp/test_failures_current_$(date +%s).txt

# Check remaining failures
grep "^test " /tmp/test_failures_current_*.txt | grep FAILED
```

### Step 5: Common Doc Test Fixes

Doc tests have unique issues that differ from regular tests:

#### Missing Imports in Doc Examples
```rust
/// ```
/// # use ccpm::utils::progress::MultiPhaseProgress;  // Add hidden import
/// let progress = MultiPhaseProgress::new(true);
/// ```
```

#### Need for `no_run` or `ignore`
```rust
/// ```no_run
/// // Code that shouldn't actually execute during tests
/// std::process::exit(0);
/// ```

/// ```ignore
/// // Code that's for illustration only
/// this_wont_compile();
/// ```
```

#### Incomplete Examples
```rust
/// ```
/// # use ccpm::manifest::Manifest;
/// # let manifest = Manifest::default();  // Add setup code
/// let deps = manifest.all_dependencies();
/// # assert!(deps.is_empty());  // Add assertion if needed
/// ```
```

### Step 6: Common CCPM-Specific Fixes

#### Missing Test Utilities

CCPM tests often need test utilities that may be missing:

```rust
// In tests/fixtures/mod.rs or similar
pub fn create_test_manifest() -> Manifest {
    // Test manifest creation
}

pub fn temp_dir_with_git() -> TempDir {
    // Creates temp dir with git repo
}
```

#### Async Test Issues

Many CCPM tests are async and need proper runtime:

```rust
#[tokio::test]  // Not #[test]
async fn test_async_operation() {
    // async test code
}
```

#### File System Dependencies

Tests may fail due to file system state:

```rust
#[test]
fn test_with_files() {
    let temp = TempDir::new().unwrap();
    // Always use temp directories, never rely on project structure
}
```

### Step 6: Final Validation

**This is CRITICAL** - you must verify ALL tests pass:

```bash
# Clear terminal for clean view
clear

# Final comprehensive test run
echo "=== FINAL VALIDATION ==="
echo "Starting at: $(date)"

# Run each category explicitly to ensure nothing is missed
echo -e "\n--- Unit Tests ---"
cargo test --lib 2>&1 | tee /tmp/final_unit.txt
UNIT_RESULT=$(grep "test result:" /tmp/final_unit.txt)

echo -e "\n--- Integration Tests ---"
cargo test --tests 2>&1 | tee /tmp/final_integration.txt
INTEGRATION_RESULT=$(grep "test result:" /tmp/final_integration.txt)

echo -e "\n--- Doc Tests ---"
cargo test --doc 2>&1 | tee /tmp/final_doc.txt
DOC_RESULT=$(grep "test result:" /tmp/final_doc.txt)

echo -e "\n--- All Tests ---"
cargo test --all 2>&1 | tee /tmp/final_all.txt

# Summary
echo -e "\n=== FINAL RESULTS ==="
echo "Unit tests: $UNIT_RESULT"
echo "Integration tests: $INTEGRATION_RESULT"
echo "Doc tests: $DOC_RESULT"
echo "Completed at: $(date)"

# Check if all passed
if ! grep -q "0 failed" /tmp/final_all.txt; then
    echo "❌ TESTS STILL FAILING - Continue fixing!"
    grep "^test " /tmp/final_all.txt | grep FAILED
else
    echo "✅ ALL TESTS PASSING!"
fi
```

### Step 7: Continuous Monitoring

If tests are still failing:

1. Check the test output carefully - sometimes failures are hidden
2. Look for panics or thread failures that might not show as "FAILED"
3. Verify no tests are being skipped unintentionally
4. Check for timing-dependent or environment-dependent failures

```bash
# Look for any signs of problems
grep -E "(FAILED|ERROR|panic|thread.*panicked)" /tmp/final_all.txt

# Check for skipped tests
grep -i "ignored" /tmp/final_all.txt
```

## 🔍 Debugging Strategies

### For Mysterious Failures

```bash
# Run with more output
RUST_BACKTRACE=1 cargo test --all -- --nocapture

# Run single test with maximum verbosity
RUST_BACKTRACE=full cargo test test_name -- --exact --nocapture
```

### For Flaky Tests

```bash
# Run multiple times to check for consistency
for i in {1..5}; do
    echo "Run $i"
    cargo test test_name
done
```

### For Compilation Issues

```bash
# Check for feature flag issues
cargo test --all --all-features
cargo test --all --no-default-features

# Check for missing dependencies
cargo tree
cargo update
```

## Quick Reference Commands

```bash
# Run all tests (may take up to 5 minutes)
cargo test --all

# Run with output
cargo test --all -- --nocapture

# Run specific test
cargo test test_name -- --exact

# Run tests in a specific file
cargo test --test integration_test_file

# Run with backtrace
RUST_BACKTRACE=1 cargo test

# Check what would run without running
cargo test --no-run
```

## Handling Doc Test Timeouts

Doc tests can be particularly slow because:
- Each example is compiled as a separate binary
- Hidden setup code still needs compilation
- Multiple examples in one file compound the issue

If doc tests timeout or hang:

```rust
/// Instead of multiple small examples:
/// ```
/// let x = 1;
/// ```
/// ```
/// let y = 2;
/// ```

/// Combine into one when possible:
/// ```
/// let x = 1;
/// let y = 2;
/// ```
```

## Success Criteria

The fix process is complete when:

✅ All unit tests pass (`cargo test --lib`)
✅ All integration tests pass (`cargo test --tests`)
✅ All doc tests pass (`cargo test --doc`)
✅ All tests pass with nextest if available (`cargo nextest run`)
✅ No tests are skipped or ignored without documented blockers
✅ No test's original intent has been changed
✅ The test suite is stable (passes consistently)
✅ Tests complete in reasonable time (no timeouts)

## When to Use Specialized Agents

Consider delegating to rust-troubleshooter-advanced when:
- Encountering memory safety issues or undefined behavior
- Dealing with complex lifetime or borrowing errors
- Facing race conditions or deadlocks in async tests

Consider delegating to rust-expert-advanced when:
- Tests reveal architectural issues requiring redesign
- Performance problems in tests indicate algorithmic issues
- Complex generic or trait bound problems

## Notes for CCPM Specifics

- The project uses **cargo nextest** for faster test execution when available
- Tests may involve Git operations - ensure Git is configured in test environment
- Many tests create temporary directories - ensure sufficient disk space
- Integration tests may run actual CLI commands - be patient with execution time
- Some tests require network access for Git operations
- Tests use `tokio::test` for async testing

Remember: **The job is NOT done until `cargo test --all` shows ZERO failures!**