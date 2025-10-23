---
agpm:
  templating: true
dependencies:
  snippets:
    - name: best_practices
      install: false
      path: ../rust-best-practices.md
---

## Your task

Perform a comprehensive pull request **review** for the AGPM project based on the arguments provided.

**IMPORTANT**: This command reviews changes and generates a report - it does NOT create or submit a pull request. It's designed to help you evaluate your changes before you decide to create a PR.

**IMPORTANT**: Batch related operations thoughtfully; schedule tool calls in Claude Code only in parallel when the workflow benefits from it.

## Best Practices

{{ agpm.deps.snippets.best_practices.content }}

**CRITICAL**: Use the Task tool to delegate to specialized agents for code analysis, NOT Grep or other direct tools. Agents have context about the project and can provide deeper insights.

## Approach

1. **Agent Delegation Strategy**:
   - Prefer the Task tool for broad or multi-file code analysis
   - Use direct Read/Grep commands for targeted inspections and pattern searches
   - Provide agents with specific context about what changed
   - Include relevant file paths and change summaries in prompts

2. Parse arguments to determine review target and type:

   **IMPORTANT**: First check what arguments were provided: $ARGUMENTS

   **Determine the review target** (in order of precedence):
   1. **DEFAULT (no arguments)**: Review uncommitted working directory changes
      - This is the PRIMARY use case - reviewing your work-in-progress before committing
      - Use `git status --short` to list modified/staged files
      - Use `git diff HEAD --stat` to see all uncommitted changes (staged + unstaged)
      - **DO NOT review branch commits or commit history when no arguments provided**
      - Examples: `/pr-self-review`, `/pr-self-review --quick`

   2. **DIFF keyword**: Review only staged (but uncommitted) changes
      - Arguments contain the DIFF keyword (e.g., `DIFF`, `HEAD..DIFF`, `HEAD~2..DIFF`)
      - DIFF represents staged changes ready for commit (`git diff --cached`)
      - For ranges like `HEAD..DIFF`: Use `git diff --cached HEAD --stat`
      - For ranges like `HEAD~2..DIFF`: Use `git diff --cached HEAD~2 --stat`
      - Use `git diff --cached --name-status` to list staged files
      - Examples: `/pr-self-review DIFF`, `/pr-self-review HEAD~2..DIFF`

   3. **Commit range**: Review multiple commits
      - Pattern: `<ref>..<ref>` (e.g., `abc123..def456`, `main..HEAD`, `origin/main..HEAD`)
      - Use `git log --oneline <range>` to see commit history
      - Use `git diff --stat <range>` and `git diff --name-status <range>` for changes
      - Examples: `/pr-self-review main..HEAD`, `/pr-self-review abc123..def456 --security`

   4. **Single commit**: Review one specific commit
      - Pattern: 6-40 character hex string (e.g., `abc123`, `5b3ee1d`)
      - Use `git show --stat <commit>` for commit details
      - Use `git diff-tree --no-commit-id --name-status -r <commit>` to list files
      - Examples: `/pr-self-review abc123`, `/pr-self-review 5b3ee1d --quick`

   **Determine the review type** from remaining arguments after the target:
   - `--quick`: Basic formatting and linting only
   - `--full`: Complete review with all checks (default if no flag specified)
   - `--security`: Focus on security implications
   - `--performance`: Focus on performance analysis

