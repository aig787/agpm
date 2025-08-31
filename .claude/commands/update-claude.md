---
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(cargo tree:*), Read, Edit, MultiEdit, Grep, LS, Task
description: Review changes and update CLAUDE.md to reflect current architecture and implementation
argument-hint: [ --check-only | --auto-update ] - e.g., "--check-only" to only report needed updates
---

## Context

- Current changes: !`git diff HEAD`
- Files changed: !`git status --short`
- Recent commits: !`git log --oneline -10`

## Your task

Review the current changes and ensure CLAUDE.md accurately reflects the project's architecture, implementation details, and development guidelines. **IMPORTANT**: CLAUDE.md must remain under 20,000 characters total.

1. Parse the update mode from arguments:
   - `--check-only`: Only report what needs updating without making changes
   - `--auto-update`: Make necessary updates to CLAUDE.md (default)
   - Arguments: $ARGUMENTS

2. Analyze the git diff to understand architectural and implementation changes:
   - New modules or restructured directories
   - Changed dependencies in Cargo.toml
   - New or modified core commands
   - Architecture decisions or design patterns
   - Testing strategy changes
   - Security rule updates
   - Build or development workflow changes
   - New resource types or formats
   
   **For complex architectural documentation, delegate to specialized agents using Task:**
   - Use Task with subagent_type="rust-doc-standard" or "rust-doc-advanced":
     ```
     Task(description="Update architectural docs",
          prompt="Review architectural changes and update CLAUDE.md documentation accordingly...",
          subagent_type="rust-doc-advanced")
     ```
   - The documentation agent will handle:
     * Generating comprehensive architectural documentation
     * Writing detailed module descriptions
     * Creating usage examples for new features
     * Documenting design patterns and decisions
   - Use Task with subagent_type="rust-expert-standard" or "rust-expert-advanced":
     ```
     Task(description="Review architecture",
          prompt="Review architectural changes in CLAUDE.md for best practices and design patterns...",
          subagent_type="rust-expert-standard")
     ```
   - The expert agent will handle:
     * Reviewing architectural changes for best practices
     * Validating design decisions
     * Suggesting improvements to module structure

3. Read the current CLAUDE.md and identify sections that may need updates:

   **Critical sections to check**:
   - **Project Structure**: New modules, renamed directories, reorganization
   - **Core Commands**: New commands, changed options, removed functionality
   - **Available Agents**: Claude Code agents in `.claude/agents/` directory
   - **Key Dependencies**: Added/removed crates in Cargo.toml
   - **Module Structure**: Module responsibilities and interactions
   - **Implementation Details**: Changed algorithms, patterns, or approaches
   - **Testing Strategy**: New test types, coverage changes, testing patterns
   - **Security Rules**: New security considerations or validations
   - **Development Guidelines**: Updated practices or requirements
   - **Example Formats**: ccpm.toml and ccpm.lock format changes
   - **Build and Release**: New build steps or requirements

4. Perform systematic checks:

   **Module Structure Verification**:
   - List actual modules in `src/` directory
   - Compare with documented module structure
   - Check if module descriptions match their actual responsibilities
   - Verify module interaction documentation is accurate

   **Agent Documentation Verification**:
   - List all agents in `.claude/agents/` directory
   - Check if CLAUDE.md documents available agents
   - Verify agent descriptions match their actual capabilities
   - Document agent delegation patterns (which agents call others)
   - Include agent specializations and when to use each

   **Dependency Verification**:
   - Check Cargo.toml for dependency changes
   - Verify version numbers are current
   - Ensure new dependencies are documented with their purpose
   - Remove documentation for deleted dependencies

   **Command Documentation**:
   - Verify all CLI commands are documented
   - Check command options and flags match implementation
   - Ensure command descriptions are accurate
   - Update example usage if syntax changed

   **Testing Documentation**:
   - Verify test coverage targets are realistic
   - Check if testing patterns documentation matches actual tests
   - Ensure CI/CD matrix information is current
   - Update test command examples if changed

