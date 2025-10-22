---
description: Fact-check all documentation files against the current codebase implementation
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/fact-check-docs.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--report-only`, `--fix`

## Documentation Fact-Checking Implementation

Systematically verify the accuracy of all documentation files against the current codebase implementation.

**CRITICAL**: This command focuses on fact-checking existing documentation rather than updating based on changes.

**IMPORTANT**: Perform LINE-BY-LINE verification, not high-level validation. Every claim in documentation must be precisely verified against the actual implementation. Do not assume documentation is correct - verify each specific detail:

- Version numbers must match exactly
- Command syntax must be identical
- File paths must exist and be correct
- Dependency names must match Cargo.toml exactly
- Configuration options must match the actual implementation

### Argument Semantics

- **Flags**:
  - `--report-only`: Only report inaccuracies without making changes (default behavior)
  - `--fix`: Automatically fix any inaccuracies found in documentation

2. Identify all documentation files to check:

   **Primary documentation files**:
   - **README.md**: Main project overview and quick start
   - **CLAUDE.md**: AI context and project instructions
   - **CONTRIBUTING.md**: Development guidelines

   **docs/ directory files**:
   - **docs/installation.md**: Installation methods and requirements
   - **docs/user-guide.md**: Getting started and workflows
   - **docs/versioning.md**: Version constraints and Git references
   - **docs/resources.md**: Resource types and configuration
   - **docs/configuration.md**: Global config and authentication
   - **docs/architecture.md**: Technical details and design
   - **docs/troubleshooting.md**: Common issues and solutions
   - **docs/faq.md**: Frequently asked questions
   - **docs/command-reference.md**: Command reference

   **Claude Code saved prompts**:
   - **.claude/commands/*.md**: Custom commands
   - **.claude/agents/*.md**: Agent definitions
   - **.agpm/snippets/*.md**: Code snippets (default location)
   - **.claude/scripts/*.md**: Script definitions

3. For each documentation file, systematically verify:

   **CRITICAL VERIFICATION APPROACH**: Read each line of documentation and verify the claim against the actual codebase implementation. Use grep, read, and glob tools to cross-reference every specific claim.

   **Command-related claims**:
   - CLI command syntax is EXACTLY correct (character-by-character)
   - Command options and flags exist and work as described
   - Subcommand names match precisely (e.g., `config [show|edit]` not `config [get|set]`)
   - Default values match implementation exactly
   - Examples are executable and produce expected results
   - Error messages match actual output
   - Command help text matches what users actually see

   **Code structure claims**:
   - Module descriptions match directory structure exactly
   - File paths referenced in docs exist and are correct
   - Function/struct names are accurate (case-sensitive)
   - Architecture descriptions match actual implementation
   - Design patterns described are actually used

   **Configuration claims**:
   - Config file formats are accurate (agpm.toml, agpm.lock)
   - Field names and types match schema
   - Default configuration values are correct
   - Environment variable names are accurate
   - Path conventions match implementation

   **Feature claims**:
   - Listed features actually exist
   - Feature descriptions match behavior
   - Performance claims are reasonable
   - Platform support claims are accurate
   - Resource types are correctly described

   **Technical details**:
   - Dependency lists are current (check Cargo.toml EXACTLY - no missing or extra dependencies)
   - Version numbers are accurate (Rust version, tool versions, etc.)
   - File format descriptions match parsers
   - API descriptions match implementation
   - Error handling descriptions are accurate
   - **CRITICAL**: Verify every dependency name mentioned in documentation appears in Cargo.toml
   - **CRITICAL**: Verify version numbers in docs match actual versions in code

   **Examples and code snippets**:
   - TOML examples are valid syntax
   - Shell commands work as shown
   - Code snippets compile/run correctly
   - Paths in examples are realistic
   - Output examples match actual output

4. Verification approach for each type of claim:

   **PRECISION VERIFICATION METHODOLOGY**:

   **For CLI commands**:
   - Use `grep` to find exact command definitions in src/cli/mod.rs and submodules
   - Verify subcommand names match EXACTLY (character-by-character comparison)
   - Check clap command structures for precise option names and types
   - Look for deprecated or removed commands
   - **Example**: If docs say `config [get|set]`, verify the actual enum has `Get` and `Set` variants

   **For configuration**:
   - Check src/manifest/mod.rs for manifest schema
   - Check src/lockfile/mod.rs for lockfile format
   - Check src/config/mod.rs for global config structure
   - Verify field names and types match exactly

   **For architecture**:
   - Use `glob` to verify actual directory structure matches docs
   - Verify module relationships in mod.rs files
   - Check that described patterns exist in code
   - Verify file paths exist and are accessible

   **For features**:
   - Use `grep` to search codebase for feature implementations
   - Verify resource types in src/core/resource.rs
   - Check for feature flags or conditional compilation

   **For dependencies**:
   - Compare against Cargo.toml dependencies EXACTLY
   - Check for removed or added dependencies
   - Verify version constraints mentioned
   - **CRITICAL**: Every dependency mentioned in docs must appear in Cargo.toml
   - **CRITICAL**: No extra dependencies should be mentioned that don't exist

5. Based on verification mode:

   **Report-only mode (--report-only or default)**:
   - List each inaccuracy found with:
     * File and line number
     * What the documentation claims
     * What the code actually does
     * Suggested correction
   - Group findings by severity:
     * Critical: Wrong commands, missing features
     * Important: Incorrect examples, wrong config
     * Minor: Outdated descriptions, typos

   **Fix mode (--fix)**:
   - Apply minimal edits to correct inaccuracies
   - Preserve documentation style and structure
   - Update only factually incorrect information
   - Keep helpful context and explanations
   - Don't remove useful examples or notes

6. Special areas requiring careful verification:

   **HIGH-PRIORITY VERIFICATION AREAS** (These commonly contain inaccuracies):

   **Version Information**:
   - Rust version in CONTRIBUTING.md vs Cargo.toml rust-version
   - Tool versions mentioned in docs vs actual requirements
   - Package version numbers vs current release

   **Command Syntax**:
   - Subcommand names in brackets (e.g., `[show|edit]` vs `[get|set]`)
   - Flag names and syntax (e.g., `--max-parallel` vs `--parallel`)
   - Default behaviors and option availability

   **Dependency Lists**:
   - Dependencies mentioned in documentation vs Cargo.toml
   - Version constraints mentioned vs actual constraints
   - Missing or extra dependency names

   **AGPM-specific**:
   - Git worktree architecture descriptions
   - SHA-based resolution claims
   - Parallel installation capabilities
   - Cache structure and paths
   - Lockfile generation and usage
   - Resource installation locations
   - Version constraint syntax
   - Pattern matching behavior (glob patterns)

   **Cross-platform claims**:
   - Windows path handling
   - Platform-specific installation steps
   - Shell command compatibility
   - File permission handling

7. Quality checks:

   - Ensure technical terms are used correctly
   - Verify that acronyms are properly explained
   - Check that cross-references between docs are valid
   - Confirm external links are relevant (don't verify they work)
   - Ensure version numbers match current release

8. Verification workflow and reporting format:

   **SYSTEMATIC VERIFICATION WORKFLOW**:
   1. Read documentation file line by line
   2. For each claim, use tools to verify against actual code
   3. Document every discrepancy found
   4. Categorize by severity based on user impact

   **When reporting inaccuracies, use this format**:
   ```
   ## File: docs/example.md

   ### Line 42: Incorrect command syntax
   - Documentation says: `agpm install --parallel`
   - Actually should be: `agpm install --max-parallel N`
   - Verification: Checked src/cli/install.rs - found `max_parallel` field, not `parallel`
   - Severity: Important

   ### Line 78: Outdated module structure
   - Documentation says: `src/installer/mod.rs`
   - Actually located at: `src/installer.rs`
   - Verification: Used `glob` to check actual file structure
   - Severity: Minor

   ### Line 15: Wrong dependency listed
   - Documentation says: `once_cell` in dependencies list
   - Actually should be: `dashmap` (once_cell not in Cargo.toml)
   - Verification: Checked Cargo.toml dependencies section
   - Severity: Critical
   ```

   **SEVERITY CLASSIFICATION**:
   - **Critical**: Causes build failures, command execution failures, or major user confusion
   - **Important**: Causes incorrect usage, wrong examples, or significant confusion
   - **Minor**: Typos, outdated descriptions, or cosmetic issues

Examples of usage:
- `/fact-check-docs` - report all documentation inaccuracies
- `/fact-check-docs --report-only` - explicitly report without fixes (same as default)
- `/fact-check-docs --fix` - automatically fix found inaccuracies

**QUALITY ASSURANCE CHECKLIST** (Before completing the fact-check):
- [ ] Verified all version numbers against actual code
- [ ] Checked all command syntax against CLI definitions
- [ ] Cross-referenced all dependencies with Cargo.toml
- [ ] Verified all file paths exist and are correct
- [ ] Confirmed all configuration options match implementation
- [ ] Tested examples for executability (where feasible)
- [ ] Checked for deprecated or removed features
- [ ] Verified all cross-references between documents

**REMEMBER**: The goal is PRECISION. Every claim in documentation must be factually correct and match the actual implementation exactly. Users rely on this documentation for their work, so inaccuracies cause real problems.

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- **VERIFICATION TOOLS**: Use Read, Grep, and Glob tools extensively to cross-reference documentation claims with actual code
- **PRECISION APPROACH**: For each claim in documentation, find the corresponding code and verify exact matches
- **SYSTEMATIC PROCESS**: Go through each documentation file systematically, line by line
- Generate a detailed report of any inconsistencies found with specific evidence

**VERIFICATION STRATEGY**:
1. Read a section of documentation
2. Identify specific claims (versions, commands, paths, dependencies)
3. Use tools to locate the corresponding implementation
4. Compare claim vs reality exactly
5. Document every discrepancy found
6. Repeat for all documentation files
