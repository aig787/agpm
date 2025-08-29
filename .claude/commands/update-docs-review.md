---
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Read, Edit, MultiEdit, Grep, Task
description: Review code changes and ensure all related documentation is accurate and up-to-date
argument-hint: [ --check-only | --auto-update | --focus=<module> ] - e.g., "--focus=cli" to review specific module docs
---

## Context

- Current changes: !`git diff HEAD`
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -5`

## Your task

Review the current code changes and ensure all related documentation (doc comments, module docs, CLAUDE.md) accurately reflects the implementation.

1. Parse the review mode from arguments:
   - `--check-only`: Only report documentation issues without making changes
   - `--auto-update`: Update documentation to match code changes (default)
   - `--focus=<module>`: Focus on specific module (e.g., cli, resolver, source)
   - Arguments: $ARGUMENTS

2. Analyze the git diff to identify code changes:
   - New functions, structs, or modules added
   - Modified function signatures or behavior
   - Removed or deprecated functionality
   - Changed error types or handling
   - Modified data structures or APIs
   - New or changed configuration options
   - Algorithm or logic changes

3. Review documentation accuracy for changed code:

   **Documentation types to check**:
   - **Doc comments (///)**: Function, struct, and module documentation
   - **Inline comments (//)**: Implementation detail explanations
   - **Module documentation**: Top-level module descriptions
   - **CLAUDE.md**: Architecture decisions and module descriptions
   - **Error messages**: User-facing error documentation
   - **Examples in docs**: Code examples in documentation

4. Identify documentation issues based on changes:

   **Common documentation problems**:
   - Doc comments describing old behavior
   - Missing documentation for new public APIs
   - Outdated parameter descriptions
   - Incorrect return value documentation
   - Stale examples that won't compile
   - Module docs not reflecting new responsibilities
   - CLAUDE.md architecture section outdated
   - Error variants without documentation

5. Apply updates based on mode:

   **Check-only mode (--check-only)**:
   - List all documentation discrepancies found
   - Show specific lines needing updates
   - Highlight missing documentation
   - Report outdated or incorrect information
   - Suggest documentation improvements

   **Auto-update mode (--auto-update or default)**:
   - Update doc comments to match implementation
   - Add missing documentation for public items
   - Fix parameter and return value descriptions
   - Update examples to compile with current code
   - Correct module-level documentation
   - Update CLAUDE.md if architecture changed
   - Ensure error messages are documented

6. Focus areas for CCPM project:

   **Critical documentation to maintain**:
   - **Public API documentation**: All pub functions and structs
   - **Error handling**: ErrorContext and user-facing errors
   - **Cross-platform behavior**: Platform-specific code paths
   - **Security considerations**: Input validation and safety checks
   - **Resource types**: Documentation for each resource type
   - **CLI commands**: Argument and behavior documentation
   - **Test requirements**: Test isolation and environment setup

7. Documentation standards to enforce:

   **Rust documentation conventions**:
   - Start doc comments with brief description
   - Use `# Examples` section for code examples
   - Document all public items
   - Use `# Panics` section if function can panic
   - Use `# Errors` section for Result-returning functions
   - Use `# Safety` section for unsafe code
   - Include parameter descriptions with backticks
   - Ensure examples compile (use ```no_run if needed)

8. Special checks for changed modules:

   **Module-specific documentation**:
   - `cli/`: Command documentation and help text
   - `resolver/`: Algorithm and constraint documentation
   - `source/`: Cache behavior and Git operations
   - `lockfile/`: Format and compatibility documentation
   - `manifest/`: Schema and validation documentation
   - `git/`: Command building and execution documentation
   - `test_utils/`: Test helper usage documentation

9. CLAUDE.md synchronization:

   **Sections to keep updated**:
   - Module organization and responsibilities
   - Key architecture decisions
   - Testing strategy and requirements
   - Security rules and validations
   - Implementation lessons learned
   - Critical testing requirements

10. Quality checks:

    **Documentation quality criteria**:
    - Accuracy: Matches current implementation
    - Completeness: All public APIs documented
    - Clarity: Easy to understand
    - Consistency: Uniform style and terminology
    - Examples: Working code examples where helpful
    - Cross-references: Links between related items

Examples of changes requiring doc updates:
- New public function → Add /// doc comment
- Changed function behavior → Update doc comment
- New error variant → Document in Error enum
- Modified algorithm → Update implementation comments
- New module → Add module-level documentation
- Architecture change → Update CLAUDE.md
- New test requirements → Document in test module

Examples of usage:
- `/update-docs-review` - automatically update docs based on code changes
- `/update-docs-review --check-only` - report documentation issues without changes
- `/update-docs-review --focus=cli` - focus on CLI module documentation
- `/update-docs-review --focus=resolver` - focus on resolver module documentation