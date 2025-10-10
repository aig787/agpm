---
description: Review changes and update CLAUDE.md and AGENTS.md to reflect current architecture and implementation
---

## Your task

Review changes and update CLAUDE.md and AGENTS.md to reflect current architecture and implementation.

**IMPORTANT**: You are being asked to directly update these documentation files - analyze the codebase and ensure the documentation accurately reflects the current state.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/update-claude.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--check-only`, `--auto-update`
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/update-claude.md`

## Execution

Based on the parsed arguments:
- `--check-only`: Report what needs updating without making changes
- `--auto-update`: Make necessary updates to CLAUDE.md and AGENTS.md (default)
- Ensure both files remain under 20,000 characters total
- Use Task tool for complex architectural documentation updates

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Read and Edit tools to update documentation
- Focus on architectural changes and new features
