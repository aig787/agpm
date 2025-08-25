# Test Coverage Improvement Instructions for CCPM

## Overview
You are tasked with improving test coverage for the CCPM (Claude Code Package Manager) project. The target is 70% minimum coverage. All existing tests must remain passing throughout this process.

**IMPORTANT**: This is an iterative process. You will:
1. Check current coverage
2. Identify gaps
3. Write tests for one module/component
4. Verify all tests still pass
5. Re-measure coverage
6. Repeat until target is met

Work incrementally - don't try to fix everything at once. Focus on one module at a time, ensuring stability at each step.

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
- Use `cargo tarpaulin` WITHOUT `--lib` flag to include binary tests
- Integration tests execute the binary and don't contribute to library coverage
- Test utilities themselves (test_utils module) should be excluded from coverage metrics

```bash
# Generate HTML coverage report (includes all test types)
cargo tarpaulin --out html --output-dir target/coverage

# Exclude test utilities from coverage metrics
cargo tarpaulin --out Stdout --exclude-files "*/test_utils/*"

# Or use the Makefile
make coverage

# View the report
open target/coverage/tarpaulin-report.html  # macOS
# xdg-open target/coverage/tarpaulin-report.html  # Linux
# start target/coverage/tarpaulin-report.html  # Windows
```

## Coverage Improvement Strategy

### Step 3: Identify Coverage Gaps

Use the coverage report to identify:
1. **Uncovered modules** - Entire modules with no tests
2. **Low coverage files** - Files with < 50% coverage
3. **Critical paths** - Important functionality with no tests
4. **Error handling** - Error cases that aren't tested
5. **Edge cases** - Boundary conditions not covered

Priority order for CCPM modules:
1. **Core functionality** (`src/core/`)
2. **CLI commands** (`src/cli/`)
3. **Manifest/Lockfile** (`src/manifest/`, `src/lockfile/`)
4. **Dependency resolver** (`src/resolver/`)
5. **Git operations** (`src/git/`)
6. **Source management** (`src/source/`)
7. **Utilities** (`src/utils/`)

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

1. **Measure Current State**
   ```bash
   # Get baseline coverage
   cargo tarpaulin --lib --out Stdout
   
   # Check specific module coverage
   cargo tarpaulin --lib --out Stdout | grep "src/module_name"
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
   # Generate new coverage report
   cargo tarpaulin --out html --output-dir target/coverage
   
   # Check if module coverage improved
   cargo tarpaulin --lib --out Stdout | grep "src/module_name"
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
   
**Remember**: Small, incremental improvements are better than large, risky changes. Each iteration should leave the codebase in a stable, working state.

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

Track coverage improvements:
```bash
# Run coverage with specific exclusions if needed
cargo tarpaulin --lib --ignore-tests --out Stdout

# Focus on specific modules
cargo tarpaulin --lib --out Stdout -- module_name::

# Skip problematic tests temporarily
cargo tarpaulin --lib --skip 'test_name_pattern'
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

## Quick Commands Reference

```bash
# Check if all tests pass
cargo test --all

# Generate coverage report
cargo tarpaulin --out html --output-dir target/coverage

# Run coverage for specific module
cargo tarpaulin --lib --out Stdout -- module_name::

# Run tests with single thread (for debugging)
cargo test -- --test-threads=1

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name -- --exact

# Check coverage without running tests
cargo tarpaulin --lib --ignore-tests --out Stdout
```

## When to Use Each Agent

- **Before starting**: Use `rust-test-fixer` to ensure all tests pass
- **Planning tests**: Use `rust-expert` for test architecture
- **Writing tests**: Use `general-purpose` to find similar patterns
- **Fixing failures**: Use `rust-test-fixer` for quick fixes
- **Complex issues**: Use `rust-troubleshooter-opus` for deep debugging
- **Code quality**: Use `rust-linting-expert` for test code style

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