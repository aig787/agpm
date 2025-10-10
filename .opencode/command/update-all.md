---
description: Update all project documentation in parallel
---

## Your task

Update all project documentation in parallel.

**IMPORTANT**: You are being asked to directly update all documentation - examine the codebase and ensure all documentation files are current and accurate.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/update-all.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- This command doesn't use specific flags but passes any arguments to sub-commands
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/update-all.md`

## Execution

Execute three documentation update tasks in parallel:
- `update-docstrings`: Review and update Rust docstrings
- `update-docs`: Update project documentation files (README.md, docs/)
- `update-claude`: Update CLAUDE.md and AGENTS.md
- Use Task tool to run specialized documentation agents in parallel

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Task tool to run multiple documentation updates in parallel
- Update README.md, CLAUDE.md, AGENTS.md, and other docs