3. Run automated checks based on review type:

   **Quick Review (--quick)**:
   - Run these checks:
     - `cargo fmt` to ensure formatting
     - `cargo clippy -- -D warnings` to catch issues
     - `cargo nextest run` for tests
     - `cargo nextest run --profile all --test stress` for stress tests
     - `cargo test --doc` for doctests

   **Full Review (--full or default)**:
   - First, run quick checks (cargo fmt -- --check, clippy, nextest run)
   - Then use the Task tool to delegate to specialized agents IN PARALLEL:
     - Use Task with subagent_type="rust-linting-standard" to check formatting and linting issues
     - Use Task with subagent_type="rust-expert-standard" to review code quality, architecture, and adherence to `.agpm/snippets/rust-best-practices.md`
     - Use Task with subagent_type="rust-test-standard" to analyze test coverage, quality, and isolation (TestProject usage)
     - Use Task with subagent_type="rust-doc-standard" to review documentation completeness AND docstring accuracy/conciseness:
       ```
       Task(description="Review docstrings for accuracy and conciseness",
            prompt="Review all docstrings in changed files for:
            1. Accuracy - ensure docstrings correctly describe what the code does
            2. Conciseness - keep docstrings brief but informative, avoid verbosity
            3. Completeness - all public functions, structs, enums, and traits have proper documentation
            4. Examples - use `no_run` for code examples by default unless they should be executed as tests; use `ignore` for examples that won't compile
            5. Consistency - follow Rust documentation conventions and style
            Focus on changed files and highlight any docstrings that are inaccurate, too verbose, missing, or inconsistent with the code.",
            subagent_type="rust-doc-standard")
       ```
     - Only escalate to advanced agents (rust-expert-advanced, rust-troubleshooter-advanced) if initial review finds complex issues
   - **CRITICAL TEST CHECK**: Search for tests using global cache:
     - Look for files matching pattern: `TempDir::new()` + `Command::cargo_bin()` but NOT `TestProject` or `Cache::with_dir()`
     - This prevents race conditions in parallel CI test execution
   - Example Task invocation:
     ```
     Task(description="Review code quality",
          prompt="Review the changed files against .agpm/snippets/rust-best-practices.md covering imports, naming, error handling, ownership, and architecture...",
          subagent_type="rust-expert-standard")
     ```
   - Additional Task invocation for code cleanup:
     ```
     Task(description="Check for deprecated methods and code cleanup",
          prompt="Analyze changed files for:
          1. Deprecated methods - look for #[deprecated] attributes, TODO/FIXME comments suggesting removal, or methods that should be deleted entirely
          2. Code duplication - identify identical or very similar code blocks that could be refactored into shared functions
          3. Redundant imports - unused imports that should be removed
          4. Dead code - functions, structs, or methods that are never called or referenced
          5. Verbose docstrings - documentation that is excessively wordy or contains redundant information
          6. Orphan documentation - docs that reference removed APIs or outdated patterns
          7. Unused variables - check for variables prefixed with `_` that should be removed entirely, not just ignored
          Focus on recommending removal of deprecated code rather than migration paths. Prioritize cleanup and simplification.",
          subagent_type="rust-expert-standard")
     ```
   - Run full test suite and doc build IN PARALLEL:
     - `cargo nextest run` for parallel test execution
     - `cargo test --doc` for doctests
     - `cargo doc --no-deps`
   - Check cross-platform compatibility

     **Security Review (--security)**:

   - Use Task with subagent_type="rust-expert-standard" with security-focused prompt:
     ```
     Task(description="Security review",
          prompt="Review for security issues per .agpm/snippets/rust-best-practices.md: credentials in code, input validation, path traversal, unsafe operations, Windows path handling...",
          subagent_type="rust-expert-standard")
     ```
   - Additionally run targeted Grep searches IN PARALLEL:
     - Search for credential patterns: `(password|token|secret|api_key)\s*=\s*"`
     - Search for unsafe blocks: `unsafe\s+\{`
     - Search for path traversal: `\.\./`
     - Search for Windows path issues: `r"[A-Z]:\\|\\\\|CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9]"`
   - Verify no secrets in version-controlled files
   - Check proper path validation in utils/path_validation.rs

   **Performance Review (--performance)**:
   - Build in release mode: `cargo build --release`
   - Use Task with subagent_type="rust-expert-standard" with performance-focused prompt:
     ```
     Task(description="Performance review",
          prompt="Review for performance issues per .agpm/snippets/rust-best-practices.md: blocking operations in async code, unnecessary allocations, algorithmic complexity, lock contention, resource cleanup...",
          subagent_type="rust-expert-standard")
     ```
   - Additionally check for specific anti-patterns:
     - `.block_on()` in async contexts
     - `std::fs::` instead of `tokio::fs` in async code
     - Excessive cloning or allocations
     - Missing Drop implementations for resources
     - Potential deadlocks in parallel code
     - Blocking I/O in async functions

