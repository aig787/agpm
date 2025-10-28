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

3. **Detect if this is a historical review**:

   **IMPORTANT**: Check if we're reviewing historical changes vs current work-in-progress.

   **Simple Detection Logic**:
   - **Historical Review** (skip automated checks):
     - Contains `..` but does NOT end with "DIFF" (commit range like `abc123..def456` or `main..HEAD`)
     - Single commit hash (like `abc123`)
     - Branch name (like `main` when not combined with `..`)

   - **Current Work Review** (run automated checks):
     - No arguments (uncommitted changes)
     - Contains "DIFF" (including ranges ending in "DIFF" like `HEAD..DIFF` or `HEAD~2..DIFF`)

   **Why this matters**:
   - **Historical**: Automated checks would test current code, not the commits being reviewed
   - **Current**: Automated checks test what you're about to commit

   **For historical reviews**:
   - Display: "⚠️  Historical Review: automated checks skipped (would test current code, not historical state)"
   - Suggest: "To test historical code: git checkout <commit> && cargo test"
   - Proceed directly to manual code review (step 5)

   **For current work**:
   - Run automated checks as usual (step 4)

4. **Detect changeset size and adapt review strategy**:

   **IMPORTANT**: Before running full reviews, analyze the changeset size to determine the appropriate approach.

   **Get changeset statistics**:
   - For uncommitted changes: `git diff HEAD --stat`
   - For single commit: `git show --stat <commit>`
   - For commit range: `git diff --stat <range>`
   - Parse the summary line (e.g., "42 files changed, 1523 insertions(+), 891 deletions(-)")

   **Categorize changeset size**:
   - **Small** (<500 lines): Standard single-pass review (existing behavior)
   - **Medium** (500-2000 lines): Standard review with progress tracking
   - **Large** (2000-5000 lines): Chunked review with parallel processing
   - **Massive** (>5000 lines): Chunked review + warn user about scope
   - **Extreme** (>20000 lines): Suggest alternatives, proceed with best-effort

   **For Large/Massive changesets, prepare chunks**:

   a. **Get detailed file list with line counts**:
      ```bash
      git diff --numstat <target> | grep -v "^-"  # Filter out binary files
      ```
      This outputs: `<additions> <deletions> <filepath>` per line

   b. **Group files by module/directory**:
      - Priority 1 (Critical): `src/core/`, `src/resolver/`, `src/installer/`
      - Priority 2 (Security): `src/git/`, `src/config/`, `src/utils/path_validation.rs`
      - Priority 3 (Standard): Other `src/` modules
      - Priority 4 (Low): `tests/`, `docs/`, config files

   c. **Create balanced chunks**:
      - Target: ~1000-1500 total lines (additions + deletions) per chunk
      - Keep related files together (same module/directory)
      - Sort by priority, create chunks that respect module boundaries
      - Example: "Chunk 1: src/resolver/ (4 files, 1200 lines)"

   d. **Notify user of strategy**:
      ```
      Detected LARGE changeset: 42 files, 3500 lines changed
      Strategy: Parallel chunked review (4 chunks)
      - Chunk 1: src/core/ + src/resolver/ (8 files, ~1200 lines)
      - Chunk 2: src/installer/ + src/lockfile/ (10 files, ~1000 lines)
      - Chunk 3: src/templating/ + src/mcp/ (12 files, ~900 lines)
      - Chunk 4: tests/ (12 files, ~400 lines)
      ```

   e. **Use TodoWrite to track chunks**:
      Create a todo list with one item per chunk:
      ```
      TodoWrite([
          {content: "Review chunk 1/4: src/core/ + src/resolver/ (8 files)", status: "pending"},
          {content: "Review chunk 2/4: src/installer/ + src/lockfile/ (10 files)", status: "pending"},
          {content: "Review chunk 3/4: src/templating/ + src/mcp/ (12 files)", status: "pending"},
          {content: "Review chunk 4/4: tests/ (12 files)", status: "pending"},
          {content: "Aggregate findings and generate report", status: "pending"}
      ])
      ```

   **For Extreme changesets (>20k lines)**:
   - Warn: "⚠️  EXTREME changeset detected: 65k lines changed across 150 files"
   - Suggest: "Consider reviewing by smaller commit ranges or individual commits instead"
   - Offer to proceed: "Proceeding with best-effort chunked review (may take significant time)"

