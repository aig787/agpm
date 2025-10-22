---
description: Review changes and update README.md to stay current with implementation
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/update-docs.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--check-only`, `--auto-update`

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Read and Edit tools to update README.md
- Focus on user-facing changes and installation instructions
