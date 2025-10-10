---
description: Squash commits between two hashes into one, optionally regrouping into logical commits, or restore from a previous squash
---

## Your task

Squash commits between two hashes into one, optionally regrouping into logical commits, or restore from a previous squash.

**IMPORTANT**: You are being asked to directly perform git squash operations - use git commands to rewrite history as requested.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/squash.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--restore`, `--regroup`
- Parse for commit hashes: `from` and `to` (required for squash mode)
- Parse for reflog entries (restore mode)
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/squash.md`

## Execution

Based on the parsed arguments:
- `--restore [entry]`: Restore from a previous squash operation
- `from to [--regroup]`: Squash commits between hashes with optional regrouping
- Validate inputs and execute appropriate git operations

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run git rebase and related commands
- Be careful with force pushes if needed
