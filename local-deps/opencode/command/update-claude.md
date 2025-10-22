---
description: Review changes and update CLAUDE.md and AGENTS.md to reflect current architecture and implementation
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/update-claude.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--check-only`, `--auto-update`

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Read and Edit tools to update documentation
- Focus on architectural changes and new features