5. Run automated checks (only for current work):

   **IMPORTANT**: Skip automated checks for historical reviews - they would test current code, not the commits being reviewed.

   **Historical Review**: Display warning and proceed to step 6 (manual review).

   **Current Work Review**: Run these automated checks:
   - `cargo fmt` (formatting)
   - `cargo clippy -- -D warnings` (linting)
   - `cargo nextest run` (unit/integration tests)
   - `cargo test --doc` (doctests)

   **Quick Review (--quick)**:
   - Run these checks:
     - `cargo fmt` to ensure formatting
     - `cargo clippy -- -D warnings` to catch issues
     - `cargo nextest run` for tests
     - `cargo nextest run --profile all --test stress` for stress tests
     - `cargo test --doc` for doctests

   **Full Review (--full or default)**:
   - First, run quick checks (cargo fmt, clippy, nextest run)

   **Agent Delegation Strategy** (adapts based on changeset size):

   **For Small/Medium changesets (<2000 lines)** - Single-pass review:
   - Use the Task tool to delegate to specialized agents IN PARALLEL:
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
   - **Systematic Code Duplication Detection**:
     - Use targeted Grep searches to find duplicate patterns:
       ```
       # Find similar function signatures (potential duplication)
       Grep(pattern="pub fn \w+\([^)]+\) -> [^{]+ \{", type="rust", output_mode="content", -n)

       # Find similar error handling patterns
       Grep(pattern="anyhow::bail|anyhow::ensure|return Err\(", type="rust", output_mode="content", -n)

       # Find similar match patterns that could be refactored
       Grep(pattern="match \w+ \{[\s\S]*?\}", type="rust", output_mode="content", -n)
       ```
     - Check for repeated async/await patterns
     - Identify similar file I/O operations
     - Look for duplicate validation logic

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
          8. Similar function patterns - look for functions with nearly identical logic that could be unified
          9. Repeated error handling - identify duplicate error creation/propagation patterns
          Focus on recommending removal of deprecated code rather than migration paths. Prioritize cleanup and simplification.",
          subagent_type="rust-expert-standard")
     ```
   - Run full test suite and doc build IN PARALLEL:
     - `cargo nextest run` for parallel test execution
     - `cargo test --doc` for doctests
     - `cargo doc --no-deps`
   - Check cross-platform compatibility

   **For Large/Massive changesets (≥2000 lines)** - Chunked parallel review:

   **IMPORTANT**: For large changesets, process chunks in parallel to stay within context limits while maintaining thorough coverage.

   **For each chunk (process 3-4 chunks in parallel)**:

   a. **Mark chunk as in_progress** using TodoWrite before starting

   b. **Get files for this chunk**:
      ```bash
      # Extract files for this chunk based on the chunking strategy from step 3
      # Example: For chunk 1 (src/resolver/), get all changed files in that directory
      git diff --name-only <target> | grep "^src/resolver/"
      ```

   c. **Get focused diff for chunk**:
      ```bash
      # Get only the diff for files in this chunk
      git diff <target> -- <file1> <file2> <file3>...
      ```

   d. **Launch parallel agent tasks** for this chunk:
      - Each agent gets:
        - **Chunk context**: "Reviewing chunk 2/5: src/installer/ + src/lockfile/ modules (10 files, ~1000 lines)"
        - **Full changeset scope**: Brief summary of what the entire PR changes
        - **Files in chunk**: List of specific files with line change counts
        - **Focused diff**: Only the changes for files in this chunk
        - **Module context**: Understanding of what this module does in the system

      Example prompt structure:
      ```
      Task(description="Review chunk 2/5: installer module",
           prompt="You are reviewing PART of a larger changeset as part of a chunked review strategy.

           FULL CHANGESET SCOPE:
           - Total: 42 files, 3500 lines changed
           - Focus: Refactoring dependency resolution and installation logic
           - This is chunk 2 of 5

           YOUR CHUNK (src/installer/ + src/lockfile/):
           - Files: src/installer/mod.rs (+234/-156), src/installer/resource_installer.rs (+89/-42), src/lockfile/mod.rs (+123/-67), ...
           - Total: 10 files, ~1000 lines
           - Module purpose: Handles resource installation and lockfile management

           Review this chunk for:
           1. Code quality and adherence to .agpm/snippets/rust-best-practices.md
           2. Architecture alignment with resolver changes (from chunk 1)
           3. Error handling consistency
           4. Test coverage and TestProject usage
           5. Cross-module interaction impacts

           [Include focused diff here]

           Provide findings specific to this chunk. Note any cross-chunk concerns.",
           subagent_type="rust-expert-standard")
      ```

      Launch similar tasks for:
      - `rust-linting-standard` - linting issues in this chunk
      - `rust-test-standard` - test coverage for this chunk
      - `rust-doc-standard` - documentation for this chunk

   e. **Store chunk findings**:
      - Collect agent responses for this chunk
      - Tag findings with chunk identifier (e.g., "Chunk 2: installer")
      - Note any cross-chunk concerns flagged by agents

   f. **Mark chunk as completed** using TodoWrite when all agents finish

   **After all chunks complete**:
   - Mark "Aggregate findings and generate report" todo as in_progress
   - Run full test suite (tests entire codebase, not individual chunks):
     - `cargo nextest run` - run all tests (not just changed modules)
     - `cargo test --doc` - verify all doctests
     - `cargo doc --no-deps` - ensure documentation builds
     - **NOTE**: For massive changesets (>5000 lines), full codebase testing is essential to catch cross-module integration issues
   - Proceed to result aggregation (see section 5.5)

   **Chunk processing example** (3 chunks in parallel):
   ```
   # Launch chunks 1, 2, 3 in parallel with Task tool
   Task(chunk 1: core/resolver) | Task(chunk 2: installer/lockfile) | Task(chunk 3: templating/mcp)

   # While chunks run, TodoWrite shows progress:
   ✓ Review chunk 1/4: src/core/ + src/resolver/
   ⊙ Review chunk 2/4: src/installer/ + src/lockfile/  (in progress)
   ⊙ Review chunk 3/4: src/templating/ + src/mcp/      (in progress)
   ⊙ Review chunk 4/4: tests/                          (pending)

   # When batch completes, launch next batch
   Task(chunk 4: tests)
   ```

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
     - Search for SQL injection patterns: `\b(format!.*SELECT|format!.*INSERT|format!.*UPDATE|format!.*DELETE)\s*\(`
     - Search for command injection: `Command::new\([^)]+\).arg\(.*format!|std::process::Command`
     - Search for hardcoded temporary files: `/tmp|\\temp`
     - Search for weak crypto: `md5|sha1\(|crypt\(`
     - Search for unvalidated input: `unwrap\(\)|expect\("|\?` (in parsing contexts)
     - **CRITICAL**: unwrap() and expect() are FORBIDDEN in ALL code including tests
     - **Exception**: Each unwrap() MUST have a comment justifying WHY it's acceptable:
       ```rust
       // System invariant: cannot fail due to validation above
       let value = config.timeout.unwrap();

       // TODO: Remove by v1.2.0 - temporary during refactoring
       let legacy = old_api.get_value().unwrap();
       ```
     - Search for eval-like patterns: `exec\(|eval\(|system\(`
     - Analyze unsafe blocks with:
       ```
       Grep(pattern="unsafe\s+\{[\s\S]*?\}", type="rust", output_mode="content", -A 5)
       ```
   - Verify no secrets in version-controlled files
   - Check proper path validation in utils/path_validation.rs

   **Deprecated Methods and Code Cleanup Detection**:
   - Run targeted searches for deprecated patterns:
     ```
     # Find deprecated attributes
     Grep(pattern="#\[deprecated[^\]]*\]", type="rust", output_mode="content", -C 2)

     # Find TODO/FIXME comments suggesting removal
     Grep(pattern="(?i)// TODO.*remove|FIXME.*delete|TODO.*deprecated", type="rust", output_mode="content", -n)
     Grep(pattern="(?i)/\*[\s\S]*?(TODO|FIXME).*remove.*?[\s\S]*?\*/", type="rust", output_mode="content", -n)

     # Find commented out code that should be removed
     Grep(pattern="^\s*//\s*(let|fn|struct|enum|impl|use|pub)\s+", type="rust", output_mode="content", -n)

     # Find old error patterns that should use new approaches
     Grep(pattern="panic!|unwrap\(\)|expect\(", type="rust", output_mode="content", -B 3 -A 1)
     # Check for unwrap without justification comments
     Grep(pattern="(?<!//.*invariant|//.*cannot fail|//.*TODO.*unwrap).unwrap\(\)", type="rust", output_mode="content", -B 3 -A 1)

     # Find legacy patterns (e.g., old async patterns)
     Grep(pattern="\.block_on\(|tokio::run\(|std::thread::sleep", type="rust", output_mode="content", -n)
     ```

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
     - **Performance Anti-Pattern Detection**:
       ```
       # Find blocking I/O in async contexts
       Grep(pattern="async fn.*\{[\s\S]*?std::fs::|async fn.*\{[\s\S]*?std::thread::|async fn.*\{[\s\S]*?\.block_on\(", type="rust", output_mode="content", -B 2 -A 2)

       # Check for unnecessary allocations
       Grep(pattern="\.to_string\(\)|\.to_owned\(\)|\.clone\(\)", type="rust", output_mode="content", -B 1 -A 1)

       # Look for missing capacity hints
       Grep(pattern="Vec::new\(\)|String::new\(\)|HashMap::new\(\)", type="rust", output_mode="content", -A 3)

       # Find potential lock contention
       Grep(pattern="Mutex::new|RwLock::new|\.lock\(\)|\.write\(\)", type="rust", output_mode="content", -B 1 -A 1)

       # Check for inefficient string operations
       Grep(pattern="format!.*\+.*\+|\+.*&str|&str.*\+", type="rust", output_mode="content", -B 1 -A 1)

       # Look for unnecessary collections
       Grep(pattern="\.collect\(\)\.iter\(\)|\.collect\(\)\.len\(\)", type="rust", output_mode="content", -B 1 -A 1)

       # Find missing #[inline] hints on small functions
       Grep(pattern="^pub fn \w+\([^)]*\) -> [^{]+\{[\s\S]{1,200}\}", type="rust", output_mode="content", -A 3)
       ```

5.5. **Enhanced Unused Code Detection**:
   - Run systematic searches for unused code patterns:
     ```
     # Find unused imports (common patterns)
     Grep(pattern="^use .+;$", type="rust", output_mode="content", -n)
     Grep(pattern="^use crate::", type="rust", output_mode="content", -n)

     # Find private functions that might be unused
     Grep(pattern="fn \w+\([^)]*\)(?: -> [^{]+)? \{", type="rust", output_mode="content", -n)

     # Look for TODO/FIXME comments suggesting removal
     Grep(pattern="(?i)// TODO|FIXME.*remove|delete|deprecated", type="rust", output_mode="content", -n)
     ```
   - Check for unused constants and static variables
   - Identify unused trait implementations
   - Find unused struct fields (private fields with no usage)

6. Manual review based on these key areas:

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
   - **Architectural Consistency Checks**:
     ```
     # Check for proper module boundaries (no direct access to private internals)
     Grep(pattern="use crate::[^:]+::[^:]+::[^:]+", type="rust", output_mode="content", -n)

     # Verify trait implementations follow patterns
     Grep(pattern="impl \w+ for \w+", type="rust", output_mode="content", -A 5)

     # Check async function boundaries
     Grep(pattern="async fn \w+", type="rust", output_mode="content", -A 2)
     Grep(pattern="\.await", type="rust", output_mode="content", -B 1 -A 1)

     # Verify error handling consistency
     Grep(pattern="Result<[^,]+,\s*[\w:]+>", type="rust", output_mode="content", -n)
     Grep(pattern="\?(?!\s*$)", type="rust", output_mode="content", -B 1 -A 1)

     # Check for proper use of types (avoiding raw pointers where possible)
     Grep(pattern="\*mut |\*const ", type="rust", output_mode="content", -n)
     ```

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
   - **Best Practices Validation**:
     ```
     # Check import organization (std → external → internal)
     Grep(pattern="^(use std::|use crate::|use )", type="rust", output_mode="content", -n)

     # Verify proper Result/Option usage (unwrap requires justification comment)
     Grep(pattern="\.unwrap\(\)|\.expect\(", type="rust", output_mode="content", -B 3 -A 1)

     # Check for iterator patterns vs manual loops
     Grep(pattern="for \w+ in &.*\.iter\(\)|for \w+ in \w+\.\.|\w+\.len\(\) \{", type="rust", output_mode="content", -C 2)

     # Verify proper error context usage
     Grep(pattern="\.context\(|\.with_context\(|anyhow::bail!", type="rust", output_mode="content", -B 1 -A 1)

     # Check for proper async I/O usage
     Grep(pattern="std::fs::|std::io::", type="rust", output_mode="content", -n)
     Grep(pattern="tokio::fs::|tokio::io::", type="rust", output_mode="content", -n)

     # Verify proper file operation error handling
     Grep(pattern="\.with_file_context\(|FileOperationError|FileOps::", type="rust", output_mode="content", -n)

     # Check for file operations without proper context
     Grep(pattern="tokio::fs::|std::fs::.*\.await\?|\.with_context\(.*file|\.with_context\(.*path", type="rust", output_mode="content", -n)

     # Verify proper use of String vs &str
     Grep(pattern="String::new\(\)|String::from\(".*"\)", type="rust", output_mode="content", -B 1 -A 1)
     ```

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

6.5. **Result Aggregation** (for chunked reviews only):

   **IMPORTANT**: If you performed a chunked review (Large/Massive changeset), aggregate findings before generating the final report.

   **Deduplication Strategy**:

   a. **Collect all findings** from chunk reviews:
      - Group by finding type: errors, warnings, suggestions, security issues, etc.
      - Preserve chunk context for each finding

   b. **Deduplicate similar issues**:
      - **Identical issues**: Same error/warning in multiple chunks
        - Example: "Missing error context" found in chunks 1, 2, 4
        - Consolidate: "Missing error context in 3 chunks (core, installer, mcp)"

      - **Pattern-based duplicates**: Same issue type across different files
        - Example: Clippy warning "needless_return" in 8 different files
        - Consolidate: "needless_return pattern detected (8 occurrences across chunks 1-3)"

      - **Cross-chunk concerns**: Issues flagged by multiple agents
        - Example: Chunk 2 agent notes dependency on chunk 1 changes
        - Link findings: "Installer changes depend on resolver refactoring (see chunk 1 findings)"

   c. **Categorize by severity**:
      - **Critical**: Security issues, breaking changes, data loss risks
      - **High**: Architecture violations, significant bugs, missing tests
      - **Medium**: Code quality issues, incomplete docs, minor bugs
      - **Low**: Style issues, typos, suggestions

   d. **Identify cross-cutting concerns**:
      - Issues that span multiple chunks (architectural)
      - Patterns repeated across modules (refactoring opportunities)
      - Missing integration points between chunks

   e. **Calculate aggregate metrics**:
      - Total issues by severity
      - Coverage by module (which chunks were clean vs. problematic)
      - Most common issue types
      - Example: "15 total issues: 2 critical (security), 5 high (architecture), 8 medium (docs)"

   **Aggregation Example**:

   ```
   Chunk 1 findings:
   - [High] Missing error context in resolver/mod.rs:245
   - [Medium] Verbose docstring in resolver/version_resolver.rs:120
   - [Low] Clippy needless_return in resolver/mod.rs:300

   Chunk 2 findings:
   - [High] Missing error context in installer/mod.rs:180
   - [Critical] Path traversal risk in installer/resource_installer.rs:95
   - [Low] Clippy needless_return in installer/mod.rs:200

   Aggregated findings:
   - [Critical] Path traversal risk in installer/resource_installer.rs:95
   - [High] Missing error context pattern (2 occurrences: resolver, installer)
   - [Medium] Verbose docstring in resolver/version_resolver.rs:120
   - [Low] Clippy needless_return pattern (2 occurrences: resolver, installer)

   Summary: 1 critical, 1 high, 1 medium, 1 low (grouped from 5 original findings)
   ```

   **Mark aggregation todo as completed** when finished.

7. Generate a summary report with:
   - **Changes Overview**: What was modified
   - **Test Results**:
     - For historical reviews: "Automated checks skipped (historical review)"
     - For current changes: Pass/fail status of automated checks
   - **Issues Found**: Any problems discovered (grouped by severity)
     - **For chunked reviews**: Use aggregated findings from step 6.5
     - Group by severity (Critical, High, Medium, Low)
     - Include cross-chunk concerns and patterns
   - **Security Analysis**: Security implications if any
   - **Performance Impact**: Performance considerations
   - **Recommendations**: Approve, request changes, or needs discussion
   - **Review Strategy Used**:
     - Note if chunked review was used (e.g., "Chunked review: 4 batches across 42 files")
     - Note if historical review with automated checks skipped

8. Focus only on tracked files - ignore untracked files marked with ?? in git status

**Historical Review Limitations**:
- When reviewing past commits or commit ranges, automated checks (cargo fmt, clippy, cargo test) are skipped
- This prevents misleading results since automated checks would run against current code, not the historical state
- To run tests on historical code, checkout the commit manually:
  ```bash
  git checkout <commit-hash>
  cargo test
  ```

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

- `/pr-review abc123` - full review of specific commit abc123 (automated checks skipped)
- `/pr-review HEAD~1 --quick` - quick review of the previous commit (automated checks skipped)
- `/pr-review 5b3ee1d --security` - security review of commit 5b3ee1d (automated checks skipped)

**Commit range review**:

- `/pr-review main..HEAD` - full review of all changes from main to HEAD (automated checks skipped)
- `/pr-review abc123..def456 --quick` - quick review of commits between abc123 and def456 (automated checks skipped)
- `/pr-review origin/main..HEAD --security` - security review of all changes not yet in origin/main (automated checks skipped)
- `/pr-review HEAD~3..HEAD` - review the last 3 commits as a range (automated checks skipped)

**Note**:
- This command only reviews and reports on changes. To create an actual pull request after review, use the `gh-pr-create` command.
- For historical reviews (single commits or commit ranges), automated checks are skipped to prevent misleading results. The code analysis is performed on the historical changes, but tests run against the current codebase would not be meaningful.

## Best Practices

{{ agpm.deps.snippets.best_practices.content }}
