---
description: Create or restore Git-based checkpoints for safe AI-assisted editing
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/checkpoint.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for subcommands: `create`, `list`, `restore`, `clean`
- Parse for flags: `--message`, `--force`
- Extract checkpoint target for restore command

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Focus on implementing the checkpoint functionality directly
- Do NOT attempt to run external checkpoint commands
