# Test Coverage Improvement Instructions for CCPM

## Overview

You are tasked with improving test coverage for the CCPM (Claude Code Package Manager) project. The target is 70%
minimum coverage. All existing tests must remain passing throughout this process.

**IMPORTANT**: This is an iterative process. You will:

1. Check current coverage
2. Identify gaps
3. Write tests for one module/component
4. Verify all tests still pass
5. Re-measure coverage
6. Repeat until target is met

Work incrementally - don't try to fix everything at once. Focus on one module at a time, ensuring stability at each
step.

## Pre-Flight Checklist

### Step 1: Ensure Clean Starting State

**CRITICAL**: All tests must pass before checking coverage. Run these commands first:

```bash
# Format and lint check
cargo fmt --check
cargo clippy -- -D warnings

# Run all tests to ensure they pass
cargo test --all

# If any tests fail, use rust-test-fixer agent to fix them first
# DO NOT proceed to coverage analysis until all tests pass
```

### Step 2: Generate Baseline Coverage Report

**IMPORTANT Coverage Measurement Notes:**

- Use `cargo tarpaulin` WITHOUT `--lib` flag to include integration tests for better overall coverage
- Integration tests DO contribute to coverage when run with tarpaulin (they test the library code)
- Test utilities themselves (test_utils module) should be excluded from coverage metrics
- **CRITICAL: `cargo tarpaulin` is expensive to run (takes ~3 minutes)** - save outputs to tmp files and refer back to them

```bash
# Save baseline coverage output to a tmp file for reference (avoid re-running)
cargo tarpaulin --out Stdout --exclude-files "*/test_utils/*" | tee /tmp/ccpm_coverage_baseline.txt

# Check current coverage percentage quickly from saved file
tail -5 /tmp/ccpm_coverage_baseline.txt

# Generate HTML coverage report only when needed for detailed analysis
cargo tarpaulin --out html --exclude-files "*/test_utils/*" --output-dir target/coverage

# View the HTML report (optional, for detailed analysis)
open target/coverage/tarpaulin-report.html  # macOS
# xdg-open target/coverage/tarpaulin-report.html  # Linux
# start target/coverage/tarpaulin-report.html  # Windows

# Or use the Makefile (if available)
make coverage
```

## Coverage Improvement Strategy

### Step 3: Identify Coverage Gaps with Impact Analysis

#### Prioritization Strategy

**IMPORTANT**: Don't just look at coverage percentages! Calculate impact scores to maximize coverage improvements.

**Working Script to Calculate Impact Scores:**

```bash
# Extract coverage ratios from the summary section (format: "src/file.rs: 123/456")
grep -E "src/[^:]+: [0-9]+/[0-9]+" /tmp/ccpm_coverage_baseline.txt | while read line; do
    # Extract file path and coverage
    file=$(echo "$line" | cut -d: -f1 | sed 's/|| //')
    coverage_part=$(echo "$line" | cut -d: -f2 | cut -d' ' -f2)
    covered=$(echo "$coverage_part" | cut -d/ -f1)
    total=$(echo "$coverage_part" | cut -d/ -f2)
    
    uncovered=$((total - covered))
    if [ "$total" -gt 0 ] && [ "$uncovered" -gt 0 ]; then
        percent=$((covered * 100 / total))
        impact=$((uncovered * (100 - percent) / 100))
        printf "%4d %4d %3d%% %s\n" "$impact" "$uncovered" "$percent" "$file"
    fi
done | sort -rn | head -20
```

**Note**: The coverage report has two sections:
1. Uncovered lines section (format: `src/file.rs: 12-15, 20, 25-30`)
2. Summary section (format: `src/file.rs: 123/456 +0.00%`) - use this for calculations

**Impact Score Formula:**
- `Impact = uncovered_lines × (100 - coverage%) / 100`
- Higher impact = more overall coverage improvement
- Example: 50 uncovered lines at 20% coverage = 50 × 80 / 100 = 40 impact

