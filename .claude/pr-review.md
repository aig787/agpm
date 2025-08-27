# PR Review Instructions for CCPM

## Overview
You are reviewing a pull request for the CCPM (Claude Code Package Manager) project, a Rust-based Git package manager for Claude Code resources. The project uses a lockfile-based dependency management model similar to Cargo.

## Review Process

### Step 1: Understand the Changes
First, examine the git diff to understand what's being changed:
```bash
git diff --cached  # For staged changes (tracked files only)
git diff HEAD      # For all changes to tracked files
git log --oneline -n 5  # Recent commit context
```

**Note**: The review should focus ONLY on tracked files. Untracked files (marked with ?? in git status) should be excluded from the review process.

### Step 2: Automated Analysis
Use specialized agents to perform thorough code analysis:

#### For Rust Code Issues
1. **Linting and Formatting** - Use `rust-linting-expert` agent:
   - Run `cargo fmt --check` to verify formatting
   - Run `cargo clippy -- -D warnings` to catch common issues
   - Ensure all auto-fixable issues are resolved

2. **Complex Rust Issues** - Use `rust-expert` agent for:
   - API design review
   - Performance implications
   - Idiomatic Rust patterns
   - Refactoring suggestions
   - Module structure improvements

3. **Memory and Safety** - Use `rust-troubleshooter-opus` agent for:
   - Memory leak detection
   - Undefined behavior risks
   - Unsafe code review
   - Concurrency issues
   - Deep performance analysis

4. **Test Failures** - Use `rust-test-fixer` agent for:
   - Fixing broken tests
   - Assertion failures
   - Missing test coverage
   - Test setup issues

### Step 3: Manual Review Checklist

#### Code Quality
- [ ] **Rust Best Practices**
  - Uses `Result<T, E>` for error handling
  - Follows Rust naming conventions
  - Appropriate use of ownership and borrowing
  - No unnecessary clones or allocations
  - Unused variables are removed (not just prefixed with `_`)
  - If unused variables must be kept, they have comments explaining why

- [ ] **DRY Principles & Code Clarity**
  - No duplicated code blocks (extract to functions/modules)
  - Common patterns extracted to reusable utilities
  - Test helpers and fixtures avoid repetition
  - Constants used for repeated values
  - Function names clearly express intent
  - Variables have descriptive names
  - Complex logic has explanatory comments
  - Avoid clever code in favor of readable code
  - Functions do one thing well (Single Responsibility)
  - Consistent naming patterns across similar operations

- [ ] **Error Handling**
  - Comprehensive error messages with context
  - Uses `anyhow` for context chaining appropriately
  - Custom errors use `thiserror` derive
  - User-facing errors are helpful and actionable

- [ ] **Cross-Platform Compatibility**
  - Path handling uses `PathBuf` and proper separators
  - Platform-specific code uses `cfg!` macros
  - Works on Windows, macOS, and Linux
  - No hardcoded shell commands or paths

#### Architecture & Design
- [ ] **Module Structure**
  - Changes align with the modular architecture in CLAUDE.md
  - Each module maintains single responsibility
  - Dependencies between modules are appropriate
  - No circular dependencies

- [ ] **Async/Concurrency**
  - Long-running operations use `tokio` async/await
  - No blocking operations in async contexts
  - Proper error propagation in async code
  - Progress indicators for long operations

#### Security Review
- [ ] **Credentials Security**
  - No credentials in `ccpm.toml` or version-controlled files
  - Auth tokens only in `~/.ccpm/config.toml`
  - No hardcoded secrets or tokens

- [ ] **Input Validation**
  - Git command inputs are sanitized
  - Repository URLs are validated
  - No path traversal vulnerabilities
  - Version constraints are validated

- [ ] **File System Safety**
  - Atomic file operations (write to temp, then rename)
  - Proper permission handling
  - No writes outside project directory
  - Symlink attack prevention

