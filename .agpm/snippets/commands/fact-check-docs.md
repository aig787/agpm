## Your task

Systematically verify the accuracy of all documentation files against the current codebase implementation.

**CRITICAL**: This command focuses on fact-checking existing documentation rather than updating based on changes.

1. Parse the mode from arguments:
   - `--report-only`: Only report inaccuracies without making changes (default)
   - `--fix`: Fix any inaccuracies found
   - Arguments: $ARGUMENTS

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
   - **.claude/snippets/*.md**: Code snippets
   - **.claude/scripts/*.md**: Script definitions

3. For each documentation file, systematically verify:

   **Command-related claims**:
   - CLI command syntax is correct
   - Command options and flags exist and work as described
   - Default values match implementation
   - Examples are executable and produce expected results
   - Error messages match actual output

   **Code structure claims**:
   - Module descriptions match directory structure
   - File paths referenced in docs exist
   - Function/struct names are accurate
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
   - Dependency lists are current (check Cargo.toml)
   - Version numbers are accurate
   - File format descriptions match parsers
   - API descriptions match implementation
   - Error handling descriptions are accurate

   **Examples and code snippets**:
   - TOML examples are valid syntax
   - Shell commands work as shown
   - Code snippets compile/run correctly
   - Paths in examples are realistic
   - Output examples match actual output

4. Verification approach for each type of claim:

   **For CLI commands**:
   - Check src/cli/mod.rs and submodules for actual command definitions
   - Verify option names and types in clap command structures
   - Look for deprecated or removed commands

   **For configuration**:
   - Check src/manifest/mod.rs for manifest schema
   - Check src/lockfile/mod.rs for lockfile format
   - Check src/config/mod.rs for global config structure

   **For architecture**:
   - Compare docs against actual directory structure
   - Verify module relationships in mod.rs files
   - Check that described patterns exist in code

   **For features**:
   - Search codebase for feature implementations
   - Verify resource types in src/core/resource.rs
   - Check for feature flags or conditional compilation

   **For dependencies**:
   - Compare against Cargo.toml dependencies
   - Check for removed or added dependencies
   - Verify version constraints mentioned

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

8. Reporting format:

   When reporting inaccuracies, use this format:
   ```
   ## File: docs/example.md

   ### Line 42: Incorrect command syntax
   - Documentation says: `agpm install --parallel`
   - Actually should be: `agpm install --max-parallel N`
   - Severity: Important

   ### Line 78: Outdated module structure
   - Documentation says: `src/installer/mod.rs`
   - Actually located at: `src/installer.rs`
   - Severity: Minor
   ```

Examples of usage:
- `/fact-check-docs` - report all documentation inaccuracies
- `/fact-check-docs --report-only` - explicitly report without fixes (same as default)
- `/fact-check-docs --fix` - automatically fix found inaccuracies