#### What to Target First

1. **High Impact Modules** - Highest impact scores regardless of percentage
2. **Testable Logic** - Modules with algorithmic code, not just strings
3. **Critical Paths** - Important functionality with no tests
4. **Error Handling** - Error cases that aren't tested
5. **Quick Wins** - Small modules that can reach 100% quickly

#### What to Skip or Deprioritize

1. **Template/String Heavy** - Modules with mostly template literals
2. **CLI Output** - Print statements and formatting code
3. **Generated Code** - Auto-generated or boilerplate code
4. **External Wrappers** - Thin wrappers around external libraries

#### Module Categories by Testing Value

**High Value** (Test First):
- `src/resolver/` - Dependency resolution logic
- `src/core/` - Core business logic
- `src/lockfile/` - Critical for reproducibility
- `src/manifest/` - Parsing and validation logic

**Medium Value** (Test Second):
- `src/git/` - Git operations (if not just wrapping CLI)
- `src/source/` - Source management logic
- `src/hooks/` - Hook configuration logic
- `src/cache/` - Cache management

**Lower Value** (Test Last):
- `src/cli/` - Often template-heavy with strings
- `src/utils/` - Usually well-tested utilities
- `src/config/` - Simple configuration loading

### Step 4: Writing Tests with Agent Assistance

#### Use Specialized Agents for Different Tasks:

1. **rust-expert** - For designing test strategies:
    - Complex test scenarios
    - Mock implementations
    - Test architecture decisions
    - Integration test design

2. **rust-linting-expert** - For test code quality:
    - Ensuring test code follows conventions
    - Fixing test compilation issues
    - Running clippy on test code

3. **rust-test-fixer** - For fixing test issues:
    - Assertion failures
    - Setup/teardown problems
    - Test isolation issues
    - Flaky test fixes

4. **rust-troubleshooter-opus** - For complex test problems:
    - Race conditions in tests
    - Memory issues in tests
    - Performance test design
    - Debugging mysterious failures

5. **general-purpose** - For research and planning:
    - Finding similar test patterns in codebase
    - Researching testing best practices
    - Understanding existing test infrastructure

### Step 5: Test Implementation Guidelines

#### Unit Tests

Location: In the same file as the code being tested

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_function_name() {
        // Arrange
        let temp_dir = TempDir::new().unwrap();

        // Act
        let result = function_under_test();

        // Assert
        assert!(result.is_ok());
    }
}
```

#### Integration Tests

Location: `tests/` directory

```rust
use ccpm::cli;
use tempfile::TempDir;

#[test]
fn test_install_command() {
    let temp_dir = TempDir::new().unwrap();
    // Test full command execution
}
```

### Step 6: CCPM-Specific Testing Requirements

#### Critical Testing Rules

1. **NEVER use `std::env::set_var`** in tests (causes race conditions)
    - Exception: Tests explicitly testing env var functionality
    - Must be documented with clear comments
    - Use `.env()` on Command for subprocesses instead

2. **Cache Directory Isolation**
    - Each test MUST use its own temp directory
    - Never share cache directories between tests
    - Clean up temp directories after tests

3. **No Global State**
    - Tests must not modify global state
    - Each test should be completely independent
    - Use dependency injection for configuration

#### Testing Patterns for CCPM

**Manifest Testing**

```rust
#[test]
fn test_manifest_parsing() {
    let toml_content = r#"
        [sources]
        community = "https://github.com/example/repo.git"
    "#;
    let manifest = Manifest::from_str(toml_content).unwrap();
    assert_eq!(manifest.sources.len(), 1);
}
```

**Git Operation Testing**

```rust
#[test]
fn test_git_clone() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");

    // Mock git operations or use test repositories
    // Never test against real external repositories
}
```

**Lockfile Testing**

```rust
#[test]
fn test_lockfile_generation() {
    let temp_dir = TempDir::new().unwrap();
    let project_dir = temp_dir.path();

    // Test lockfile is deterministic
    let lock1 = generate_lockfile(project_dir).unwrap();
    let lock2 = generate_lockfile(project_dir).unwrap();
    assert_eq!(lock1, lock2);
}
```

### Step 7: Iterative Coverage Improvement Workflow

**This is a continuous cycle - repeat for each module until overall target is met:**

#### Smart Module Selection

When selecting modules to test, use this decision matrix:

```bash
# Quick analysis to find best targets
echo "=== High Impact Modules (test these first) ==="
# Modules with >50 uncovered lines and <50% coverage
grep "^|| src/" /tmp/ccpm_coverage_*.txt | awk -F': ' '{
    split($2, a, " ");
    split(a[1], b, "/");
    if (b[2] > 0) {
        uncovered = b[2] - b[1];
        coverage = b[1] * 100 / b[2];
        if (uncovered > 50 && coverage < 50) {
            impact = uncovered * (100 - coverage) / 100;
            printf "%3d impact | %3d lines | %5.1f%% | %s\n", impact, uncovered, coverage, $1;
        }
    }
}' | sort -rn

