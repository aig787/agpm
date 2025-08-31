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

**CRITICAL**: Use the Task tool to delegate to specialized agents for code analysis, NOT Grep or other direct tools. Agents have context about the project and can provide deeper insights.

1. **Agent Delegation Strategy**:
   - ALWAYS use Task tool for code analysis, NOT direct Grep/Read
   - Provide agents with specific context about what changed
   - Run multiple Task invocations in parallel for efficiency
   - Include relevant file paths and change summaries in prompts

2. Parse the review type from arguments:
   - `--quick`: Basic formatting and linting only
   - `--full`: Complete review with all checks (default)
   - `--security`: Focus on security implications
   - `--performance`: Focus on performance analysis
   - Arguments: $ARGUMENTS

3. Run automated checks based on review type:

   **Quick Review (--quick)**:
   - Run these checks IN PARALLEL using multiple tool calls in a single message:
     * `cargo fmt` to fix formatting
     * `cargo clippy -- -D warnings` to catch issues
     * `cargo test --lib` for basic tests

   **Full Review (--full or default)**:
   - First, run quick checks IN PARALLEL (cargo fmt, clippy, test --lib)
   - Then use the Task tool to delegate to specialized agents IN PARALLEL:
     * Use Task with subagent_type="rust-linting-standard" to check formatting and linting issues
     * Use Task with subagent_type="rust-expert-standard" to review code quality, architecture, and best practices
     * Use Task with subagent_type="rust-test-standard" to analyze test coverage and quality
     * Use Task with subagent_type="rust-doc-standard" to review documentation completeness
     * Only escalate to advanced agents (rust-expert-advanced, rust-troubleshooter-advanced) if initial review finds complex issues
   - Example Task invocation:
     ```
     Task(description="Review code quality", 
          prompt="Review the changed files for Rust best practices, error handling, and architecture...", 
          subagent_type="rust-expert-standard")
     ```
   - Run full test suite and doc build IN PARALLEL:
     * `cargo test --all`
     * `cargo doc --no-deps`
   - Check cross-platform compatibility

   **Security Review (--security)**:
   - Use Task with subagent_type="rust-expert-standard" with security-focused prompt:
     ```
     Task(description="Security review", 
          prompt="Review for security issues: credentials in code, input validation, path traversal, unsafe operations...", 
          subagent_type="rust-expert-standard")
     ```
   - Additionally run targeted Grep searches IN PARALLEL:
     * Search for credential patterns: `(password|token|secret|api_key)\s*=\s*"`
     * Search for unsafe blocks: `unsafe\s+\{`
     * Search for path traversal: `\.\./`
   - Verify no secrets in version-controlled files

   **Performance Review (--performance)**:
   - Build in release mode: `cargo build --release`
   - Use Task with subagent_type="rust-expert-standard" with performance-focused prompt:
     ```
     Task(description="Performance review",
          prompt="Review for performance issues: blocking operations in async code, unnecessary allocations, algorithmic complexity...",
          subagent_type="rust-expert-standard")
     ```
   - Additionally check for specific anti-patterns:
     * `.block_on()` in async contexts
     * `std::fs::` instead of `tokio::fs` in async code
     * Excessive cloning or allocations

4. Manual review based on these key areas:

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

5. Generate a summary report with:
   - **Changes Overview**: What was modified
   - **Test Results**: Pass/fail status of automated checks
   - **Issues Found**: Any problems discovered (grouped by severity)
   - **Security Analysis**: Security implications if any
   - **Performance Impact**: Performance considerations
   - **Recommendations**: Approve, request changes, or needs discussion

6. Focus only on tracked files - ignore untracked files marked with ?? in git status

Examples of usage:
- `/pr-review` - performs full comprehensive review
- `/pr-review --quick` - quick formatting and linting check
- `/pr-review --security` - focused security review
- `/pr-review --performance` - performance-focused analysis