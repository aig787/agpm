---
description: Execute a shared markdown prompt from .agpm/ directory
---

## Your task

Execute a shared markdown prompt from the .agpm/ directory.

**IMPORTANT**: You are being asked to directly perform the actions described in the prompt file - locate the prompt, read its contents, and execute its instructions yourself.

**IMPORTANT**: This command extends the shared base prompt. Read the complete command logic from:
- `.agpm/snippets/commands/execute.md`

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- First argument is the prompt name (without .md extension)
- Additional arguments are passed to customize prompt execution
- Pass parsed arguments to the sub-logic in `.agpm/snippets/commands/execute.md`

## Execution

Based on the parsed arguments:
- Locate and load the specified prompt file from `.opencode/` and `.agpm/snippets/prompts/` directories
- Execute the prompt instructions, incorporating any additional arguments
- Use Task tool for complex operations requiring specialized agents

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust prompt paths to search `.opencode/` and `.agpm/snippets/prompts/` directories
- Read the prompt file and implement its instructions directly
- Do NOT attempt to run external execute commands
