---
description: Update all project documentation in parallel
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/update-all.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- This command doesn't use specific flags but passes any arguments to sub-commands

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Task tool to run multiple documentation updates in parallel
- Update README.md, CLAUDE.md, AGENTS.md, and other docs
