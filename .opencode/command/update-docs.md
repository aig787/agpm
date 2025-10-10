---
description: Review changes and update README.md to stay current with implementation
---

## Your task

Review changes and update README.md to stay current with implementation.

**IMPORTANT**: You are being asked to directly update the README.md file - examine the codebase and ensure the documentation accurately reflects the current state.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/update-docs.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--check-only`, `--auto-update`
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/update-docs.md`

## Execution

Based on the parsed arguments:
- `--check-only`: Report what needs updating without making changes
- `--auto-update`: Make necessary updates to README.md (default)
- Focus on user-facing changes and installation instructions
- Use Task tool for comprehensive documentation updates

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Read and Edit tools to update README.md
- Focus on user-facing changes and installation instructions
