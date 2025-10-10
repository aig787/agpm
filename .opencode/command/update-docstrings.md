---
description: Review code changes and ensure all related documentation is accurate and up-to-date
---

## Your task

Review code changes and ensure all related documentation is accurate and up-to-date.

**IMPORTANT**: You are being asked to directly update docstrings and documentation - examine the code changes and add/update documentation as needed.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/update-docstrings.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--check-only`, `--auto-update`, `--focus=<module>`
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/update-docstrings.md`

## Execution

Based on the parsed arguments:
- `--check-only`: Report documentation issues without making changes
- `--auto-update`: Update documentation to match code changes (default)
- `--focus=<module>`: Focus on specific module (e.g., cli, resolver, source)
- Use Task tool to delegate to specialized documentation agents

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Read and Edit tools to add/update docstrings
- Focus on public APIs and complex functions