echo -e "\n=== Quick Wins (small modules to boost percentage) ==="
# Modules with <20 uncovered lines that can reach 90%+
grep "^|| src/" /tmp/ccpm_coverage_*.txt | awk -F': ' '{
    split($2, a, " ");
    split(a[1], b, "/");
    if (b[2] > 0) {
        uncovered = b[2] - b[1];
        coverage = b[1] * 100 / b[2];
        potential = (b[1] + uncovered * 0.8) * 100 / b[2];
        if (uncovered < 20 && uncovered > 0 && potential > 90) {
            printf "%3d lines | %5.1f%% → %5.1f%% | %s\n", uncovered, coverage, potential, $1;
        }
    }
}' | sort -n
```

#### When You're Close to Target

If you're within 1% of the target (e.g., at 69.91% aiming for 70%):

1. **Calculate Exact Need:**
   ```bash
   # Extract actual numbers from coverage report
   coverage_line=$(tail -1 /tmp/ccpm_coverage_baseline.txt)
   # Example line: "69.91% coverage, 3609/5162 lines covered, +0.00% change in coverage"
   
   current_pct=$(echo "$coverage_line" | grep -oE "[0-9]+\.[0-9]+%" | head -1 | tr -d '%')
   current_covered=$(echo "$coverage_line" | grep -oE "[0-9]+/[0-9]+" | cut -d/ -f1)
   total_lines=$(echo "$coverage_line" | grep -oE "[0-9]+/[0-9]+" | cut -d/ -f2)
   
   target_pct=70.0
   target_covered=$(echo "scale=0; $total_lines * $target_pct / 100" | bc)
   need_to_cover=$((target_covered - current_covered))
   
   echo "Current: $current_pct% ($current_covered/$total_lines)"
   echo "Target: $target_pct% ($target_covered/$total_lines)"
   echo "Need to cover $need_to_cover more lines to reach $target_pct%"
   ```
   
   **Real Example**: From 69.91% to 70.0% required covering just 5 additional lines!

2. **Find Quick Wins:**
   - Look for modules with 5-10 uncovered lines
   - Target utility functions that are easy to test
   - Add missing error case tests
   - Complete partially tested functions

3. **Avoid Over-Engineering:**
   - Don't write complex tests for string templates
   - Skip CLI formatting/output code
   - Focus on testable business logic

1. **Measure Current State**
   ```bash
   # IMPORTANT: Save coverage output to avoid expensive re-runs
   cargo tarpaulin --lib --out Stdout | tee /tmp/ccpm_coverage_current.txt
   
   # Check specific module coverage from saved output
   grep "src/module_name" /tmp/ccpm_coverage_current.txt
   
   # Or reference previous runs
   cat /tmp/ccpm_coverage_*.txt | tail -1 | grep "src/module_name"
   ```

2. **Pick ONE Module to Improve**
    - Choose module with lowest coverage
    - Or pick critical functionality
    - Don't try to fix multiple modules at once

3. **Write Tests for That Module**
    - Start with happy path tests
    - Add error case tests
    - Include edge cases
    - Test cross-platform behavior

4. **Verify Stability**
   ```bash
   # Run only new tests first
   cargo test module_name::tests
   
   # CRITICAL: Run ALL tests to ensure no regression
   cargo test --all
   
   # If any test fails, fix it before proceeding
   ```

5. **Measure Improvement**
   ```bash
   # Save new coverage measurement
   cargo tarpaulin --lib --out Stdout | tee /tmp/ccpm_coverage_after_$(date +%Y%m%d_%H%M%S).txt
   
   # Compare with previous baseline
   echo "=== Before ===" && grep "src/module_name" /tmp/ccpm_coverage_current.txt
   echo "=== After ===" && grep "src/module_name" /tmp/ccpm_coverage_after_*.txt | tail -1
   
   # Generate HTML report only when needed for detailed analysis
   # cargo tarpaulin --out html --output-dir target/coverage
   ```

6. **Commit Progress**
   ```bash
   # Commit your tests for this module
   git add -A
   git commit -m "Add tests for [module_name], improve coverage"
   ```

7. **Repeat Cycle**
    - Return to step 1
    - Pick next module
    - Continue until 70% overall coverage reached

**Remember**: Small, incremental improvements are better than large, risky changes. Each iteration should leave the
codebase in a stable, working state.

### Step 8: Common Testing Scenarios for CCPM

#### Testing CLI Commands

```rust
#[test]
fn test_install_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let manifest = create_test_manifest();

    // Run install command
    let result = cli::install::execute(temp_dir.path()).await;

    // Verify lockfile created
    assert!(temp_dir.path().join("ccpm.lock").exists());
}
```

#### Testing Error Handling

```rust
#[test]
fn test_invalid_manifest_error() {
    let invalid_toml = "invalid content";
    let result = Manifest::from_str(invalid_toml);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("parsing"));
}
```

#### Testing Async Code

```rust
#[tokio::test]
async fn test_async_operation() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### Step 9: Monitoring Progress

