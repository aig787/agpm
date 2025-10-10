---
description: Fact-check all documentation files against the current codebase implementation
---

## Your task

Fact-check all documentation files against the current codebase implementation.

**IMPORTANT**: You are being asked to directly perform the fact-checking analysis - read the documentation, examine the code, and identify any discrepancies yourself.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/fact-check-docs.md`

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
- Use Read and Grep tools to examine documentation and code
- Generate a report of any inconsistencies found