5. Based on the changes, determine what CLAUDE.md updates are needed:

   **Types of updates to make**:
   - Add new modules to project structure
   - Update module responsibilities if refactored
   - Document available Claude Code agents and their roles
   - Document new architectural decisions
   - Add new dependencies with explanations
   - Update testing requirements or patterns
   - Document new security considerations
   - Update development workflow instructions
   - Fix outdated implementation details
   - Add new configuration format examples
   - Update error handling patterns
   - Add or update agent documentation section

6. Apply updates based on mode:

   **Check-only mode (--check-only)**:
   - Report all architectural discrepancies found
   - List modules not documented or incorrectly described
   - Show dependencies missing from documentation
   - Identify outdated implementation details
   - Provide suggested updates without applying them

   **Auto-update mode (--auto-update or default)**:
   - Update module structure to match actual codebase
   - Synchronize dependency list with Cargo.toml
   - Update command documentation to match CLI
   - Fix implementation detail inaccuracies
   - Add documentation for new architectural decisions
   - Preserve existing valuable context and lessons learned

7. Maintain CLAUDE.md quality and purpose:
   - **CRITICAL**: Keep file under 20,000 characters total
   - Keep focus on helping AI assistants understand the codebase
   - Preserve "Lessons Learned" and "Design Decisions" sections
   - Maintain detailed explanations of complex algorithms
   - Keep security rules prominent and clear
   - Ensure cross-platform considerations are documented
   - Don't remove historical context that explains "why"
   - If file exceeds 20k characters, prioritize removing:
     * Verbose examples (use concise versions)
     * Redundant information covered in other docs
     * Overly detailed dependency lists
     * Long code examples (reference files instead)

8. Special sections in CLAUDE.md to verify:

   **Available Claude Code Agents**:
   - Document all agents in `.claude/agents/` directory
   - Include agent descriptions and specializations
   - Document delegation patterns between agents
   - Specify when to use each agent
   - Example format:
     * `rust-expert-standard`/`rust-expert-advanced`: Rust development and architecture
     * `rust-linting-standard`/`rust-linting-advanced`: Code formatting and linting
     * `rust-doc-standard`/`rust-doc-advanced`: Documentation and docstrings
     * `rust-test-standard`/`rust-test-advanced`: Test fixes and test infrastructure
     * `rust-troubleshooter-standard`/`rust-troubleshooter-advanced`: Debugging and troubleshooting

   **Implementation Lessons Learned**:
   - Keep valuable insights from development
   - Add new lessons from recent changes
   - Don't remove unless obsolete

   **Design Decisions**:
   - Document new architectural choices
   - Explain rationale for major changes
   - Keep record of what worked well

   **Critical Testing Requirements**:
   - Verify environment variable handling rules
   - Check cache directory isolation requirements
   - Ensure parallel test safety guidelines are current

   **Security Rules**:
   - Keep all security validations documented
   - Add new security measures implemented
   - Ensure credential handling rules are clear

9. Cross-reference with other documentation:
   - Ensure CLAUDE.md doesn't contradict README.md
   - Verify manifest format examples match actual implementation
   - Check that build commands work as documented
   - Validate that module descriptions align with code comments

10. Final character count check:
   - After all edits, check the file size with `wc -c CLAUDE.md`
   - If over 20,000 characters, further condense:
     * Remove verbose sections
     * Use bullet points instead of paragraphs
     * Reference other docs instead of duplicating content
   - Target: Keep under 20,000 characters while maintaining essential information

Examples of changes requiring CLAUDE.md updates:
- Adding new `src/` module → Update Project Structure and Module Structure
- Adding new dependency → Update Key Dependencies with purpose
- Refactoring module responsibilities → Update module descriptions
- Adding new CLI command → Update Core Commands section
- Adding new agent in `.claude/agents/` → Update Available Claude Code Agents section
- Modifying agent capabilities → Update agent descriptions and delegation patterns
- Changing testing approach → Update Testing Strategy
- Implementing new security validation → Update Security Rules
- Discovering new cross-platform issue → Update platform considerations
- Learning from bug/issue → Add to Implementation Lessons Learned

Examples of usage:
- `/update-claude-md` - automatically update CLAUDE.md based on changes
- `/update-claude-md --check-only` - report what needs updating without changes
- `/update-claude-md --auto-update` - explicitly update CLAUDE.md (same as default)