Track coverage improvements efficiently:

```bash
# Save coverage runs with descriptive names
cargo tarpaulin --lib --ignore-tests --out Stdout | tee /tmp/ccpm_coverage_no_tests.txt

# Focus on specific modules (save output)
cargo tarpaulin --lib --out Stdout -- module_name:: | tee /tmp/ccpm_coverage_module_$(date +%Y%m%d).txt

# Skip problematic tests temporarily (save output)
cargo tarpaulin --lib --skip 'test_name_pattern' | tee /tmp/ccpm_coverage_skip_problematic.txt

# Compare coverage over time
ls -la /tmp/ccpm_coverage_*.txt
diff /tmp/ccpm_coverage_current.txt /tmp/ccpm_coverage_after_*.txt | tail -1

# Quick coverage check from saved results
tail -20 /tmp/ccpm_coverage_current.txt
```

## Red Flags to Avoid

1. **Test Coupling**: Tests that depend on execution order
2. **External Dependencies**: Tests requiring internet or external services
3. **Hardcoded Paths**: Using absolute paths instead of temp directories
4. **Time Dependencies**: Tests that depend on system time
5. **Resource Leaks**: Not cleaning up temp files/directories
6. **Flaky Tests**: Non-deterministic tests that pass/fail randomly

## Coverage Goals by Module

Based on module importance:

- `src/core/`: Target 80% (critical functionality)
- `src/cli/`: Target 75% (user-facing commands)
- `src/manifest/`: Target 85% (parsing critical)
- `src/lockfile/`: Target 85% (reproducibility critical)
- `src/resolver/`: Target 80% (dependency logic)
- `src/git/`: Target 70% (wrapper around git CLI)
- `src/utils/`: Target 60% (utility functions)

## Practical Tips from Experience

