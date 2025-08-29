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

Perform a comprehensive pull request review for the CCPM project based on the arguments provided:

1. Parse the review type from arguments:
   - `--quick`: Basic formatting and linting only
   - `--full`: Complete review with all checks (default)
   - `--security`: Focus on security implications
   - `--performance`: Focus on performance analysis
   - Arguments: $ARGUMENTS

2. Run automated checks based on review type:

   **Quick Review (--quick)**:
   - Run `cargo fmt` to fix formatting
   - Run `cargo clippy -- -D warnings` to catch issues
   - Run basic tests with `cargo test --lib`

   **Full Review (--full or default)**:
   - All quick review checks
   - Use specialized agents for deep analysis:
     * `rust-linting-expert` for formatting and linting
     * `rust-expert` for architecture and API design
     * `rust-troubleshooter-opus` for memory and safety issues
     * `rust-test-fixer` if tests are failing
   - Run full test suite: `cargo test --all`
   - Build documentation: `cargo doc --no-deps`
   - Check cross-platform compatibility

   **Security Review (--security)**:
   - Search for credential patterns in changed files
   - Validate input sanitization in git operations
   - Check for path traversal vulnerabilities
   - Review file system operations for safety
   - Verify no secrets in version-controlled files

   **Performance Review (--performance)**:
   - Build in release mode: `cargo build --release`
   - Check for blocking operations in async code
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