4. Manual review based on these key areas:

   **Code Quality**:
   - Adherence to `.agpm/snippets/rust-best-practices.md` (imports, naming, error handling, ownership)
   - DRY principles and code clarity
   - Cross-platform compatibility
   - Unnecessary renames (e.g., `thing()` → `get_thing()` without justification)
   - **Deprecated code removal**: Check for methods marked with `#[deprecated]` that should be removed entirely
   - **Code duplication**: Identify duplicate or very similar code blocks that should be refactored
   - **Unused variables**: Look for variables prefixed with `_` that should be removed entirely
   - **Dead code**: Functions, structs, or methods that are never referenced
   - **File size limits**: Ensure source files stay under 1,000 lines of code (excluding empty lines and comments). Use `cloc` to count lines of code: `cloc src/file.rs --include-lang=Rust`

   **Architecture**:
   - Module structure alignment with CLAUDE.md
   - Proper async/await usage
   - No circular dependencies

   **Security**:
   - No credentials in agpm.toml
   - Input validation for git commands
   - Atomic file operations

   **Testing**:
   - New functionality has tests
   - Tests follow isolation requirements (use TestProject, not global cache)
   - **CRITICAL**: All integration tests MUST use `TestProject` for cache isolation
   - Check for tests using `TempDir::new()` with `Command::cargo_bin()` but no `TestProject` or `Cache::with_dir()`
   - Platform-specific tests handled correctly

     **Documentation**:

   - Public APIs documented
   - **Docstrings reviewed for accuracy and conciseness**:
     - Ensure docstrings correctly describe what the code does
     - Keep docstrings brief but informative, avoid verbosity
     - Verify examples use proper attributes (`no_run` by default, `ignore` for non-compilable examples)
     - **Verbose docstrings**: Identify documentation that is excessively wordy or contains redundant information
     - **Orphan documentation**: Check for docs that reference removed APIs, outdated patterns, or non-existent code
   - README.md accuracy check
   - CLAUDE.md reflects architectural changes
   - AGENTS.md updated for architectural changes
   - Examples in docs/ updated if relevant
   - Help text and man page consistency

5. Generate a summary report with:
   - **Changes Overview**: What was modified
   - **Test Results**: Pass/fail status of automated checks
   - **Issues Found**: Any problems discovered (grouped by severity)
   - **Security Analysis**: Security implications if any
   - **Performance Impact**: Performance considerations
   - **Recommendations**: Approve, request changes, or needs discussion

6. Focus only on tracked files - ignore untracked files marked with ?? in git status

Examples of usage:

**DEFAULT - Review uncommitted changes (most common)**:

- `/pr-review` - full review of all uncommitted changes (staged + unstaged)
- `/pr-review --quick` - quick review of uncommitted changes
- `/pr-review --security` - security-focused review of uncommitted changes
- `/pr-review --performance` - performance-focused review of uncommitted changes

**DIFF - Review only staged changes**:

- `/pr-review DIFF` - review staged changes ready for commit
- `/pr-review DIFF --quick` - quick review of staged changes
- `/pr-review HEAD..DIFF` - review the most recent commit plus staged changes
- `/pr-review HEAD~2..DIFF` - review the last 2 commits plus staged changes

**Single commit review**:

- `/pr-review abc123` - full review of specific commit abc123
- `/pr-review HEAD~1 --quick` - quick review of the previous commit
- `/pr-review 5b3ee1d --security` - security review of commit 5b3ee1d

**Commit range review**:

- `/pr-review main..HEAD` - full review of all changes from main to HEAD
- `/pr-review abc123..def456 --quick` - quick review of commits between abc123 and def456
- `/pr-review origin/main..HEAD --security` - security review of all changes not yet in origin/main
- `/pr-review HEAD~3..HEAD` - review the last 3 commits as a range

**Note**: This command only reviews and reports on changes. To create an actual pull request after review, use the `gh-pr-create` command.
