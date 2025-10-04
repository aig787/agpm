---
allowed-tools: Task, Bash(cargo fmt:*), Bash(cargo clippy:*), Bash(cargo doc:*), Bash(cargo check:*), Bash(cargo build:*), Bash(cargo test:*), Bash(cargo fix:*), Bash(rustfmt:*), BashOutput, Read, Write, Edit, MultiEdit, Glob, Grep, TodoWrite, WebSearch, WebFetch, ExitPlanMode
description: Run code quality checks (formatting, linting, documentation)
argument-hint: [ --fix | --check ] [ --all ] [ --doc ] - e.g., "--fix" or "--check --all"
---

## Context

- Project name: AGPM (Claude Code Package Manager)

## Your task

Run code quality checks for the AGPM project using specialized Rust agents with automatic escalation:

1. Parse the arguments:
   - `--fix`: Apply automatic fixes where possible (uses escalating agents)
   - `--check`: Run in CI mode (strict checking, fail on warnings)
   - `--all`: Run all checks including test compilation
   - `--doc`: Add comprehensive documentation (uses rust-doc-standard)
   - If no arguments: Run standard checks (fmt, clippy, doc)
   - Arguments: $ARGUMENTS

2. **CRITICAL**: Use the Task tool to delegate to specialized agents. Do NOT run cargo commands directly.

3. **AUTOMATIC ESCALATION STRATEGY** - Based on the arguments, use this multi-phase approach:

   **Phase 1: Fast Fixes (`--fix` mode)**

   a) First, use `rust-linting-standard` (Haiku) for quick automatic fixes:
   ```
   Task(description="Apply automatic linting fixes",
        prompt="Run cargo fmt and clippy with --fix flag. Apply all automatic fixes for all features and test code. Report any remaining warnings that require manual intervention.",
        subagent_type="rust-linting-standard")
   ```

   b) **Analyze the result**: If the agent reports remaining warnings, issues, or manual intervention needed, proceed to Phase 2.

   **Phase 2: Advanced Fixes (Automatic Escalation)**

   If Phase 1 reports remaining issues, automatically escalate to `rust-linting-advanced` (Sonnet):
   ```
   Task(description="Fix remaining linting issues",
        prompt="Fix the remaining clippy warnings and code quality issues. The following issues were identified:
        [summarize issues from Phase 1]

        Address all fixable warnings including:
        - Redundant else blocks
        - Documentation improvements (add # Errors sections)
        - Code simplification opportunities
        - Naming conventions
        - Function complexity

        Run cargo clippy --all-features --all-targets -- -D warnings to verify all warnings are fixed.",
        subagent_type="rust-linting-advanced")
   ```

   c) The advanced agent will:
     - Fix redundant code patterns
     - Add missing documentation sections
     - Simplify complex logic
     - Delegate architectural changes to rust-expert if needed
     - Verify with strict warning mode

   **For linting basics (what agents do):**
   - Run `cargo fmt --all` (or `cargo fmt --all --check` if `--check` mode)
   - Run `cargo clippy --all-features --all-targets` with appropriate flags
   - Apply automatic fixes if `--fix` is specified
   - **Important**: Always use `--all-features` to check feature-gated code
   - **Important**: Always use `--all-targets` to include tests, benches, examples

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
   - Use `rust-linting-standard` for quick analysis
   - No escalation needed - just report findings

4. Agent delegation strategy:
   - `rust-linting-standard` (Haiku) → Fast, mechanical fixes → **Escalates if issues remain**
   - `rust-linting-advanced` (Sonnet) → Complex linting and refactoring → **Escalates to rust-expert if needed**
   - `rust-expert-standard` or `rust-expert-advanced` → Architectural changes
   - `rust-troubleshooter-advanced` → Memory issues or undefined behavior
   - `rust-doc-standard` or `rust-doc-advanced` → Documentation improvements

5. Provide a clear summary of:
   - What Phase 1 fixed (if applicable)
   - Whether escalation occurred and why
   - What Phase 2 fixed (if applicable)
   - Any remaining issues (should be none after Phase 2)

Examples of usage:

- `/lint` - Run standard checks with --all-features (fmt, clippy, doc)
- `/lint --fix` - **Two-phase fix with auto-escalation**:
  - Phase 1: rust-linting-standard applies quick fixes
  - Phase 2: Auto-escalates to rust-linting-advanced if issues remain
  - Result: All fixable warnings resolved
- `/lint --check` - CI mode with strict validation (all features and tests)
- `/lint --check --all` - Full CI validation including test compilation with all features
- `/lint --all` - Run all checks including test compilation with all features
- `/lint --doc` - Add/improve documentation using rust-doc-standard or rust-doc-advanced

Note: All commands automatically include `--all-features` and `--all-targets` to ensure complete coverage.

**Escalation Flow for --fix:**
```
rust-linting-standard (Phase 1)
    ↓ (if warnings remain)
rust-linting-advanced (Phase 2)
    ↓ (if architectural changes needed)
rust-expert-standard/advanced (Phase 3)
```