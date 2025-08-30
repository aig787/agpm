---
allowed-tools: Task
description: Run code quality checks (formatting, linting, documentation)
argument-hint: [ --fix | --check ] [ --all ] [ --doc ] - e.g., "--fix" or "--check --all"
---

## Context

- Project name: CCPM (Claude Code Package Manager)

## Your task

Run code quality checks for the CCPM project using specialized Rust agents:

1. Parse the arguments:
   - `--fix`: Apply automatic fixes where possible (uses rust-linting-expert)
   - `--check`: Run in CI mode (strict checking, fail on warnings)
   - `--all`: Run all checks including test compilation
   - `--doc`: Add comprehensive documentation (uses rust-doc-expert)
   - If no arguments: Run standard checks (fmt, clippy, doc)
   - Arguments: $ARGUMENTS

2. Based on the arguments, delegate to the appropriate specialized agent:

   **For linting and formatting (`--fix` or standard checks):**
   - Use the `rust-linting-expert` agent to:
     - Run `cargo fmt --all` (or `cargo fmt --all --check` if `--check` mode)
     - Run `cargo clippy --all-features --tests` with appropriate flags
     - Apply automatic fixes if `--fix` is specified with `--all-features`
     - The agent will delegate complex refactoring to rust-expert if needed
     - **Important**: Always use `--all-features` to check feature-gated code
     - **Important**: Always include `--tests` to check test code

   **For documentation (`--doc` flag):**
   - Use the `rust-doc-expert` agent to:
     - Add comprehensive documentation to undocumented code
     - Improve existing documentation with examples
     - Ensure all public APIs have proper rustdoc comments
     - Add module-level documentation where missing
     - Run `cargo doc --all-features --no-deps` to verify documentation

   **For standard checks without --fix:**
   - Use `rust-linting-expert` in check-only mode to analyze issues without fixing
   - Always include `--all-features` and `--tests` flags

3. Agent delegation strategy:
   - The rust-linting-expert handles fast linting fixes
   - Complex refactoring is delegated to rust-expert by the linting agent
   - Memory issues or undefined behavior are delegated to rust-troubleshooter-opus
   - Documentation improvements use rust-doc-expert

4. Provide a clear summary of actions taken and any remaining issues

Examples of usage:

- `/lint` - Run standard checks with --all-features (fmt, clippy, doc)
- `/lint --fix` - Apply automatic fixes for formatting and clippy issues (includes test code)
- `/lint --check` - CI mode with strict validation (all features and tests)
- `/lint --check --all` - Full CI validation including test compilation with all features
- `/lint --all` - Run all checks including test compilation with all features
- `/lint --doc` - Add/improve documentation using rust-doc-expert

Note: All commands automatically include `--all-features` and check test code to ensure complete coverage.