---
allowed-tools: Task, Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo doc:*), Bash(cargo check:*), Bash(cargo build:*), Bash(cargo test:*), Bash(cargo fix:*), Bash(rustfmt:*), BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch, ExitPlanMode
description: Run code quality checks (formatting, linting, documentation)
argument-hint: [ --fix | --check ] [ --all ] [ --doc ] - e.g., "--fix" or "--check --all"
---

## Context

- Project name: CCPM (Claude Code Package Manager)

## Your task

Run code quality checks for the CCPM project using specialized Rust agents:

1. Parse the arguments:
   - `--fix`: Apply automatic fixes where possible (uses rust-linting-standard)
   - `--check`: Run in CI mode (strict checking, fail on warnings)
   - `--all`: Run all checks including test compilation
   - `--doc`: Add comprehensive documentation (uses rust-doc-standard)
   - If no arguments: Run standard checks (fmt, clippy, doc)
   - Arguments: $ARGUMENTS

2. **CRITICAL**: Use the Task tool to delegate to specialized agents. Do NOT run cargo commands directly.

3. Based on the arguments, delegate to the appropriate specialized agent using Task:

   **For linting and formatting (`--fix` or standard checks):**
   - Use Task with subagent_type="rust-linting-standard" for quick fixes:
     ```
     Task(description="Fix linting issues",
          prompt="Run cargo fmt and clippy with --fix flag. Apply automatic fixes for all features and test code...",
          subagent_type="rust-linting-standard")
     ```
   - Or use Task with subagent_type="rust-linting-advanced" for complex issues
   - The agents will:
     - Run `cargo fmt --all` (or `cargo fmt --all --check` if `--check` mode)
     - Run `cargo clippy --all-features --tests` with appropriate flags
     - Apply automatic fixes if `--fix` is specified with `--all-features`
     - The agent will delegate complex refactoring to rust-expert if needed
     - **Important**: Always use `--all-features` to check feature-gated code
     - **Important**: Always include `--tests` to check test code

   **For documentation (`--doc` flag):**
   - Use Task with subagent_type="rust-doc-standard" for documentation:
     ```
     Task(description="Add documentation",
          prompt="Add comprehensive rustdoc comments to all public APIs. Include examples and ensure completeness...",
          subagent_type="rust-doc-standard")
     ```
   - Or use Task with subagent_type="rust-doc-advanced" for architectural docs
     - Add comprehensive documentation to undocumented code
     - Improve existing documentation with examples
     - Ensure all public APIs have proper rustdoc comments
     - Add module-level documentation where missing
     - Run `cargo doc --all-features --no-deps` to verify documentation

   **For standard checks without --fix:**
   - Use `rust-linting-standard` for quick analysis or `rust-linting-advanced` for detailed review
   - Always include `--all-features` and `--tests` flags

4. Agent delegation strategy:
   - `rust-linting-standard` (Haiku) handles fast, mechanical fixes
   - `rust-linting-advanced` (Sonnet) handles complex linting and refactoring suggestions
   - Complex refactoring is delegated to `rust-expert-standard` or `rust-expert-advanced`
   - Memory issues or undefined behavior are delegated to `rust-troubleshooter-advanced`
   - Documentation improvements use `rust-doc-standard` or `rust-doc-advanced`

5. Provide a clear summary of actions taken and any remaining issues

Examples of usage:

- `/lint` - Run standard checks with --all-features (fmt, clippy, doc)
- `/lint --fix` - Apply automatic fixes for formatting and clippy issues (includes test code)
- `/lint --check` - CI mode with strict validation (all features and tests)
- `/lint --check --all` - Full CI validation including test compilation with all features
- `/lint --all` - Run all checks including test compilation with all features
- `/lint --doc` - Add/improve documentation using rust-doc-standard or rust-doc-advanced

Note: All commands automatically include `--all-features` and check test code to ensure complete coverage.