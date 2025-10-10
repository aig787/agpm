---
description: Run code quality checks (formatting, linting, documentation)
---

## Your task

Run code quality checks including formatting, linting, and documentation.

**IMPORTANT**: You are being asked to directly perform these quality checks - run the appropriate commands and report the results.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/lint.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--fix`, `--check`, `--all`, `--doc`
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/lint.md`

## Execution

Based on the parsed arguments, execute the appropriate logic from the sub-command file:
- If `--fix`: Use the two-phase automatic escalation strategy
- If `--check`: Run in CI mode with strict validation  
- If no arguments: Run standard checks (fmt, clippy, doc)
- etc.

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run cargo fmt, cargo clippy, etc.
- Report any issues found and suggest fixes
