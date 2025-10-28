# Cargo Commands for AGPM Development

## Quick Reference

```bash
# Core development
cargo check                 # Quick type/compile check (fastest)
cargo build                 # Compile project
cargo build --release       # Optimized build for production
cargo run                   # Run the main binary

# Testing (AGPM uses nextest)
cargo nextest run           # Primary test runner for AGPM (parallel, fast)
cargo test --doc            # Run doctests
cargo test                  # Fallback test runner (sequential)

# Code quality
cargo fmt                   # Format code
cargo clippy                # Run linter
cargo clippy --fix --allow-dirty  # Auto-fix issues with uncommitted changes
```

## Common AGPM Workflows

### Pre-commit Checklist

```bash
# Run in this order before committing
cargo fmt                              # Format code
cargo clippy -- -D warnings            # Ensure no warnings
cargo nextest run                      # Run all tests
cargo test --doc                       # Run doctests
```

### Iterative Development

```bash
# Quick feedback loop during development
cargo check              # Fastest compile check
cargo run                # Test your changes
cargo nextest run        # Verify tests pass
cargo nextest run --no-capture --test $TEST # Run a specific test with output going to stdout
```

### Full Project Build

```bash
# For releases or testing performance
cargo build --release   # Optimized with LTO
```

## Development Commands

### Building

```bash
cargo build              # Debug build (fast compilation, includes debug info)
cargo build --release    # Release build (optimized, slower compile)
cargo check              # Quick check without generating binaries
                         # Use during development for fast feedback
cargo check --all-targets  # Check tests, benches, examples too
```

### Running

```bash
cargo run                # Run main binary
cargo run --bin agpm     # Explicitly run the agpm binary
cargo run --example foo  # Run a specific example
```

## Testing in AGPM

### Primary Test Commands

```bash
cargo nextest run        # AGPM's primary test runner
                         # Features: parallel execution, smart test selection
                         # Note: All tests must be parallel-safe in AGPM

cargo test --doc         # Run doctests (documentation examples)
                         # Use `no_run` attribute by default in doctests
```

### Test Execution Options

```bash
# When you need standard cargo test
cargo test                               # Sequential test execution
cargo test --release                    # Test in release mode
cargo test -- --test-threads=1          # Run serially (useful for debugging)
cargo test -- --ignored                 # Run ignored tests
cargo test -- --nocapture               # Show println! output
cargo test --all-features               # Test with all features enabled
```

### Debugging Tests

```bash
RUST_BACKTRACE=1 cargo test           # Stack traces on test failures
RUST_BACKTRACE=full cargo test        # Detailed backtraces
RUST_LOG=debug cargo nextest run      # Enable debug logging during tests
```

### Test Troubleshooting

```bash
# When tests pass but shouldn't (caching issues)
cargo clean && cargo test            # Clean and rebuild
rm -rf target && cargo test          # Full clean rebuild

# Run specific test
cargo nextest run test_name           # Run specific test
cargo nextest run --package agpm     # Run tests for specific package
```

## Code Quality & Analysis

### Formatting

```bash
cargo fmt                           # Format all source files
cargo fmt -- --check                # Check if code is formatted (CI)
cargo fmt --all                     # Format dependencies too (rarely needed)
```

### Linting with Clippy

```bash
cargo clippy                        # Run clippy lints
cargo clippy -- -D warnings         # Treat warnings as errors (CI requirement)
cargo clippy --fix                  # Auto-fix simple issues
cargo clippy --fix --allow-dirty    # Fix issues even with uncommitted changes
                                   # AGPM tip: Use this during development
cargo clippy --all-targets          # Include tests, benches, examples
cargo clippy --workspace            # Run on all workspace members
```

### Documentation

```bash
cargo doc                          # Generate documentation
cargo doc --no-deps                # Generate docs for workspace only
cargo doc --open                   # Generate and open in browser
cargo doc --document-private-items # Include private items in docs
```

## Dependency Management

### Viewing Dependencies

```bash
cargo tree                         # Show dependency tree
cargo tree --format "{p}"         # Just package names
cargo tree --duplicates           # Show duplicate dependencies
cargo tree --invert               # Show what depends on a crate
```

### Checking Updates

```bash
cargo outdated                     # Check for outdated dependencies
cargo update                       # Update dependencies (minor/patch)
cargo update -p crate_name        # Update specific crate
```

### Security

```bash
cargo audit                        # Check for security vulnerabilities
cargo audit --fix                  # Auto-fix vulnerable dependencies
```

## Debugging & Analysis

### Macro Expansion

```bash
cargo expand                       # Expand macros in main
cargo expand --bin agpm           # Expand macros in specific binary
cargo expand --test test_name     # Expand macros in test
```

## Coverage Analysis

```bash
# Requires cargo-llvm-cov plugin
cargo install cargo-llvm-cov       # Install once
cargo llvm-cov                     # Generate coverage report
cargo llvm-cov --html              # Generate HTML report (opens in browser)
cargo llvm-cov --lcov              # Generate LCOV format (for CI)
cargo llvm-cov --workspace         # Coverage for entire workspace
```

## Workspace Commands

```bash
# AGPM uses cargo workspace
cargo check --workspace            # Check all workspace members
cargo build --workspace            # Build all packages
cargo test --workspace             # Test all packages
cargo clippy --workspace           # Lint all packages
```

## Environment Variables

```bash
# Common variables for debugging
RUST_LOG=debug cargo run           # Enable debug logging
RUST_LOG=trace cargo run           # Enable trace logging (very verbose)
RUST_BACKTRACE=1 cargo run         # Stack traces on panic
RUST_BACKTRACE=full cargo run      # Full backtraces
```

## Clean Up

```bash
cargo clean                        # Remove target directory
cargo clean --release              # Remove release artifacts only
cargo clean --package crate_name   # Clean specific package
```

## AGPM-Specific Notes

- **Always use `cargo nextest run`** for tests, not `cargo test` (except for doctests)
- **All tests must be parallel-safe** - no serial tests allowed
- **Use `cargo clippy --fix --allow-dirty`** during development to fix issues without committing
- **CI requirements**: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo nextest run`, `cargo test --doc` must all pass
- **Use `--workspace` flag** when working with multiple packages in the workspace
