---
description: Create well-formatted git commits following project conventions - supports single or multiple logically grouped commits
agpm:
  templating: true
dependencies:
  snippets:
      - name: base
        install: false
        path: ../../snippets/commands/commit.md
---

## Argument Parsing

Parse the arguments from the command invocation:
- Arguments received: $ARGUMENTS
- Parse for flags: `--multi`, `--multi=N`, `--no-attribution`, `--co-authored`, `--contributed`, `--include-untracked`
- Parse for paths: directory/file paths
- Parse for commit message: after `--` separator or as last argument

{{ agpm.deps.snippets.base.content }}

## Tool-Specific Notes

- This command is designed for OpenCode
- Adjust any tool-specific syntax as needed
- Use the Bash tool to run git commands directly
- Follow the commit message style from the repository
