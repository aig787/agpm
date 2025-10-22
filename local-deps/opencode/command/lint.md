---
description: Run code quality checks (formatting, linting, documentation)
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/lint.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--fix`, `--check`, `--all`, `--doc`

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run cargo fmt, cargo clippy, etc.
- Report any issues found and suggest fixes
