---
description: Fact-check all documentation files against the current codebase implementation
---

## Your task

Fact-check all documentation files against the current codebase implementation.

**IMPORTANT**: You are being asked to directly perform the fact-checking analysis - read the documentation, examine the code, and identify any discrepancies yourself.

**CRITICAL**: This requires LINE-BY-LINE verification, not high-level validation. Every specific claim in documentation must be precisely verified against the actual code implementation.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/fact-check-docs.md`

**KEY FOCUS AREAS** (Common sources of inaccuracies):
- Version numbers (Rust version, tool versions)
- Command syntax and subcommand names
- Dependency lists vs Cargo.toml
- File paths and module structure
- Configuration options and defaults

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--report-only`, `--fix`
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/fact-check-docs.md`

## Execution

Based on the parsed arguments:
- `--report-only`: Report inaccuracies without making changes (default)
- `--fix`: Fix any inaccuracies found in documentation
- Systematically verify all documentation files against current implementation

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