1. **Finding Missing Test Cases**: Look for patterns in existing tests - often resource types (agents, snippets, commands) have tests but newer types (scripts, hooks) are missing.

2. **Test File Location**: Unit tests go in the same file in a `#[cfg(test)] mod tests` block at the bottom.

3. **Running Specific Tests**: 
   ```bash
   # Run tests for a specific module
   cargo test --lib cli::remove::tests
   
   # With output (note the -- before --nocapture)
   cargo test --lib cli::remove::tests -- --nocapture
   ```

4. **Coverage Calculation**: Coverage percentage = (covered_lines / total_lines) × 100. Each line covered adds roughly 0.02% to a 5000-line codebase.

5. **Integration vs Unit Tests**: Both contribute to coverage when using tarpaulin. Integration tests often provide broader coverage per test.

## Quick Commands Reference

```bash
# Check if all tests pass
cargo test --all

# Generate and save coverage report (EXPENSIVE - save output!)
cargo tarpaulin --out Stdout --exclude-files "*/test_utils/*" | tee /tmp/ccpm_coverage_baseline.txt

# Generate HTML report only when needed for detailed analysis
cargo tarpaulin --out html --output-dir target/coverage

# Run coverage for specific module (save output)
cargo tarpaulin --lib --out Stdout -- module_name:: | tee /tmp/ccpm_coverage_module.txt

# View saved coverage reports
ls -la /tmp/ccpm_coverage_*.txt
cat /tmp/ccpm_coverage_current.txt

# Run tests with single thread (for debugging)
cargo test -- --test-threads=1

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name -- --exact

# Check coverage without running tests (save output)
cargo tarpaulin --lib --ignore-tests --out Stdout | tee /tmp/ccpm_coverage_no_tests.txt

# Compare coverage between runs
diff /tmp/ccpm_coverage_current.txt /tmp/ccpm_coverage_after_*.txt | tail -1
```

## When to Use Each Agent

- **Before starting**: Use `rust-test-fixer` to ensure all tests pass
- **Planning tests**: Use `rust-expert` for test architecture
- **Writing tests**: Use `general-purpose` to find similar patterns
- **Fixing failures**: Use `rust-test-fixer` for quick fixes
- **Complex issues**: Use `rust-troubleshooter-opus` for deep debugging
- **Code quality**: Use `rust-linting-expert` for test code style

## Real-World Example: Improving CCPM Coverage from 69.91% to 70.11%

This is an actual example of how the coverage was improved:

### 1. Initial Assessment
```bash
# Check baseline
tail -5 /tmp/ccpm_coverage_baseline.txt
# Output: 69.91% coverage, 3609/5162 lines covered
```

### 2. Impact Analysis
```bash
# Found highest impact modules
# Output (top 3):
#   68  113  39% src/cli/remove.rs     <- Highest impact!
#   61  154  60% src/cli/install.rs
#   60  140  57% src/cli/validate.rs
```

### 3. Targeted Improvement
Added 3 strategic tests to `src/cli/remove.rs`:
- `test_remove_script_success` - Covered script removal path
- `test_remove_hook_success` - Covered hook removal path  
- `test_remove_script_and_hook_from_lockfile` - Covered lockfile updates
- Updated existing test to include script/hook checking

### 4. Result
```bash
# After adding tests
tail -5 /tmp/ccpm_coverage_after.txt
# Output: 70.11% coverage, 3619/5162 lines covered, +0.19% change
```

**Key Insight**: Just 10 lines of additional coverage (focused on high-impact areas) achieved the goal!

## Success Criteria

✅ All existing tests still passing after EACH iteration
✅ Coverage increased to 70% minimum through incremental improvements
✅ No flaky or non-deterministic tests added
✅ All new tests follow isolation requirements
✅ Tests work on Windows, macOS, and Linux
✅ No use of `std::env::set_var` (except documented exceptions)
✅ Each test uses its own temp directory
✅ No external network dependencies in tests
✅ Each module improved and committed separately
✅ Stable, working codebase maintained throughout the process