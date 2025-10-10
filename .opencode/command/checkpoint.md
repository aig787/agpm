---
description: Create or restore Git-based checkpoints for safe AI-assisted editing
---

## Your task

Create or restore Git-based checkpoints for safe AI-assisted editing.

**IMPORTANT**: You are being asked to directly perform these checkpoint actions - do NOT attempt to execute any external command or script. Implement the checkpoint logic yourself using the available tools.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/checkpoint.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for subcommands: `create`, `list`, `restore`, and their respective arguments
- Parse for flags: `--message`, `--force`, etc.
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/checkpoint.md`

## Execution

Based on the parsed arguments, execute the appropriate checkpoint operation:
- `create`: Create a new checkpoint with optional message
- `list`: List available checkpoints
- `restore [checkpoint]`: Restore a specific checkpoint or latest

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Focus on implementing the checkpoint functionality directly
- Do NOT attempt to run external checkpoint commands
