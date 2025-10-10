---
description: Automatically create a GitHub pull request with a well-formatted title and description
---

## Your task

Automatically create a GitHub pull request with a well-formatted title and description.

**IMPORTANT**: You are being asked to directly create a pull request - analyze the changes, craft an appropriate title and description, and use the gh CLI to create the PR.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/gh-pr-create.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--base <branch>`, `--draft`
- Parse for optional PR title (after flags)
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/gh-pr-create.md`

## Execution

Based on the parsed arguments:
- Verify prerequisites (uncommitted changes, remote branch, gh CLI)
- Determine base branch (from `--base` flag or auto-detection)
- Create draft or regular PR based on `--draft` flag
- Use provided title or generate one automatically

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run gh commands directly
- Stage changes if needed before creating the PR
