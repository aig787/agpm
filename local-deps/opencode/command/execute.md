---
description: Execute a shared markdown prompt from .agpm/ directory
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/execute.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- First argument is the prompt name (without .md extension)
- Additional arguments are passed to customize prompt execution

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust prompt paths to search `.opencode/` and `.agpm/snippets/prompts/` directories
- Read the prompt file and implement its instructions directly
- Do NOT attempt to run external execute commands
