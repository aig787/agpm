---
description: Perform comprehensive PR review for AGPM project
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/pr-self-review.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for review target: DIFF keyword for staged changes, commit hashes, branch names
- Parse for review scope: specific files, modules, or full review

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Focus on reviewing the actual changes in the repository
- Do NOT use gh CLI commands to create PRs
