---
description: Squash commits between two hashes into one, optionally regrouping into logical commits, or restore from a previous squash
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/squash.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--restore`, `--regroup`
- Parse for commit hashes: `from` and `to` (required for squash mode)
- Parse for reflog entries (restore mode)

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run git rebase and related commands
- Be careful with force pushes if needed
