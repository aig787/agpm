---
description: Review code changes and ensure all related documentation is accurate and up-to-date
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/update-docstrings.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--check-only`, `--auto-update`, `--focus=<module>`

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use Read and Edit tools to add/update docstrings
- Focus on public APIs and complex functions
