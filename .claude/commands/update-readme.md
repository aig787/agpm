---
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Read, Edit, MultiEdit, Grep
description: Review changes and update README.md to stay current with implementation
argument-hint: [ --check-only | --auto-update ] - e.g., "--check-only" to only report needed updates
---

## Context

- Current changes: !`git diff HEAD`
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -5`

## Your task

Review the current changes and ensure README.md accurately reflects the project's current state.

1. Parse the update mode from arguments:
   - `--check-only`: Only report what needs updating without making changes
   - `--auto-update`: Make necessary updates to README.md (default)
   - Arguments: $ARGUMENTS

2. Analyze the git diff to understand what has changed:
   - New features or commands added
   - Changed CLI options or arguments
   - Modified behavior or functionality
   - Removed features or deprecated options
   - New resource types or configuration options
   - Changes to installation or usage instructions
   - Performance improvements or architecture changes

3. Read the current README.md and identify sections that may need updates:

   **Critical sections to check**:
   - **Features list**: New capabilities or removed features
   - **Resource Types**: New resource types (agents, snippets, scripts, hooks, MCP servers)
   - **Installation**: Changes to installation process or requirements
   - **Quick Start**: Changes to manifest format or basic usage
   - **Commands**: New commands, changed syntax, or new options
   - **Configuration**: New configuration options or format changes
   - **Error Messages**: Updated error handling or new error types
   - **Platform Support**: Changes to cross-platform behavior
   - **Dependencies**: New or removed dependencies

4. Based on the changes, determine what README updates are needed:

   **Types of updates to make**:
   - Add documentation for new features or commands
   - Update command syntax and options
   - Correct outdated information
   - Add new examples for new functionality
   - Update manifest format examples if schema changed
   - Add or update resource type descriptions
   - Update performance claims if improvements were made
   - Fix any inaccuracies introduced by recent changes

5. Apply updates based on mode:

   **Check-only mode (--check-only)**:
   - Report all discrepancies found
   - List specific sections needing updates
   - Show what information is missing or incorrect
   - Provide suggested changes without applying them

   **Auto-update mode (--auto-update or default)**:
   - Make minimal, targeted edits to fix discrepancies
   - Preserve existing README structure and style
   - Add new sections only if necessary for new features
   - Update examples to match current implementation
   - Ensure all code snippets are valid

6. Focus on accuracy and completeness:
   - Verify all command examples work with current implementation
   - Ensure manifest examples are valid TOML
   - Check that installation instructions are current
   - Validate that feature descriptions match actual behavior
   - Confirm resource type descriptions are complete

7. Maintain README quality:
   - Keep language clear and concise
   - Preserve existing formatting conventions
   - Ensure examples are practical and helpful
   - Maintain consistent terminology throughout
   - Don't remove useful existing content

8. Special considerations for CCPM:
   - Lockfile behavior (ccpm.lock) must be accurately described
   - Git-based distribution model should be clear
   - Cross-platform support claims must be accurate
   - Security considerations should be mentioned where relevant
   - Resource installation paths should match implementation

Examples of changes that require README updates:
- Adding a new CLI command → Document in Commands section
- Changing manifest format → Update examples in Quick Start
- Adding new resource type → Add to Resource Types section
- Modifying installation paths → Update in relevant sections
- Improving performance → Update performance claims if made
- Adding new dependencies → Update installation requirements
- Changing error messages → Update troubleshooting if present

Examples of usage:
- `/update-readme` - automatically update README based on changes
- `/update-readme --check-only` - report what needs updating without changes
- `/update-readme --auto-update` - explicitly update README (same as default)