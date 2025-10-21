---
description: Automatically create a GitHub pull request with a well-formatted title and description
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/gh-pr-create.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--base <branch>`, `--draft`
- Parse for optional PR title (after flags)

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run gh commands directly
- Stage changes if needed before creating the PR
