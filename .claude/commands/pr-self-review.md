---
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo test:*), Bash(cargo build:*), Bash(cargo doc:*), Task, Grep, Read, LS
description: Perform comprehensive PR review for CCPM project
argument-hint: [ --quick | --full | --security | --performance ] - e.g., "--quick" for basic checks only
---

## Context

- Current changes: !`git diff HEAD`
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -5`

## Your task

Perform a comprehensive pull request review for the CCPM project based on the arguments provided.

**IMPORTANT**: Always run multiple independent operations IN PARALLEL by using multiple tool calls in a single message. This significantly improves performance.

1. Parse the review type from arguments:
   - `--quick`: Basic formatting and linting only
   - `--full`: Complete review with all checks (default)
   - `--security`: Focus on security implications
   - `--performance`: Focus on performance analysis
   - Arguments: $ARGUMENTS

2. Run automated checks based on review type:

   **Quick Review (--quick)**:
   - Run these checks IN PARALLEL using multiple tool calls in a single message:
     * `cargo fmt` to fix formatting
     * `cargo clippy -- -D warnings` to catch issues
     * `cargo test --lib` for basic tests

   **Full Review (--full or default)**:
   - First, run quick checks IN PARALLEL (cargo fmt, clippy, test --lib)
   - Then use specialized agents IN PARALLEL for deep analysis:
     * `rust-linting-expert` for formatting and linting (delegates complex refactoring to rust-expert)
     * `rust-expert` for architecture and API design review (handles implementation and refactoring)
     * `rust-troubleshooter-opus` for memory safety, undefined behavior, and performance issues (use when rust-expert cannot resolve)
     * `rust-test-fixer` if tests are failing (handles assertion failures, test setup issues)
     * `rust-doc-expert` for documentation quality review (ensures comprehensive docs with examples)
   - Run full test suite and doc build IN PARALLEL:
     * `cargo test --all`
     * `cargo doc --no-deps`
   - Check cross-platform compatibility

   **Security Review (--security)**:
   - Run these searches IN PARALLEL using multiple Grep calls:
     * Search for credential patterns (tokens, passwords, secrets)
     * Search for unsafe input handling in git operations
     * Search for path traversal patterns (../, absolute paths)
     * Search for unsafe file operations
   - Verify no secrets in version-controlled files

   **Performance Review (--performance)**:
   - Build in release mode: `cargo build --release`
   - Check for blocking operations IN PARALLEL using multiple Grep calls:
     * Search for `.block_on()` calls in async functions
     * Search for `std::fs::` usage in async contexts (should be tokio::fs)
     * Search for `std::sync::Mutex` in async contexts
     * Search for `std::thread::sleep` in async code
   - Look for unnecessary allocations or clones
   - Review algorithmic complexity

3. Manual review based on these key areas:

   **Code Quality**:
   - Rust best practices (Result usage, ownership, borrowing)
   - DRY principles and code clarity
   - Comprehensive error handling
   - Cross-platform compatibility

   **Architecture**:
   - Module structure alignment with CLAUDE.md
   - Proper async/await usage
   - No circular dependencies

   **Security**:
   - No credentials in ccpm.toml
   - Input validation for git commands
   - Atomic file operations

   **Testing**:
   - New functionality has tests
   - Tests follow isolation requirements
   - Platform-specific tests handled correctly

   **Documentation**:
   - Public APIs documented
   - README.md accuracy check
   - CLAUDE.md reflects architectural changes

4. Generate a summary report with:
   - **Changes Overview**: What was modified
   - **Test Results**: Pass/fail status of automated checks
   - **Issues Found**: Any problems discovered (grouped by severity)
   - **Security Analysis**: Security implications if any
   - **Performance Impact**: Performance considerations
   - **Recommendations**: Approve, request changes, or needs discussion

5. Focus only on tracked files - ignore untracked files marked with ?? in git status

Examples of usage:
- `/pr-review` - performs full comprehensive review
- `/pr-review --quick` - quick formatting and linting check
- `/pr-review --security` - focused security review
- `/pr-review --performance` - performance-focused analysis