#### Testing
- [ ] **Test Coverage**
  - New functionality has unit tests
  - Integration tests for CLI commands
  - Tests follow isolation requirements (no `std::env::set_var`)
  - Each test uses its own temp directory

- [ ] **Test Quality**
  - Tests are deterministic
  - No global state dependencies
  - Platform-specific tests use appropriate cfg attributes
  - Tests pass on all platforms

#### Documentation
- [ ] **Code Documentation**
  - Public APIs have doc comments
  - Complex logic is explained
  - CLAUDE.md updated if architecture changes
  - README.md updated for user-facing changes

- [ ] **Documentation Accuracy Check**
  - README.md is still accurate after all changes
    - Installation instructions remain correct
    - Usage examples still work as documented
    - Command descriptions match current implementation
    - Feature list reflects actual capabilities
    - All example code snippets are valid
  - CLAUDE.md reflects any architectural changes
    - Module structure documentation is current
    - Security rules are still being followed
    - Development guidelines remain applicable
  - USAGE.md (if present) matches current CLI behavior
  - CONTRIBUTING.md procedures still valid
  - Any linked documentation files are accurate
  - Version numbers in examples match current version
  - All internal links and references are not broken

### Step 4: Performance & Build
Run these checks:
```bash
# Build checks
cargo build --release
cargo test --all
cargo doc --no-deps

# Coverage (if significant changes)
cargo tarpaulin --out html

# Benchmarks (if performance-critical changes)
cargo bench
```

### Step 5: Specific CCPM Concerns

#### Manifest/Lockfile System
- [ ] `ccpm.toml` changes are backward compatible
- [ ] Lockfile generation is deterministic
- [ ] Dependency resolution handles conflicts correctly
- [ ] Version constraints work as expected

#### Git Operations
- [ ] Uses system git command (not git2 library)
- [ ] Handles git failures gracefully
- [ ] Supports different authentication methods
- [ ] Cache operations are atomic

#### Resource Management
- [ ] Markdown files are parsed correctly
- [ ] Frontmatter metadata is preserved
- [ ] Resource checksums are validated
- [ ] Installation copies (not symlinks) work correctly

### Step 6: Summary Report

Provide a summary with:
1. **Impact Assessment**: What does this change affect?
2. **Risk Analysis**: Potential issues or breaking changes
3. **Test Results**: What was tested and results
4. **Security Review**: Any security implications
5. **Performance Impact**: Changes to performance characteristics
6. **Recommendations**: Approve, request changes, or needs discussion

## Example Review Commands

```bash
# Quick quality check
cargo fmt --check && cargo clippy -- -D warnings && cargo test

# Full pre-merge validation
cargo fmt && cargo clippy -- -D warnings && cargo test && cargo doc --no-deps

# Performance validation
cargo build --release && cargo bench

# Platform-specific testing
cargo test --target x86_64-pc-windows-gnu  # Cross-compile test
```

## When to Use Specialized Agents

- **rust-linting-expert**: First pass for any Rust code changes
- **rust-expert**: Architecture changes, new modules, API design
- **rust-test-fixer**: When tests are failing or need updates
- **rust-troubleshooter-opus**: Performance issues, memory problems, complex bugs
- **general-purpose**: Researching dependencies, checking documentation

## Red Flags to Watch For

1. **Breaking Changes**: Changes to public APIs or manifest format
2. **Security Issues**: New network operations, file system access, credential handling
3. **Platform Regressions**: Code that might break cross-platform compatibility
4. **Test Degradation**: Reduced coverage, disabled tests, or flaky tests
5. **Performance Regressions**: New blocking operations, unnecessary allocations
6. **Dependency Bloat**: Adding heavy dependencies for simple functionality

## Note on Test Environment Variables
Tests must NEVER use `std::env::set_var` (causes race conditions). Instead:
- Pass env vars to Command instances using `.env()`
- Refactor functions to accept env vars as parameters
- Only exception: tests explicitly testing env var functionality (must be documented)